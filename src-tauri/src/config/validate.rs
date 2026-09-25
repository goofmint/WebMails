//! TOML text to typed [`Config`], failing on the first missing or invalid
//! key (design.md §2.2.1, SPEC.md §6).

use std::collections::HashSet;
use std::path::Path;

use toml::{Table, Value};
use url::Url;

use super::model::{
    Config, ConfigError, IconSource, ProfileName, ServiceConfig, ServiceId, Settings,
};

fn key_path(parent: &str, key: &str) -> String {
    if parent.is_empty() {
        key.to_string()
    } else {
        format!("{parent}.{key}")
    }
}

fn missing(file: &Path, key: &str) -> ConfigError {
    ConfigError {
        file: file.to_path_buf(),
        key: Some(key.to_string()),
        reason: "missing required key".to_string(),
    }
}

fn type_mismatch(file: &Path, key: &str, expected: &str, actual: &Value) -> ConfigError {
    ConfigError {
        file: file.to_path_buf(),
        key: Some(key.to_string()),
        reason: format!("expected {expected}, found {}", actual.type_str()),
    }
}

fn invalid(file: &Path, key: &str, reason: impl Into<String>) -> ConfigError {
    ConfigError {
        file: file.to_path_buf(),
        key: Some(key.to_string()),
        reason: reason.into(),
    }
}

/// Reads a required table-valued key.
pub(crate) fn require_table<'a>(
    table: &'a Table,
    key: &str,
    parent: &str,
    file: &Path,
) -> Result<&'a Table, ConfigError> {
    let path = key_path(parent, key);
    match table.get(key) {
        None => Err(missing(file, &path)),
        Some(Value::Table(inner)) => Ok(inner),
        Some(other) => Err(type_mismatch(file, &path, "table", other)),
    }
}

/// Reads a required array-valued key.
pub(crate) fn require_array<'a>(
    table: &'a Table,
    key: &str,
    parent: &str,
    file: &Path,
) -> Result<&'a Vec<Value>, ConfigError> {
    let path = key_path(parent, key);
    match table.get(key) {
        None => Err(missing(file, &path)),
        Some(Value::Array(inner)) => Ok(inner),
        Some(other) => Err(type_mismatch(file, &path, "array", other)),
    }
}

/// Reads a required string-valued key.
pub(crate) fn require_str<'a>(
    table: &'a Table,
    key: &str,
    parent: &str,
    file: &Path,
) -> Result<&'a str, ConfigError> {
    let path = key_path(parent, key);
    match table.get(key) {
        None => Err(missing(file, &path)),
        Some(Value::String(inner)) => Ok(inner.as_str()),
        Some(other) => Err(type_mismatch(file, &path, "string", other)),
    }
}

/// Reads a required boolean-valued key.
pub(crate) fn require_bool(
    table: &Table,
    key: &str,
    parent: &str,
    file: &Path,
) -> Result<bool, ConfigError> {
    let path = key_path(parent, key);
    match table.get(key) {
        None => Err(missing(file, &path)),
        Some(Value::Boolean(inner)) => Ok(*inner),
        Some(other) => Err(type_mismatch(file, &path, "boolean", other)),
    }
}

/// Reads a required key holding a non-negative integer that fits in
/// `u32`. TOML integers are `i64`; negative values and values exceeding
/// `u32::MAX` are rejected.
pub(crate) fn require_u32(
    table: &Table,
    key: &str,
    parent: &str,
    file: &Path,
) -> Result<u32, ConfigError> {
    let path = key_path(parent, key);
    match table.get(key) {
        None => Err(missing(file, &path)),
        Some(Value::Integer(inner)) => u32::try_from(*inner).map_err(|_| {
            invalid(
                file,
                &path,
                format!("must be between 0 and {} (found {inner})", u32::MAX),
            )
        }),
        Some(other) => Err(type_mismatch(file, &path, "integer", other)),
    }
}

fn require_http_url(
    table: &Table,
    key: &str,
    parent: &str,
    file: &Path,
) -> Result<Url, ConfigError> {
    let path = key_path(parent, key);
    let raw = require_str(table, key, parent, file)?;
    let url = Url::parse(raw).map_err(|err| invalid(file, &path, format!("invalid URL: {err}")))?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(invalid(
            file,
            &path,
            format!("must use http or https scheme (found `{}`)", url.scheme()),
        ));
    }
    Ok(url)
}

fn parse_settings(table: &Table, file: &Path) -> Result<Settings, ConfigError> {
    let parent = "settings";
    Ok(Settings {
        reconcile_interval_seconds: require_u32(table, "reconcile_interval_seconds", parent, file)?,
        notifications: require_bool(table, "notifications", parent, file)?,
        notification_batch_threshold: require_u32(
            table,
            "notification_batch_threshold",
            parent,
            file,
        )?,
        badge_sidebar: require_bool(table, "badge_sidebar", parent, file)?,
    })
}

