//! `get_snapshot`'s response DTO and its pure assembly (Task 1.9;
//! design.md §2.2.12: `{ settings, services[], statuses, sidebarWidth,
//! configError? }`).
//!
//! Only this DTO's own top-level keys are camelCase (`sidebarWidth`,
//! `configError`) — the nested `settings`/`services`/`configError` values
//! reuse [`Settings`]/[`ServiceConfig`]/[`ConfigError`] exactly as they
//! already serialize elsewhere (snake_case field names), per the
//! implementation plan's instruction not to touch those existing models.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::config::{ConfigError, ServiceConfig, ServiceId, Settings};

/// `get_snapshot`'s response (design.md §2.2.12).
///
/// `settings` is `None` exactly when `config_error` is `Some` — this app
/// never fabricates a default `Settings` to paper over a failed load
/// (project rule: no fallback defaults; design.md §5.1: "no defaults are
/// substituted"). `statuses` is always an empty map for now: the
/// `unread`/`ServiceStatus` store this field will report from is Task
/// 2.3's; until then every service is simply absent from it, which the
/// shell's `Loading` badge (design.md §2.2.13) already treats as the
/// no-report-yet case.
#[derive(Debug, Serialize)]
pub struct SnapshotDto {
    pub settings: Option<Settings>,
    pub services: Vec<ServiceConfig>,
    /// Always empty until Task 2.3 gives it a real `ServiceStatus` value
    /// type; kept keyed by [`ServiceId`] now so that task only has to
    /// change the map's value type, not its shape.
    pub statuses: BTreeMap<ServiceId, ()>,
    #[serde(rename = "sidebarWidth")]
    pub sidebar_width: f64,
    #[serde(rename = "configError", skip_serializing_if = "Option::is_none")]
    pub config_error: Option<ConfigError>,
}

/// Pure assembly of [`SnapshotDto`] from its parts — [`crate::services::
/// ServiceManager::snapshot`] supplies `settings`/`services`/`config_error`,
/// and the `get_snapshot` command supplies `sidebar_width` (`host::layout::
/// SIDEBAR_WIDTH`, design.md §2.2.4) — kept separate from both so it is
/// unit-testable without a running `ServiceManager` or Tauri app.
pub fn build_snapshot(
    settings: Option<Settings>,
    services: Vec<ServiceConfig>,
    config_error: Option<ConfigError>,
    sidebar_width: f64,
) -> SnapshotDto {
    SnapshotDto {
        settings,
        services,
        statuses: BTreeMap::new(),
        sidebar_width,
        config_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{IconSource, ProfileName};
    use serde_json::json;
    use std::path::PathBuf;

    fn settings() -> Settings {
        Settings {
            reconcile_interval_seconds: 60,
            notifications: true,
            notification_batch_threshold: 5,
            badge_sidebar: true,
        }
    }

    fn service() -> ServiceConfig {
        ServiceConfig {
            id: ServiceId::new("gmail").expect("valid id"),
            name: "Gmail".to_string(),
            url: url::Url::parse("https://mail.example.com/").expect("valid url"),
            profile: ProfileName::new("default").expect("valid profile"),
            notifications: true,
            icon: IconSource::Favicon,
        }
    }

    #[test]
    fn success_snapshot_has_settings_and_services_and_no_config_error() {
        let dto = build_snapshot(Some(settings()), vec![service()], None, 64.0);
        let value = serde_json::to_value(&dto).expect("serialize");

        assert_eq!(value["sidebarWidth"], json!(64.0));
        assert_eq!(value["statuses"], json!({}));
        assert!(value.get("configError").is_none());
        assert_eq!(value["settings"]["reconcile_interval_seconds"], json!(60));
        assert_eq!(value["services"][0]["id"], json!("gmail"));
    }

    #[test]
    fn failed_snapshot_has_null_settings_empty_services_and_a_config_error() {
        let error = ConfigError {
            file: PathBuf::from("/tmp/eluma/config.toml"),
            key: Some("services[0].url".to_string()),
            reason: "invalid scheme".to_string(),
        };
        let dto = build_snapshot(None, Vec::new(), Some(error), 64.0);
        let value = serde_json::to_value(&dto).expect("serialize");

        assert_eq!(value["settings"], json!(null));
        assert_eq!(value["services"], json!([]));
        assert_eq!(
            value["configError"]["file"],
            json!("/tmp/eluma/config.toml")
        );
        assert_eq!(value["configError"]["key"], json!("services[0].url"));
        assert_eq!(value["configError"]["reason"], json!("invalid scheme"));
    }

    #[test]
    fn config_error_is_omitted_entirely_rather_than_serialized_as_null() {
        let dto = build_snapshot(Some(settings()), Vec::new(), None, 64.0);
        let value = serde_json::to_value(&dto).expect("serialize");
        let obj = value.as_object().expect("object");
        assert!(!obj.contains_key("configError"));
    }
}
