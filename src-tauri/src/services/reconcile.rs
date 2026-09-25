//! Diffs two [`Config`]s into a plan of webview operations (design.md
//! §2.2.5): which services to create, destroy, recreate or update in
//! place, and whether the shell needs a `services-changed` event.
//!
//! Pure: no Tauri, no I/O, no [`crate::host::WebviewHost`] calls. [`diff`]
//! only looks at `Config.services`; a change to `[settings]` alone (with
//! `services` otherwise identical) produces an empty plan (design.md
//! §2.2.5's settings-only case is outside this module's per-service
//! table).

use std::collections::HashSet;

use crate::config::{Config, ServiceId};

/// One webview-affecting change to apply, from [`diff`]ing an old and new
/// [`Config`] (design.md §2.2.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebviewOp {
    /// A service id present in the new config but not the old one.
    Create(ServiceId),
    /// A service id present in the old config but not the new one.
    Destroy(ServiceId),
    /// A service kept its id, but its `url` or `profile` changed —
    /// destroy the old webview and create a fresh one.
    Recreate(ServiceId),
    /// A service kept its id, `url` and `profile`, but its `name`, `icon`
    /// or `notifications` changed — no webview operation, only sidebar
    /// metadata.
    UpdateInPlace(ServiceId),
}

/// The result of [`diff`]ing an old and new [`Config`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    /// Ops in a deterministic order: every [`WebviewOp::Destroy`] (old
    /// order), then every [`WebviewOp::Create`] (new order), then every
    /// [`WebviewOp::Recreate`]/[`WebviewOp::UpdateInPlace`] for a service
    /// kept in both (new order).
    pub ops: Vec<WebviewOp>,
    /// Whether `services-changed` should be emitted to the shell: true
    /// whenever `new.services` differs from `old.services` in any way —
    /// an add, a remove, a field change, or a pure reorder — false when
    /// the two are identical (e.g. a settings-only edit).
    pub services_changed: bool,
}