fn parse_icon(table: &Table, parent: &str, file: &Path) -> Result<IconSource, ConfigError> {
    let source_key = key_path(parent, "source");
    let source = require_str(table, "source", parent, file)?;
    match source {
        "favicon" => Ok(IconSource::Favicon),
        "file" => {
            let value_key = key_path(parent, "value");
            let value = require_str(table, "value", parent, file)?;
            if value.is_empty() {
                return Err(invalid(file, &value_key, "must not be empty"));
            }
            Ok(IconSource::File(value.into()))
        }
        "url" => Ok(IconSource::Url(require_http_url(
            table, "value", parent, file,
        )?)),
        other => Err(invalid(
            file,
            &source_key,
            format!("unknown icon source `{other}` (expected `favicon`, `file`, or `url`)"),
        )),
    }
}

fn parse_service(table: &Table, parent: &str, file: &Path) -> Result<ServiceConfig, ConfigError> {
    let id_key = key_path(parent, "id");
    let id_raw = require_str(table, "id", parent, file)?;
    let id = ServiceId::new(id_raw).map_err(|reason| invalid(file, &id_key, reason))?;

    let name = require_str(table, "name", parent, file)?.to_string();

    let url = require_http_url(table, "url", parent, file)?;

    let profile_key = key_path(parent, "profile");
    let profile_raw = require_str(table, "profile", parent, file)?;
    let profile =
        ProfileName::new(profile_raw).map_err(|reason| invalid(file, &profile_key, reason))?;

    let notifications = require_bool(table, "notifications", parent, file)?;

    let icon_parent = key_path(parent, "icon");
    let icon_table = require_table(table, "icon", parent, file)?;
    let icon = parse_icon(icon_table, &icon_parent, file)?;

    Ok(ServiceConfig {
        id,
        name,
        url,
        profile,
        notifications,
        icon,
    })
}

