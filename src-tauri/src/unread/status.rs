//! Pure status types and transition helpers (design.md §2.2.7). Nothing
//! here touches Tauri or [`crate::services::ServiceManager`]'s locks —
//! [`store::StatusStore`] (the state machine that actually holds and
//! mutates per-service statuses) is the only consumer, and it stays just
//! as Tauri-free.
//!
//! `ServiceStatus` serializes as the JSON shape design.md §3.2 specifies:
//! `{ kind: "loading" | "ok" | "needsAttention" | "stale", count?,
//! reason? }`, via serde's internal tagging (`tag = "kind"`) plus
//! `rename_all = "camelCase"`, which turns each PascalCase variant name
//! into exactly the lowercase/camelCase `kind` value the shell expects.

use serde::Serialize;
use url::{Origin, Url};

use crate::config::ServiceId;

/// A service's current unread/attention state (design.md §2.2.7).
///
/// `Stale` is defined here only — its producer is liveness (design.md
/// §2.2.8), a later task (3.2); nothing in this module ever constructs
/// it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ServiceStatus {
    /// The webview was created (or is being recreated) but no report has
    /// arrived yet.
    Loading,
    Ok {
        count: u32,
    },
    NeedsAttention {
        reason: AttentionReason,
    },
    Stale,
}

impl ServiceStatus {
    /// The status a validated report (design.md §2.2.6) transitions to:
    /// `Some(n)` → `Ok`, `None` → `NeedsAttention(ReportedNone)`.
    pub fn from_report(count: Option<u32>) -> Self {
        match count {
            Some(count) => ServiceStatus::Ok { count },
            None => ServiceStatus::NeedsAttention {
                reason: AttentionReason::ReportedNone,
            },
        }
    }

    /// The status a webview-creation failure transitions to (design.md
    /// §5.1's `NeedsAttention(CreateFailed)`).
    pub fn create_failed() -> Self {
        ServiceStatus::NeedsAttention {
            reason: AttentionReason::CreateFailed,
        }
    }

    /// The status an off-origin page load transitions to (design.md
    /// §2.2.7).
    pub fn off_origin() -> Self {
        ServiceStatus::NeedsAttention {
            reason: AttentionReason::OffOrigin,
        }
    }
}

/// Why a service is in [`ServiceStatus::NeedsAttention`] (design.md
/// §2.2.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AttentionReason {
    /// The agent reported `count: null`.
    ReportedNone,
    /// `on_page_load` observed the webview on a different origin than
    /// the service's configured URL.
    OffOrigin,
    /// The webview (or its profile) failed to create.
    CreateFailed,
}

/// The `status-changed { serviceId, status }` event payload (design.md
/// §2.2.7), emitted to the `shell` webview only when a [`super::store::
/// StatusStore`] mutation actually changed a service's status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusChanged {
    pub service_id: ServiceId,
    pub status: ServiceStatus,
}

/// Whether `page_url` counts as "off origin" for design.md §2.2.7's
/// `on_page_load` rule: neither the same origin as `service_origin`, nor
/// an `about:` page (e.g. `about:blank`, whose own origin is opaque and
/// so never equals any real origin by comparison alone — design.md
/// explicitly carves this case out, so it is checked by scheme instead).
pub fn is_off_origin(page_url: &Url, service_origin: &Origin) -> bool {
    page_url.scheme() != "about" && &page_url.origin() != service_origin
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin(url: &str) -> Origin {
        Url::parse(url).expect("valid url").origin()
    }

    #[test]
    fn from_report_some_is_ok() {
        assert_eq!(
            ServiceStatus::from_report(Some(3)),
            ServiceStatus::Ok { count: 3 }
        );
    }

    #[test]
    fn from_report_none_is_needs_attention_reported_none() {
        assert_eq!(
            ServiceStatus::from_report(None),
            ServiceStatus::NeedsAttention {
                reason: AttentionReason::ReportedNone
            }
        );
    }

    #[test]
    fn is_off_origin_true_for_a_different_origin() {
        let service = origin("https://mail.google.com");
        let page = Url::parse("https://evil.example.com/phish").expect("valid url");
        assert!(is_off_origin(&page, &service));
    }

    #[test]
    fn is_off_origin_false_for_the_same_origin() {
        let service = origin("https://mail.google.com");
        let page = Url::parse("https://mail.google.com/mail/u/0/").expect("valid url");
        assert!(!is_off_origin(&page, &service));
    }

    #[test]
    fn is_off_origin_false_for_an_about_page() {
        let service = origin("https://mail.google.com");
        let page = Url::parse("about:blank").expect("valid url");
        assert!(!is_off_origin(&page, &service));
    }

    #[test]
    fn status_serializes_with_the_designed_json_shape() {
        let ok = serde_json::to_value(ServiceStatus::Ok { count: 5 }).expect("serialize");
        assert_eq!(ok, serde_json::json!({ "kind": "ok", "count": 5 }));

        let loading = serde_json::to_value(ServiceStatus::Loading).expect("serialize");
        assert_eq!(loading, serde_json::json!({ "kind": "loading" }));

        let stale = serde_json::to_value(ServiceStatus::Stale).expect("serialize");
        assert_eq!(stale, serde_json::json!({ "kind": "stale" }));

        let needs_attention = serde_json::to_value(ServiceStatus::NeedsAttention {
            reason: AttentionReason::OffOrigin,
        })
        .expect("serialize");
        assert_eq!(
            needs_attention,
            serde_json::json!({ "kind": "needsAttention", "reason": "offOrigin" })
        );
    }

    #[test]
    fn attention_reason_serializes_as_camel_case_strings() {
        assert_eq!(
            serde_json::to_value(AttentionReason::ReportedNone).expect("serialize"),
            serde_json::json!("reportedNone")
        );
        assert_eq!(
            serde_json::to_value(AttentionReason::OffOrigin).expect("serialize"),
            serde_json::json!("offOrigin")
        );
        assert_eq!(
            serde_json::to_value(AttentionReason::CreateFailed).expect("serialize"),
            serde_json::json!("createFailed")
        );
    }

    #[test]
    fn status_changed_serializes_service_id_and_status() {
        let changed = StatusChanged {
            service_id: ServiceId::new("gmail-personal").expect("valid id"),
            status: ServiceStatus::Ok { count: 2 },
        };
        let value = serde_json::to_value(&changed).expect("serialize");
        assert_eq!(
            value,
            serde_json::json!({ "serviceId": "gmail-personal", "status": { "kind": "ok", "count": 2 } })
        );
    }
}