/// Diffs `old` against `new` (design.md §2.2.5). Only `services` is
/// compared; `[settings]` changes never appear in the returned [`Plan`].
pub fn diff(old: &Config, new: &Config) -> Plan {
    let old_ids: HashSet<&ServiceId> = old.services.iter().map(|s| &s.id).collect();
    let new_ids: HashSet<&ServiceId> = new.services.iter().map(|s| &s.id).collect();

    let mut ops = Vec::new();

    for service in &old.services {
        if !new_ids.contains(&service.id) {
            ops.push(WebviewOp::Destroy(service.id.clone()));
        }
    }

    for service in &new.services {
        if !old_ids.contains(&service.id) {
            ops.push(WebviewOp::Create(service.id.clone()));
        }
    }

    for new_service in &new.services {
        let Some(old_service) = old.services.iter().find(|s| s.id == new_service.id) else {
            continue;
        };
        if old_service.url != new_service.url || old_service.profile != new_service.profile {
            ops.push(WebviewOp::Recreate(new_service.id.clone()));
        } else if old_service.name != new_service.name
            || old_service.icon != new_service.icon
            || old_service.notifications != new_service.notifications
        {
            ops.push(WebviewOp::UpdateInPlace(new_service.id.clone()));
        }
    }

    Plan {
        ops,
        services_changed: old.services != new.services,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{IconSource, ProfileName, ServiceConfig, Settings};
    use url::Url;

    fn settings() -> Settings {
        Settings {
            reconcile_interval_seconds: 60,
            notifications: true,
            notification_batch_threshold: 5,
            badge_sidebar: true,
        }
    }

    fn id(value: &str) -> ServiceId {
        ServiceId::new(value).expect("valid id")
    }

    fn profile(value: &str) -> ProfileName {
        ProfileName::new(value).expect("valid profile")
    }

    fn service(id_value: &str, name: &str, url: &str, profile_value: &str) -> ServiceConfig {
        ServiceConfig {
            id: id(id_value),
            name: name.to_string(),
            url: Url::parse(url).expect("valid url"),
            profile: profile(profile_value),
            notifications: true,
            icon: IconSource::Favicon,
        }
    }

    fn config(services: Vec<ServiceConfig>) -> Config {
        Config {
            version: 1,
            settings: settings(),
            services,
        }
    }

    #[test]
    fn identical_configs_produce_an_empty_plan() {
        let cfg = config(vec![service(
            "alpha",
            "Alpha",
            "https://alpha.example.com/",
            "default",
        )]);
        let plan = diff(&cfg, &cfg);
        assert_eq!(plan, Plan::default());
    }

    #[test]
    fn added_service_creates_and_changes() {
        let old = config(vec![]);
        let new = config(vec![service(
            "alpha",
            "Alpha",
            "https://alpha.example.com/",
            "default",
        )]);
        let plan = diff(&old, &new);
        assert_eq!(plan.ops, vec![WebviewOp::Create(id("alpha"))]);
        assert!(plan.services_changed);
    }

    #[test]
    fn removed_service_destroys_and_changes() {
        let old = config(vec![service(
            "alpha",
            "Alpha",
            "https://alpha.example.com/",
            "default",
        )]);
        let new = config(vec![]);
        let plan = diff(&old, &new);
        assert_eq!(plan.ops, vec![WebviewOp::Destroy(id("alpha"))]);
        assert!(plan.services_changed);
    }

    #[test]
    fn url_change_recreates() {
        let old = config(vec![service(
            "alpha",
            "Alpha",
            "https://alpha.example.com/",
            "default",
        )]);
        let new = config(vec![service(
            "alpha",
            "Alpha",
            "https://alpha2.example.com/",
            "default",
        )]);
        let plan = diff(&old, &new);
        assert_eq!(plan.ops, vec![WebviewOp::Recreate(id("alpha"))]);
        assert!(plan.services_changed);
    }

    #[test]
    fn profile_change_recreates() {
        let old = config(vec![service(
            "alpha",
            "Alpha",
            "https://alpha.example.com/",
            "default",
        )]);
        let new = config(vec![service(
            "alpha",
            "Alpha",
            "https://alpha.example.com/",
            "isolated",
        )]);
        let plan = diff(&old, &new);
        assert_eq!(plan.ops, vec![WebviewOp::Recreate(id("alpha"))]);
        assert!(plan.services_changed);
    }

    #[test]
    fn name_change_updates_in_place() {
        let old = config(vec![service(
            "alpha",
            "Alpha",
            "https://alpha.example.com/",
            "default",
        )]);
        let new = config(vec![service(
            "alpha",
            "Alpha Prime",
            "https://alpha.example.com/",
            "default",
        )]);
        let plan = diff(&old, &new);
        assert_eq!(plan.ops, vec![WebviewOp::UpdateInPlace(id("alpha"))]);
        assert!(plan.services_changed);
    }

    #[test]
    fn icon_change_updates_in_place() {
        let mut old_service = service("alpha", "Alpha", "https://alpha.example.com/", "default");
        let mut new_service = old_service.clone();
        old_service.icon = IconSource::Favicon;
        new_service.icon =
            IconSource::Url(Url::parse("https://alpha.example.com/icon.png").unwrap());

        let old = config(vec![old_service]);
        let new = config(vec![new_service]);
        let plan = diff(&old, &new);
        assert_eq!(plan.ops, vec![WebviewOp::UpdateInPlace(id("alpha"))]);
        assert!(plan.services_changed);
    }

    #[test]
    fn notifications_change_updates_in_place() {
        let mut old_service = service("alpha", "Alpha", "https://alpha.example.com/", "default");
        let mut new_service = old_service.clone();
        old_service.notifications = true;
        new_service.notifications = false;

        let old = config(vec![old_service]);
        let new = config(vec![new_service]);
        let plan = diff(&old, &new);
        assert_eq!(plan.ops, vec![WebviewOp::UpdateInPlace(id("alpha"))]);
        assert!(plan.services_changed);
    }

    #[test]
    fn reorder_only_produces_no_ops_but_still_changes() {
        let alpha = service("alpha", "Alpha", "https://alpha.example.com/", "default");
        let beta = service("beta", "Beta", "https://beta.example.com/", "default");
        let old = config(vec![alpha.clone(), beta.clone()]);
        let new = config(vec![beta, alpha]);

        let plan = diff(&old, &new);
        assert!(plan.ops.is_empty());
        assert!(plan.services_changed);
    }

    #[test]
    fn settings_only_change_produces_an_empty_plan() {
        let alpha = service("alpha", "Alpha", "https://alpha.example.com/", "default");
        let old = config(vec![alpha.clone()]);
        let mut new = config(vec![alpha]);
        new.settings.notifications = false;

        let plan = diff(&old, &new);
        assert_eq!(plan, Plan::default());
    }

    #[test]
    fn combined_add_remove_and_field_change_in_one_edit() {
        let alpha = service("alpha", "Alpha", "https://alpha.example.com/", "default");
        let beta = service("beta", "Beta", "https://beta.example.com/", "default");
        let mut beta_renamed = beta.clone();
        beta_renamed.name = "Beta Prime".to_string();
        let gamma = service("gamma", "Gamma", "https://gamma.example.com/", "default");

        let old = config(vec![alpha.clone(), beta]);
        let new = config(vec![beta_renamed, gamma.clone()]);

        let plan = diff(&old, &new);
        assert_eq!(
            plan.ops,
            vec![
                WebviewOp::Destroy(id("alpha")),
                WebviewOp::Create(id("gamma")),
                WebviewOp::UpdateInPlace(id("beta")),
            ]
        );
        assert!(plan.services_changed);
    }
}
