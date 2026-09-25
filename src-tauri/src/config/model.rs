//! Typed configuration model (design.md §2.2.1, SPEC.md §6).
//!
//! Nothing in this module falls back to a default when a value is absent;
//! every field is populated by [`crate::config::parse`] from an explicit
//! key in `config.toml`, or by [`Config::initial`] from the fixed values
//! documented in SPEC.md §6.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize};
use url::Url;

use crate::error::AppError;

/// Character class shared by [`ServiceId`] and [`ProfileName`].
///
/// SPEC.md §6's example service ids (`gmail-personal`, `gmail-work`,
/// `icloud`, `outlook`) and SPEC.md §5's example profile names (`default`,
/// `isolated`, `work`) are all lowercase alphanumeric-with-hyphen tokens,
/// but neither section states a format explicitly. This pattern is
/// therefore an assumption, applied identically to both newtypes for
/// consistency, and bounded to 48 characters to keep ids usable as
/// filesystem-safe path components elsewhere in the app (e.g. `state.json`
/// keys, log file names).
const ID_PATTERN_DESCRIPTION: &str = "[a-z0-9-]{1,48}";

fn validate_id_chars(value: &str) -> Result<(), String> {
    let len = value.chars().count();
    if len == 0 || len > 48 {
        return Err(format!(
            "must match {ID_PATTERN_DESCRIPTION} (length {len} is out of range)"
        ));
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(format!(
            "must match {ID_PATTERN_DESCRIPTION} (found `{value}`)"
        ));
    }
    Ok(())
}

/// A service identifier: `[a-z0-9-]{1,48}`, unique within `config.toml`
/// (design.md §2.2.1).
#[derive(Serialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct ServiceId(String);

/// Deserialization (e.g. as a `state.json` map key) runs the same validation as [`ServiceId::new`].
impl<'de> Deserialize<'de> for ServiceId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

