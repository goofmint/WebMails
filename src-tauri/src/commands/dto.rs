//! Input DTOs for the `commands` module (Task 1.9), and their pure
//! conversions into the existing [`crate::config`] patch types.
//!
//! Every field here mirrors an existing typed field one-to-one (`name`,
//! `url`, `profile`, `notifications`, `icon` for a service;
//! `reconcile_interval_seconds`, `notifications`, `notification_batch_threshold`,
//! `badge_sidebar` for settings) — none are multi-word-renamed, per the
//! implementation plan's instruction to keep nested config field names as
//! they already are (only the wrapping snapshot DTO's own keys are
//! camelCase; see `super::snapshot`). Deliberately, **neither patch DTO has
//! an `id` field**: which service a patch applies to is always a separate
//! command parameter, never something the patch body can influence.

use serde::Deserialize;

use crate::config::{IconSource, ProfileName, ServicePatch, SettingsPatch};

/// The `patch` argument of the `update_service` command (design.md
/// §2.2.12). `None` fields are left untouched, exactly like
/// [`ServicePatch`] itself — this type exists only so the argument can be
/// deserialized directly from the command's JSON input; [`ServicePatch`]
/// itself carries no `serde` derive (it is never read from `config.toml`
/// directly — `toml_edit` builds it by hand in `config::store`).
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct ServicePatchDto {
    pub name: Option<String>,
    pub url: Option<url::Url>,
    pub profile: Option<ProfileName>,
    pub notifications: Option<bool>,
    pub icon: Option<IconSource>,
}

impl From<ServicePatchDto> for ServicePatch {
    fn from(dto: ServicePatchDto) -> Self {
        ServicePatch {
            name: dto.name,
            url: dto.url,
            profile: dto.profile,
            notifications: dto.notifications,
            icon: dto.icon,
        }
    }
}

/// The `patch` argument of the `update_settings` command (design.md
/// §2.2.12). `None` fields are left untouched, exactly like
/// [`SettingsPatch`] itself.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct SettingsPatchDto {
    pub reconcile_interval_seconds: Option<u32>,
    pub notifications: Option<bool>,
    pub notification_batch_threshold: Option<u32>,
    pub badge_sidebar: Option<bool>,
}

impl From<SettingsPatchDto> for SettingsPatch {
    fn from(dto: SettingsPatchDto) -> Self {
        SettingsPatch {
            reconcile_interval_seconds: dto.reconcile_interval_seconds,
            notifications: dto.notifications,
            notification_batch_threshold: dto.notification_batch_threshold,
            badge_sidebar: dto.badge_sidebar,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(value: &str) -> ProfileName {
        ProfileName::new(value).expect("valid profile name")
    }

    // --- ServicePatchDto → ServicePatch ----------------------------------

    #[test]
    fn service_patch_dto_default_converts_to_an_all_none_patch() {
        let patch: ServicePatch = ServicePatchDto::default().into();
        assert_eq!(patch.name, None);
        assert_eq!(patch.url, None);
        assert_eq!(patch.profile, None);
        assert_eq!(patch.notifications, None);
        assert_eq!(patch.icon, None);
    }

    #[test]
    fn service_patch_dto_carries_every_field_through_unchanged() {
        let dto = ServicePatchDto {
            name: Some("Gmail Work".to_string()),
            url: Some(url::Url::parse("https://mail.example.com/").expect("valid url")),
            profile: Some(profile("isolated")),
            notifications: Some(false),
            icon: Some(IconSource::Favicon),
        };
        let patch: ServicePatch = dto.clone().into();
        assert_eq!(patch.name, dto.name);
        assert_eq!(patch.url, dto.url);
        assert_eq!(patch.profile, dto.profile);
        assert_eq!(patch.notifications, dto.notifications);
        assert_eq!(patch.icon, dto.icon);
    }

    #[test]
    fn service_patch_dto_deserializes_partial_json() {
        let dto: ServicePatchDto =
            serde_json::from_str(r#"{"name":"Renamed"}"#).expect("deserialize");
        assert_eq!(dto.name.as_deref(), Some("Renamed"));
        assert_eq!(dto.url, None);
        assert_eq!(dto.profile, None);
    }

    #[test]
    fn service_patch_dto_ignores_an_id_field_in_the_json_body() {
        // The DTO has no `id` field at all, so a patch body that (wrongly)
        // includes one has no way to influence which service is patched —
        // `serde`'s default behaviour (no `deny_unknown_fields`) just skips
        // it, and the rest of the patch still deserializes normally.
        let dto: ServicePatchDto =
            serde_json::from_str(r#"{"id":"sneaky","name":"Renamed"}"#).expect("deserialize");
        assert_eq!(dto.name.as_deref(), Some("Renamed"));
    }

    #[test]
    fn service_patch_dto_empty_json_object_deserializes_to_default() {
        let dto: ServicePatchDto = serde_json::from_str("{}").expect("deserialize");
        assert_eq!(dto, ServicePatchDto::default());
    }

    // --- SettingsPatchDto → SettingsPatch --------------------------------

    #[test]
    fn settings_patch_dto_default_converts_to_an_all_none_patch() {
        let patch: SettingsPatch = SettingsPatchDto::default().into();
        assert_eq!(patch.reconcile_interval_seconds, None);
        assert_eq!(patch.notifications, None);
        assert_eq!(patch.notification_batch_threshold, None);
        assert_eq!(patch.badge_sidebar, None);
    }

    #[test]
    fn settings_patch_dto_carries_every_field_through_unchanged() {
        let dto = SettingsPatchDto {
            reconcile_interval_seconds: Some(120),
            notifications: Some(false),
            notification_batch_threshold: Some(10),
            badge_sidebar: Some(false),
        };
        let patch: SettingsPatch = dto.clone().into();
        assert_eq!(
            patch.reconcile_interval_seconds,
            dto.reconcile_interval_seconds
        );
        assert_eq!(patch.notifications, dto.notifications);
        assert_eq!(
            patch.notification_batch_threshold,
            dto.notification_batch_threshold
        );
        assert_eq!(patch.badge_sidebar, dto.badge_sidebar);
    }

    #[test]
    fn settings_patch_dto_deserializes_partial_json() {
        // Nested config field names are kept as-is (snake_case), per the
        // implementation plan's instruction — no `rename_all = "camelCase"`
        // on this DTO, unlike the snapshot wrapper's own top-level keys.
        let dto: SettingsPatchDto =
            serde_json::from_str(r#"{"badge_sidebar":false}"#).expect("deserialize");
        assert_eq!(dto.badge_sidebar, Some(false));
        assert_eq!(dto.reconcile_interval_seconds, None);
    }
}
