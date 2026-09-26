//! `get_diagnostics`'s response DTO and its pure assembly (Task 3.3;
//! design.md §2.2.12: `{ services: [{ serviceId, name, status,
//! lastReportAgeMs, staleCount, lastStaleAt }] }`).
//!
//! Mirrors `commands::snapshot`'s split: [`build_diagnostics`] is a pure
//! function of already-resolved data (no lock, no Tauri, no clock read of
//! its own), unit-tested directly below; `get_diagnostics` (in
//! `commands::mod`) is the only production caller, and is the one place
//! that actually reads the clock and the liveness runtime's per-service
//! last-report timing.
//!
//! Only this DTO's own multi-word keys are camelCase-renamed
//! (`lastReportAgeMs`, `staleCount`, `lastStaleAt`, `serviceId`), the same
//! convention `SnapshotDto` uses; `status` reuses [`ServiceStatus`]'s own
//! existing `{ kind, count?, reason? }` serialization unchanged.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use crate::config::{ServiceConfig, ServiceId};
use crate::state::StalenessStats;
use crate::unread::ServiceStatus;

/// `get_diagnostics`'s response (design.md §2.2.12, §9.4).
#[derive(Debug, Serialize, PartialEq)]
pub struct DiagnosticsDto {
    pub services: Vec<ServiceDiagnosticDto>,
}

/// One row of the diagnostics table (design.md §2.2.13): a configured
/// service's name, current status, last-report age and staleness history.
#[derive(Debug, Serialize, PartialEq)]
pub struct ServiceDiagnosticDto {
    #[serde(rename = "serviceId")]
    pub service_id: ServiceId,
    pub name: String,
    pub status: ServiceStatus,
    /// Milliseconds since this service's last actual report, or `null` if
    /// it has never reported (design.md §2.2.2's "services never
    /// reported show null age" — no fallback to e.g. `0`).
    #[serde(rename = "lastReportAgeMs")]
    pub last_report_age_ms: Option<u64>,
    /// How many times this service has been marked `Stale` (design.md
    /// §9.4). `0` for a service with no staleness history yet.
    #[serde(rename = "staleCount")]
    pub stale_count: u32,
    /// Unix epoch ms of the last time this service was marked `Stale`, or
    /// `null` before its first stale episode (`StalenessStats::last_at`'s
    /// own doc has the full semantics).
    #[serde(rename = "lastStaleAt")]
    pub last_stale_at: Option<u64>,
}