impl ServiceId {
    /// Validates `value` against the id character class.
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        validate_id_chars(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ServiceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A profile name (design.md §2.2.1, SPEC.md §5): `"default"`, `"isolated"`,
/// or an arbitrary named profile, all sharing the id character class (see
/// [`ID_PATTERN_DESCRIPTION`]).
#[derive(Serialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct ProfileName(String);

/// Deserialization (e.g. as a `state.json` map key) runs the same validation as [`ProfileName::new`].
impl<'de> Deserialize<'de> for ProfileName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

impl ProfileName {
    /// Validates `value` against the id character class.
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        validate_id_chars(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProfileName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Global settings (`[settings]`, SPEC.md §6). Every field is required.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Settings {
    pub reconcile_interval_seconds: u32,
    pub notifications: bool,
    pub notification_batch_threshold: u32,
    pub badge_sidebar: bool,
}

/// Where a service's sidebar icon comes from (`[[services]].icon`,
/// SPEC.md §6).
///
/// Serialized as an adjacently tagged value with `source`/`value` fields
/// and lowercase variant names, matching `icon = { source = "favicon" }`
/// and `icon = { source = "file", value = "icons/icloud.png" }` in
/// SPEC.md §6. SPEC.md does not state a format for the `url` source's
/// value; this module assumes it must be an http(s) URL, the same
/// constraint as a service's own `url` (§6), since both are fetched by a
/// webview.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(tag = "source", content = "value", rename_all = "lowercase")]
pub enum IconSource {
    /// Use the service's own favicon.
    Favicon,
    /// A local file path, relative to an icons directory.
    File(PathBuf),
    /// An http(s) URL to fetch the icon from.
    Url(Url),
}

/// One sidebar entry (`[[services]]`, SPEC.md §6). Every field is
/// required.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct ServiceConfig {
    pub id: ServiceId,
    pub name: String,
    pub url: Url,
    pub profile: ProfileName,
    pub notifications: bool,
    pub icon: IconSource,
}

/// The full contents of `config.toml` (design.md §2.2.1, SPEC.md §6).
/// `services` preserves file order, which is sidebar order.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Config {
    pub version: u32,
    pub settings: Settings,
    pub services: Vec<ServiceConfig>,
}

impl Config {
    /// The configuration written by [`crate::config::write_initial`] on
    /// first launch: every settings key set explicitly to the value
    /// documented in SPEC.md §6's example (lines 147–153), and no
    /// services. Never used to fill in values missing from an existing
    /// file — only for a brand-new file.
    pub fn initial() -> Self {
        Config {
            version: 1,
            settings: Settings {
                reconcile_interval_seconds: 60,
                notifications: true,
                notification_batch_threshold: 5,
                badge_sidebar: true,
            },
            services: Vec::new(),
        }
    }
}

/// A `config.toml` load, parse or validation failure (design.md §2.2.1,
/// §5.1).
///
/// `key` is a dotted/indexed path such as `settings.reconcile_interval_seconds`
/// or `services[1].icon.value`, unified across every failure kind so a
/// caller can always point the user at a location in the file. It is
/// `None` only for failures that precede key resolution entirely (I/O
/// errors, TOML syntax errors).
#[derive(Debug, thiserror::Error)]
#[error("{}: {}: {reason}", file.display(), key_or_root(key))]
pub struct ConfigError {
    pub file: PathBuf,
    pub key: Option<String>,
    pub reason: String,
}

/// Display placeholder for [`ConfigError::key`] when a failure has no key
/// (a syntax or I/O error), so every message keeps the same
/// `<file>: <key>: <reason>` shape.
fn key_or_root(key: &Option<String>) -> &str {
    match key {
        Some(key) => key.as_str(),
        None => "<root>",
    }
}

impl From<ConfigError> for AppError {
    fn from(err: ConfigError) -> Self {
        AppError::Config(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_id_accepts_lowercase_alnum_and_hyphen() {
        assert!(ServiceId::new("gmail-personal").is_ok());
    }

    #[test]
    fn service_id_rejects_uppercase() {
        assert!(ServiceId::new("Gmail").is_err());
    }

    #[test]
    fn service_id_rejects_empty() {
        assert!(ServiceId::new("").is_err());
    }

    #[test]
    fn service_id_rejects_too_long() {
        let value = "a".repeat(49);
        assert!(ServiceId::new(value).is_err());
    }

    #[test]
    fn service_id_accepts_max_length() {
        let value = "a".repeat(48);
        assert!(ServiceId::new(value).is_ok());
    }

    #[test]
    fn profile_name_accepts_default_and_isolated() {
        assert!(ProfileName::new("default").is_ok());
        assert!(ProfileName::new("isolated").is_ok());
        assert!(ProfileName::new("work").is_ok());
    }

    #[test]
    fn profile_name_rejects_invalid_characters() {
        assert!(ProfileName::new("My Profile").is_err());
    }

    #[test]
    fn config_error_display_includes_file_and_key() {
        let err = ConfigError {
            file: PathBuf::from("/tmp/eluma/config.toml"),
            key: Some("services[0].id".to_string()),
            reason: "missing required key".to_string(),
        };
        let message = err.to_string();
        assert!(message.contains("/tmp/eluma/config.toml"));
        assert!(message.contains("services[0].id"));
        assert!(message.contains("missing required key"));
    }

    #[test]
    fn config_error_display_uses_root_placeholder_without_key() {
        let err = ConfigError {
            file: PathBuf::from("/tmp/eluma/config.toml"),
            key: None,
            reason: "invalid TOML syntax".to_string(),
        };
        let message = err.to_string();
        assert!(message.contains("/tmp/eluma/config.toml"));
        assert!(message.contains("<root>"));
    }

    #[test]
    fn config_error_converts_to_app_error_with_config_kind() {
        let err = ConfigError {
            file: PathBuf::from("/tmp/eluma/config.toml"),
            key: Some("version".to_string()),
            reason: "must equal 1".to_string(),
        };
        let message = err.to_string();
        let app_err: AppError = err.into();
        assert_eq!(app_err.kind(), "config");
        assert_eq!(app_err.to_string(), format!("config error: {message}"));
        assert!(app_err.to_string().contains("version"));
    }
}