/// Parses `text` (the contents of a `config.toml` file at `file`, used
/// only to label errors) into a fully validated [`Config`]. Fails on the
/// first missing or invalid key; nothing is filled in from a default.
pub fn parse(text: &str, file: &Path) -> Result<Config, ConfigError> {
    let table: Table = text.parse().map_err(|err: toml::de::Error| ConfigError {
        file: file.to_path_buf(),
        key: None,
        reason: err.to_string(),
    })?;

    let version = require_u32(&table, "version", "", file)?;
    if version != 1 {
        return Err(invalid(
            file,
            "version",
            format!("must equal 1 (found {version})"),
        ));
    }

    let settings_table = require_table(&table, "settings", "", file)?;
    let settings = parse_settings(settings_table, file)?;

    let services_value = require_array(&table, "services", "", file)?;
    let mut services = Vec::with_capacity(services_value.len());
    let mut seen_ids: HashSet<String> = HashSet::new();
    for (index, value) in services_value.iter().enumerate() {
        let parent = format!("services[{index}]");
        let service_table = match value {
            Value::Table(inner) => inner,
            other => return Err(type_mismatch(file, &parent, "table", other)),
        };
        let service = parse_service(service_table, &parent, file)?;
        if !seen_ids.insert(service.id.as_str().to_string()) {
            return Err(invalid(
                file,
                &key_path(&parent, "id"),
                format!("duplicate service id `{}`", service.id),
            ));
        }
        services.push(service);
    }

    Ok(Config {
        version,
        settings,
        services,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::IconSource;
    use std::path::PathBuf;

    /// SPEC.md §6, lines 146–189: the documented example, verbatim.
    const SPEC_EXAMPLE: &str = r#"
version = 1

[settings]
reconcile_interval_seconds = 60   # sweep; observers are the primary signal
notifications = true
notification_batch_threshold = 5
badge_sidebar = true

# ─── Gmail ×2, sharing one Google session ───
[[services]]
id = "gmail-personal"
name = "Personal"
url = "https://mail.google.com/mail/u/me@example.com/"
profile = "default"
notifications = true
icon = { source = "favicon" }

[[services]]
id = "gmail-work"
name = "Work"
url = "https://mail.google.com/mail/u/me@company.com/"
profile = "default"
notifications = true
icon = { source = "favicon" }

# ─── iCloud, its own session ───
[[services]]
id = "icloud"
name = "iCloud"
url = "https://www.icloud.com/mail/"
profile = "isolated"
notifications = true
icon = { source = "file", value = "icons/icloud.png" }

# ─── Outlook.com, its own session ───
[[services]]
id = "outlook"
name = "Outlook"
url = "https://outlook.live.com/mail/0/"
profile = "isolated"
notifications = true
icon = { source = "favicon" }
"#;

    /// A minimal single-service fixture used by the invalid-value tests.
    /// `services[0].notifications` is deliberately `false`, distinct from
    /// `settings.notifications = true`, so tests can target either one by
    /// an unambiguous string replacement.
    const MINIMAL_VALID_CONFIG: &str = r#"
version = 1

[settings]
reconcile_interval_seconds = 60
notifications = true
notification_batch_threshold = 5
badge_sidebar = true

[[services]]
id = "svc"
name = "Service"
url = "https://mail.example.com/"
profile = "default"
notifications = false
icon = { source = "favicon" }
"#;

    const DUPLICATE_ID_CONFIG: &str = r#"
version = 1

[settings]
reconcile_interval_seconds = 60
notifications = true
notification_batch_threshold = 5
badge_sidebar = true

[[services]]
id = "svc"
name = "Service One"
url = "https://mail.example.com/one"
profile = "default"
notifications = true
icon = { source = "favicon" }

[[services]]
id = "svc"
name = "Service Two"
url = "https://mail.example.com/two"
profile = "default"
notifications = true
icon = { source = "favicon" }
"#;

    fn test_file() -> PathBuf {
        PathBuf::from("/tmp/eluma/config.toml")
    }

    fn replace_once(base: &str, from: &str, to: &str) -> String {
        assert_eq!(
            base.matches(from).count(),
            1,
            "expected exactly one occurrence of {from:?} in fixture"
        );
        base.replacen(from, to, 1)
    }

    /// Removes a value from a cloned copy of `table`, following `path`
    /// (table keys, or array indices given as decimal strings).
    fn remove_key(table: &Table, path: &[&str]) -> Table {
        fn remove_nested(value: &mut Value, path: &[&str]) {
            let (head, rest) = match path.split_first() {
                Some(pair) => pair,
                None => return,
            };
            match value {
                Value::Table(t) => {
                    if rest.is_empty() {
                        t.remove(*head);
                    } else if let Some(child) = t.get_mut(*head) {
                        remove_nested(child, rest);
                    }
                }
                Value::Array(a) => {
                    if let Ok(index) = head.parse::<usize>() {
                        if let Some(child) = a.get_mut(index) {
                            remove_nested(child, rest);
                        }
                    }
                }
                _ => {}
            }
        }

        let mut value = Value::Table(table.clone());
        remove_nested(&mut value, path);
        match value {
            Value::Table(t) => t,
            _ => unreachable!("root value stays a table"),
        }
    }

    #[test]
    fn parses_the_spec_example() {
        let config = parse(SPEC_EXAMPLE, &test_file()).expect("SPEC.md §6 example should parse");

        assert_eq!(config.version, 1);
        assert_eq!(config.services.len(), 4);

        let ids: Vec<&str> = config.services.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["gmail-personal", "gmail-work", "icloud", "outlook"]);

        let icloud = &config.services[2];
        assert_eq!(icloud.profile.as_str(), "isolated");
        assert_eq!(
            icloud.icon,
            IconSource::File(PathBuf::from("icons/icloud.png"))
        );
    }

    #[test]
    fn missing_required_keys_are_named_in_the_error() {
        let base: Table = SPEC_EXAMPLE.parse().expect("fixture parses as TOML");

        let cases: &[(&[&str], &str)] = &[
            (&["version"], "version"),
            (&["settings"], "settings"),
            (&["services"], "services"),
            (
                &["settings", "reconcile_interval_seconds"],
                "settings.reconcile_interval_seconds",
            ),
            (&["settings", "notifications"], "settings.notifications"),
            (
                &["settings", "notification_batch_threshold"],
                "settings.notification_batch_threshold",
            ),
            (&["settings", "badge_sidebar"], "settings.badge_sidebar"),
            (&["services", "0", "id"], "services[0].id"),
            (&["services", "0", "name"], "services[0].name"),
            (&["services", "0", "url"], "services[0].url"),
            (&["services", "0", "profile"], "services[0].profile"),
            (
                &["services", "0", "notifications"],
                "services[0].notifications",
            ),
            (&["services", "0", "icon"], "services[0].icon"),
            (
                &["services", "2", "icon", "source"],
                "services[2].icon.source",
            ),
            (
                &["services", "2", "icon", "value"],
                "services[2].icon.value",
            ),
        ];

        for (path, expected_key) in cases {
            let modified = remove_key(&base, path);
            let text = toml::to_string(&modified).expect("serialize modified fixture");
            let err =
                parse(&text, &test_file()).expect_err(&format!("expected error removing {path:?}"));
            assert_eq!(err.key.as_deref(), Some(*expected_key), "removing {path:?}");
            assert!(
                err.reason.contains("missing required key"),
                "removing {path:?}"
            );
        }
    }

    #[test]
    fn version_must_equal_one() {
        let text = replace_once(MINIMAL_VALID_CONFIG, "version = 1", "version = 2");
        let err = parse(&text, &test_file()).expect_err("version 2 should fail");
        assert_eq!(err.key.as_deref(), Some("version"));
        assert!(err.reason.contains("must equal 1"));
    }

    #[test]
    fn version_must_be_an_integer() {
        let text = replace_once(MINIMAL_VALID_CONFIG, "version = 1", r#"version = "1""#);
        let err = parse(&text, &test_file()).expect_err("string version should fail");
        assert_eq!(err.key.as_deref(), Some("version"));
    }

    #[test]
    fn service_id_rejects_uppercase() {
        let text = replace_once(MINIMAL_VALID_CONFIG, r#"id = "svc""#, r#"id = "SVC""#);
        let err = parse(&text, &test_file()).expect_err("uppercase id should fail");
        assert_eq!(err.key.as_deref(), Some("services[0].id"));
    }

    #[test]
    fn service_id_rejects_empty() {
        let text = replace_once(MINIMAL_VALID_CONFIG, r#"id = "svc""#, r#"id = """#);
        let err = parse(&text, &test_file()).expect_err("empty id should fail");
        assert_eq!(err.key.as_deref(), Some("services[0].id"));
    }

    #[test]
    fn service_id_rejects_49_characters() {
        let long = "a".repeat(49);
        let text = replace_once(
            MINIMAL_VALID_CONFIG,
            r#"id = "svc""#,
            &format!(r#"id = "{long}""#),
        );
        let err = parse(&text, &test_file()).expect_err("49-char id should fail");
        assert_eq!(err.key.as_deref(), Some("services[0].id"));
    }

    #[test]
    fn duplicate_service_ids_are_rejected() {
        let err = parse(DUPLICATE_ID_CONFIG, &test_file()).expect_err("duplicate id should fail");
        assert_eq!(err.key.as_deref(), Some("services[1].id"));
        assert!(err.reason.contains("duplicate"));
    }

    #[test]
    fn service_url_rejects_non_http_scheme() {
        let text = replace_once(
            MINIMAL_VALID_CONFIG,
            r#"url = "https://mail.example.com/""#,
            r#"url = "ftp://mail.example.com/""#,
        );
        let err = parse(&text, &test_file()).expect_err("ftp:// url should fail");
        assert_eq!(err.key.as_deref(), Some("services[0].url"));
    }

    #[test]
    fn service_url_rejects_unparsable_value() {
        let text = replace_once(
            MINIMAL_VALID_CONFIG,
            r#"url = "https://mail.example.com/""#,
            r#"url = "not a url""#,
        );
        let err = parse(&text, &test_file()).expect_err("unparsable url should fail");
        assert_eq!(err.key.as_deref(), Some("services[0].url"));
    }

    #[test]
    fn profile_name_rejects_invalid_value() {
        let text = replace_once(
            MINIMAL_VALID_CONFIG,
            r#"profile = "default""#,
            r#"profile = "My Profile""#,
        );
        let err = parse(&text, &test_file()).expect_err("invalid profile name should fail");
        assert_eq!(err.key.as_deref(), Some("services[0].profile"));
    }

    #[test]
    fn icon_source_rejects_unknown_value() {
        let text = replace_once(
            MINIMAL_VALID_CONFIG,
            r#"icon = { source = "favicon" }"#,
            r#"icon = { source = "gravatar" }"#,
        );
        let err = parse(&text, &test_file()).expect_err("unknown icon source should fail");
        assert_eq!(err.key.as_deref(), Some("services[0].icon.source"));
    }

    #[test]
    fn settings_u32_rejects_negative_value() {
        let text = replace_once(
            MINIMAL_VALID_CONFIG,
            "reconcile_interval_seconds = 60",
            "reconcile_interval_seconds = -1",
        );
        let err = parse(&text, &test_file()).expect_err("negative u32 should fail");
        assert_eq!(
            err.key.as_deref(),
            Some("settings.reconcile_interval_seconds")
        );
    }

    #[test]
    fn settings_bool_rejects_wrong_type() {
        let text = replace_once(
            MINIMAL_VALID_CONFIG,
            "badge_sidebar = true",
            "badge_sidebar = 1",
        );
        let err = parse(&text, &test_file()).expect_err("integer badge_sidebar should fail");
        assert_eq!(err.key.as_deref(), Some("settings.badge_sidebar"));
        assert!(err.reason.contains("boolean"));
    }

    #[test]
    fn syntax_error_has_no_key_and_includes_the_file_path() {
        let file = PathBuf::from("/tmp/eluma/broken-config.toml");
        let err = parse("[settings\nversion = 1", &file).expect_err("malformed TOML should fail");
        assert_eq!(err.key, None);
        assert!(err.to_string().contains("/tmp/eluma/broken-config.toml"));
    }
}