/// Pure assembly of [`DiagnosticsDto`] from already-resolved parts:
///
/// - `services` — configured services in sidebar order (`ServiceManager::
///   diagnostics_snapshot`'s own order, unchanged).
/// - `statuses` — the `unread` status map; a service absent from it (no
///   webview created yet) defaults to `Loading` (design.md §2.2.7's own
///   no-report-yet meaning).
/// - `staleness` — `state.staleness`; a service absent from it defaults to
///   `{ count: 0, last_at: None }` (never having gone stale).
/// - `last_report_ms` — per-service last-actual-report time from the
///   liveness runtime (`LivenessMachine::last_real_report_ms`), never
///   `state.staleness` (that field means something different now — see
///   `StalenessStats::last_at`'s doc). A service absent here has never
///   reported.
/// - `now_ms` — the caller's already-resolved "now" (its own [`super::
///   super::liveness::Clock`] read), or `None` if that read failed. `None`
///   here forces every row's age to `None` too, rather than fabricating a
///   time (project rule: no fallback defaults).
pub fn build_diagnostics(
    services: &[ServiceConfig],
    statuses: &HashMap<ServiceId, ServiceStatus>,
    staleness: &BTreeMap<ServiceId, StalenessStats>,
    last_report_ms: &HashMap<ServiceId, u64>,
    now_ms: Option<u64>,
) -> DiagnosticsDto {
    let rows = services
        .iter()
        .map(|service| {
            let status = statuses
                .get(&service.id)
                .cloned()
                .unwrap_or(ServiceStatus::Loading);
            let stats = staleness.get(&service.id);
            let last_report_age_ms = match (now_ms, last_report_ms.get(&service.id)) {
                (Some(now_ms), Some(&last_ms)) => Some(now_ms.saturating_sub(last_ms)),
                _ => None,
            };
            ServiceDiagnosticDto {
                service_id: service.id.clone(),
                name: service.name.clone(),
                status,
                last_report_age_ms,
                stale_count: stats.map(|s| s.count).unwrap_or(0),
                last_stale_at: stats.and_then(|s| s.last_at),
            }
        })
        .collect();
    DiagnosticsDto { services: rows }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{IconSource, ProfileName};
    use serde_json::json;

    fn service(id: &str, name: &str) -> ServiceConfig {
        ServiceConfig {
            id: ServiceId::new(id).expect("valid id"),
            name: name.to_string(),
            url: url::Url::parse("https://example.com").expect("valid url"),
            profile: ProfileName::new("default").expect("valid profile"),
            notifications: true,
            icon: IconSource::Favicon,
        }
    }

    #[test]
    fn a_service_with_no_status_defaults_to_loading() {
        let services = vec![service("gmail", "Gmail")];
        let dto = build_diagnostics(
            &services,
            &HashMap::new(),
            &BTreeMap::new(),
            &HashMap::new(),
            Some(1_000),
        );
        assert_eq!(dto.services.len(), 1);
        assert_eq!(dto.services[0].status, ServiceStatus::Loading);
    }

    #[test]
    fn a_service_with_no_staleness_entry_defaults_to_zero_count_and_null_last_stale_at() {
        let services = vec![service("gmail", "Gmail")];
        let dto = build_diagnostics(
            &services,
            &HashMap::new(),
            &BTreeMap::new(),
            &HashMap::new(),
            Some(1_000),
        );
        assert_eq!(dto.services[0].stale_count, 0);
        assert_eq!(dto.services[0].last_stale_at, None);
    }

    #[test]
    fn a_service_that_never_reported_has_a_null_age_even_when_now_is_known() {
        let services = vec![service("gmail", "Gmail")];
        let dto = build_diagnostics(
            &services,
            &HashMap::new(),
            &BTreeMap::new(),
            &HashMap::new(), // no entry for "gmail": never reported
            Some(1_000),
        );
        assert_eq!(dto.services[0].last_report_age_ms, None);
    }

    #[test]
    fn a_service_that_reported_gets_a_null_age_when_now_is_unknown() {
        // The clock read failed (caller passes `None`): every age must be
        // `None`, never a fabricated value — even for a service that has
        // a last-report time on record.
        let services = vec![service("gmail", "Gmail")];
        let mut last_report_ms = HashMap::new();
        last_report_ms.insert(ServiceId::new("gmail").expect("valid id"), 500);

        let dto = build_diagnostics(
            &services,
            &HashMap::new(),
            &BTreeMap::new(),
            &last_report_ms,
            None,
        );
        assert_eq!(dto.services[0].last_report_age_ms, None);
    }

    #[test]
    fn a_service_that_reported_gets_the_elapsed_time_since_its_last_report() {
        let services = vec![service("gmail", "Gmail")];
        let mut last_report_ms = HashMap::new();
        last_report_ms.insert(ServiceId::new("gmail").expect("valid id"), 500);

        let dto = build_diagnostics(
            &services,
            &HashMap::new(),
            &BTreeMap::new(),
            &last_report_ms,
            Some(1_500),
        );
        assert_eq!(dto.services[0].last_report_age_ms, Some(1_000));
    }

    #[test]
    fn stale_count_and_last_stale_at_come_from_the_staleness_map_when_present() {
        let services = vec![service("gmail", "Gmail")];
        let mut staleness = BTreeMap::new();
        staleness.insert(
            ServiceId::new("gmail").expect("valid id"),
            StalenessStats {
                count: 3,
                last_at: Some(42),
            },
        );

        let dto = build_diagnostics(
            &services,
            &HashMap::new(),
            &staleness,
            &HashMap::new(),
            Some(1_000),
        );
        assert_eq!(dto.services[0].stale_count, 3);
        assert_eq!(dto.services[0].last_stale_at, Some(42));
    }

    #[test]
    fn rows_follow_the_given_service_order_not_map_iteration_order() {
        let services = vec![service("zeta", "Zeta"), service("alpha", "Alpha")];
        let dto = build_diagnostics(
            &services,
            &HashMap::new(),
            &BTreeMap::new(),
            &HashMap::new(),
            Some(1_000),
        );
        let ids: Vec<&str> = dto
            .services
            .iter()
            .map(|row| row.service_id.as_str())
            .collect();
        assert_eq!(ids, vec!["zeta", "alpha"]);
    }

    #[test]
    fn a_stale_service_keeps_its_real_status_not_the_loading_default() {
        let services = vec![service("gmail", "Gmail")];
        let mut statuses = HashMap::new();
        statuses.insert(
            ServiceId::new("gmail").expect("valid id"),
            ServiceStatus::Stale,
        );

        let dto = build_diagnostics(
            &services,
            &statuses,
            &BTreeMap::new(),
            &HashMap::new(),
            Some(1_000),
        );
        assert_eq!(dto.services[0].status, ServiceStatus::Stale);
    }

    #[test]
    fn serializes_with_the_designed_camel_case_shape() {
        let services = vec![service("gmail", "Gmail")];
        let mut staleness = BTreeMap::new();
        staleness.insert(
            ServiceId::new("gmail").expect("valid id"),
            StalenessStats {
                count: 2,
                last_at: Some(42),
            },
        );
        let mut last_report_ms = HashMap::new();
        last_report_ms.insert(ServiceId::new("gmail").expect("valid id"), 900);

        let dto = build_diagnostics(
            &services,
            &HashMap::new(),
            &staleness,
            &last_report_ms,
            Some(1_000),
        );
        let value = serde_json::to_value(&dto).expect("serialize");

        assert_eq!(
            value,
            json!({
                "services": [{
                    "serviceId": "gmail",
                    "name": "Gmail",
                    "status": { "kind": "loading" },
                    "lastReportAgeMs": 100,
                    "staleCount": 2,
                    "lastStaleAt": 42
                }]
            })
        );
    }

    #[test]
    fn empty_services_list_serializes_to_an_empty_array() {
        let dto = build_diagnostics(
            &[],
            &HashMap::new(),
            &BTreeMap::new(),
            &HashMap::new(),
            None,
        );
        let value = serde_json::to_value(&dto).expect("serialize");
        assert_eq!(value, json!({ "services": [] }));
    }
}
