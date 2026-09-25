//! Applies typed [`ConfigEdit`]s to `config.toml` with `toml_edit`,
//! preserving comments and formatting on every field an edit does not
//! touch (design.md §2.2.1, SPEC.md §6).
//!
//! [`apply`] always re-reads `path` first, so the latest on-disk content is
//! the base for the edit even if it changed since a previous
//! [`super::load`] (design.md §2.2.1). The file is parsed and validated
//! both before and after the edit with [`super::parse`]; nothing is
//! written if either validation fails, if the edit names an unknown
//! service id, or if a [`ConfigEdit::Reorder`] is not a permutation of the
//! existing service ids.

use std::path::Path;
use std::sync::Mutex;

use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value};
use url::Url;

use super::model::{Config, ConfigError, IconSource, ProfileName, ServiceConfig, ServiceId};
use super::parse;

/// A single typed change to `config.toml` (design.md §2.2.1).
#[derive(Debug, Clone)]
pub enum ConfigEdit {
    /// Appends a new service after the last existing one.
    AddService(ServiceConfig),
    /// Changes the named fields of an existing service; `None` fields are
    /// left untouched.
    UpdateService(ServiceId, ServicePatch),
    /// Removes an existing service.
    RemoveService(ServiceId),
    /// Reorders the services to match `Vec<ServiceId>`, which must be a
    /// permutation of the existing service ids (sidebar order).
    Reorder(Vec<ServiceId>),
    /// Changes the named fields of `[settings]`; `None` fields are left
    /// untouched.
    UpdateSettings(SettingsPatch),
}

/// A partial update to a [`ServiceConfig`] (design.md §2.2.1). Only `Some`
/// fields are changed; `id` is not patchable, since it identifies which
/// service the patch applies to.
#[derive(Debug, Clone, Default)]
pub struct ServicePatch {
    pub name: Option<String>,
    pub url: Option<Url>,
    pub profile: Option<ProfileName>,
    pub notifications: Option<bool>,
    pub icon: Option<IconSource>,
}

/// A partial update to [`super::Settings`] (design.md §2.2.1). Only `Some`
/// fields are changed.
#[derive(Debug, Clone, Default)]
pub struct SettingsPatch {
    pub reconcile_interval_seconds: Option<u32>,
    pub notifications: Option<bool>,
    pub notification_batch_threshold: Option<u32>,
    pub badge_sidebar: Option<bool>,
}

/// Serializes every [`apply`] in the process, from the read through the
/// atomic write, so two concurrent edits cannot overwrite each other or
/// share a temporary file.
static APPLY_LOCK: Mutex<()> = Mutex::new(());

/// Applies `edit` to the configuration file at `path` and writes the
/// result back atomically (design.md §2.2.1).
///
/// Order of operations: read the file, parse and validate it as it
/// currently stands (`before`), parse it again as a `toml_edit::DocumentMut`,
/// apply `edit` to the document, parse and validate the edited text, and
/// only then write it atomically. Nothing is written if any step fails —
/// an I/O error reading the file, an invalid existing file, an edit that
/// names an unknown service id or a non-permutation reorder, or an edited
/// document that fails validation (e.g. `AddService` with a duplicate id).
pub fn apply(path: &Path, edit: ConfigEdit) -> Result<Config, ConfigError> {
    let _guard = APPLY_LOCK.lock().map_err(|_| ConfigError {
        file: path.to_path_buf(),
        key: None,
        reason: "config edit lock poisoned by an earlier failed edit".to_string(),
    })?;

    let text = std::fs::read_to_string(path).map_err(|err| super::io_error(path, err))?;

    // The file must already be valid before any edit is applied.
    let before = parse(&text, path)?;

    let mut doc: DocumentMut = text
        .parse()
        .map_err(|err: toml_edit::TomlError| ConfigError {
            file: path.to_path_buf(),
            key: None,
            reason: err.to_string(),
        })?;

    apply_edit(&mut doc, &before, edit, path)?;

    let after_text = doc.to_string();
    let after = parse(&after_text, path)?;

    super::write_atomic(path, &after_text)?;

    Ok(after)
}

fn apply_edit(
    doc: &mut DocumentMut,
    before: &Config,
    edit: ConfigEdit,
    path: &Path,
) -> Result<(), ConfigError> {
    match edit {
        ConfigEdit::AddService(service) => add_service(doc, path, service),
        ConfigEdit::UpdateService(id, patch) => update_service(doc, before, path, &id, patch),
        ConfigEdit::RemoveService(id) => remove_service(doc, before, path, &id),
        ConfigEdit::Reorder(order) => reorder_services(doc, before, path, order),
        ConfigEdit::UpdateSettings(patch) => update_settings(doc, path, patch),
    }
}

/// The current shape of `doc`'s `services` key: either table entries
/// (`[[services]]`) or the empty-array form written when there are none
/// (`services = []`, design.md §2.2.1, matching [`super::write_initial`]).
/// [`apply`]'s pre-edit [`parse`] has already confirmed the key exists and
/// is array-shaped, so [`services_mut`] returning neither is unreachable
/// in practice; it is still handled as an ordinary error, never a panic.
enum ServicesSlot<'a> {
    Empty,
    Populated(&'a mut ArrayOfTables),
}

fn services_mut<'a>(
    doc: &'a mut DocumentMut,
    path: &Path,
) -> Result<ServicesSlot<'a>, ConfigError> {
    match doc.get_mut("services") {
        Some(Item::ArrayOfTables(arr)) => Ok(ServicesSlot::Populated(arr)),
        Some(Item::Value(Value::Array(arr))) if arr.is_empty() => Ok(ServicesSlot::Empty),
        _ => Err(ConfigError {
            file: path.to_path_buf(),
            key: Some("services".to_string()),
            reason: "expected an array of tables".to_string(),
        }),
    }
}

fn table_id(table: &Table) -> Option<&str> {
    table.get("id").and_then(Item::as_str)
}

fn find_by_id<'a>(services: &'a mut ArrayOfTables, id: &str) -> Option<&'a mut Table> {
    services
        .iter_mut()
        .find(|table| table_id(table) == Some(id))
}

fn unknown_id_error(path: &Path, id: &ServiceId) -> ConfigError {
    ConfigError {
        file: path.to_path_buf(),
        key: Some("services".to_string()),
        reason: format!("no service with id `{id}`"),
    }
}

/// Builds a fresh `[[services]]` table for `service`, with fields in the
/// key order documented in SPEC.md §6: `id`, `name`, `url`, `profile`,
/// `notifications`, `icon`.
fn service_table(path: &Path, service: &ServiceConfig) -> Result<Table, ConfigError> {
    let mut table = Table::new();
    table.insert("id", Item::Value(Value::from(service.id.as_str())));
    table.insert("name", Item::Value(Value::from(service.name.as_str())));
    table.insert("url", Item::Value(Value::from(service.url.as_str())));
    table.insert(
        "profile",
        Item::Value(Value::from(service.profile.as_str())),
    );
    table.insert(
        "notifications",
        Item::Value(Value::from(service.notifications)),
    );
    table.insert(
        "icon",
        Item::Value(Value::InlineTable(icon_inline_table(path, &service.icon)?)),
    );
    Ok(table)
}

/// Builds the `icon = { ... }` inline table for `icon`, with keys in the
/// order documented in SPEC.md §6: `source`, then `value` when present.
fn icon_inline_table(path: &Path, icon: &IconSource) -> Result<InlineTable, ConfigError> {
    let mut table = InlineTable::new();
    match icon {
        IconSource::Favicon => {
            table.insert("source", Value::from("favicon"));
        }
        IconSource::File(file_path) => {
            let value = file_path.to_str().ok_or_else(|| ConfigError {
                file: path.to_path_buf(),
                key: Some("icon.value".to_string()),
                reason: "must be valid UTF-8".to_string(),
            })?;
            table.insert("source", Value::from("file"));
            table.insert("value", Value::from(value));
        }
        IconSource::Url(url) => {
            table.insert("source", Value::from("url"));
            table.insert("value", Value::from(url.as_str()));
        }
    }
    Ok(table)
}

/// Replaces the value at `key`, copying the previous value's [`toml_edit`]
/// decor (surrounding whitespace and same-line comment) onto the new one,
/// so an edited field keeps its formatting. Leaves the key itself
/// untouched.
fn set_scalar(
    path: &Path,
    table: &mut Table,
    key: &str,
    mut new_value: Value,
) -> Result<(), ConfigError> {
    let Some(item) = table.get_mut(key) else {
        return Err(ConfigError {
            file: path.to_path_buf(),
            key: Some(key.to_string()),
            reason: "missing required key".to_string(),
        });
    };
    if let Some(old_value) = item.as_value() {
        *new_value.decor_mut() = old_value.decor().clone();
    }
    *item = Item::Value(new_value);
    Ok(())
}

fn add_service(
    doc: &mut DocumentMut,
    path: &Path,
    service: ServiceConfig,
) -> Result<(), ConfigError> {
    let table = service_table(path, &service)?;
    match services_mut(doc, path)? {
        ServicesSlot::Populated(services) => services.push(table),
        ServicesSlot::Empty => {
            let mut services = ArrayOfTables::new();
            services.push(table);
            doc.insert("services", Item::ArrayOfTables(services));
        }
    }
    Ok(())
}

fn remove_service(
    doc: &mut DocumentMut,
    before: &Config,
    path: &Path,
    id: &ServiceId,
) -> Result<(), ConfigError> {
    if !before.services.iter().any(|service| service.id == *id) {
        return Err(unknown_id_error(path, id));
    }

    let now_empty = {
        let ServicesSlot::Populated(services) = services_mut(doc, path)? else {
            return Err(unknown_id_error(path, id));
        };
        let index = services
            .iter()
            .position(|table| table_id(table) == Some(id.as_str()))
            .ok_or_else(|| unknown_id_error(path, id))?;
        services.remove(index);
        services.is_empty()
    };

    if now_empty {
        doc.insert("services", Item::Value(Value::Array(Array::new())));
    }
    Ok(())
}

fn update_service(
    doc: &mut DocumentMut,
    before: &Config,
    path: &Path,
    id: &ServiceId,
    patch: ServicePatch,
) -> Result<(), ConfigError> {
    if !before.services.iter().any(|service| service.id == *id) {
        return Err(unknown_id_error(path, id));
    }

    let ServicesSlot::Populated(services) = services_mut(doc, path)? else {
        return Err(unknown_id_error(path, id));
    };
    let table = find_by_id(services, id.as_str()).ok_or_else(|| unknown_id_error(path, id))?;

    if let Some(name) = patch.name {
        set_scalar(path, table, "name", Value::from(name))?;
    }
    if let Some(url) = patch.url {
        set_scalar(path, table, "url", Value::from(url.as_str()))?;
    }
    if let Some(profile) = patch.profile {
        set_scalar(path, table, "profile", Value::from(profile.as_str()))?;
    }
    if let Some(notifications) = patch.notifications {
        set_scalar(path, table, "notifications", Value::from(notifications))?;
    }
    if let Some(icon) = patch.icon {
        let inline = icon_inline_table(path, &icon)?;
        set_scalar(path, table, "icon", Value::InlineTable(inline))?;
    }
    Ok(())
}

/// Reorders the `[[services]]` tables to match `order`, which must be a
/// permutation of the existing service ids. Each table (including its own
/// header comment and any same-line trailing comment, both part of its
/// `toml_edit` decor) is moved as a whole, and its recorded document
/// position is cleared so the encoder prints tables in the new `Vec`
/// order instead of reverting to the order recorded when the file was
/// parsed (see the module-level test `reorder_...` for the round trip
/// this depends on).
fn reorder_services(
    doc: &mut DocumentMut,
    before: &Config,
    path: &Path,
    order: Vec<ServiceId>,
) -> Result<(), ConfigError> {
    let mut existing: Vec<&str> = before.services.iter().map(|s| s.id.as_str()).collect();
    let mut wanted: Vec<&str> = order.iter().map(ServiceId::as_str).collect();
    existing.sort_unstable();
    wanted.sort_unstable();
    if existing != wanted {
        return Err(ConfigError {
            file: path.to_path_buf(),
            key: Some("services".to_string()),
            reason: "must be a permutation of the existing service ids".to_string(),
        });
    }

    let ServicesSlot::Populated(services) = services_mut(doc, path)? else {
        // Both `order` and the existing services are empty; nothing to do.
        return Ok(());
    };

    let mut tables = Vec::with_capacity(services.len());
    while !services.is_empty() {
        tables.push(services.remove(0));
    }
    for id in &order {
        // The permutation check above guarantees every id is present.
        let index = tables
            .iter()
            .position(|table| table_id(table) == Some(id.as_str()))
            .ok_or_else(|| unknown_id_error(path, id))?;
        let mut table = tables.remove(index);
        table.set_position(None);
        services.push(table);
    }
    Ok(())
}

fn update_settings(
    doc: &mut DocumentMut,
    path: &Path,
    patch: SettingsPatch,
) -> Result<(), ConfigError> {
    let Some(Item::Table(settings)) = doc.get_mut("settings") else {
        return Err(ConfigError {
            file: path.to_path_buf(),
            key: Some("settings".to_string()),
            reason: "expected a table".to_string(),
        });
    };

    if let Some(value) = patch.reconcile_interval_seconds {
        set_scalar(
            path,
            settings,
            "reconcile_interval_seconds",
            Value::from(i64::from(value)),
        )?;
    }
    if let Some(value) = patch.notifications {
        set_scalar(path, settings, "notifications", Value::from(value))?;
    }
    if let Some(value) = patch.notification_batch_threshold {
        set_scalar(
            path,
            settings,
            "notification_batch_threshold",
            Value::from(i64::from(value)),
        )?;
    }
    if let Some(value) = patch.badge_sidebar {
        set_scalar(path, settings, "badge_sidebar", Value::from(value))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::validate::tests::{replace_once, MINIMAL_VALID_CONFIG};
    use crate::config::{load, write_initial};
    use std::fs;
    use std::path::PathBuf;

    // Three services, each with its own header comment above `[[services]]`
    // (design.md §2.2.1's round-trip requirement) and its own same-line
    // trailing comment on a different field, so tests can tell which
    // comment moved with which service. Built from separate blocks rather
    // than one long literal so removal and reorder tests can recombine
    // them and compare byte-for-byte against `apply`'s output.
    const HEADER: &str = r#"version = 1

[settings]
reconcile_interval_seconds = 60   # sweep; observers are the primary signal
notifications = true
notification_batch_threshold = 5
badge_sidebar = true
"#;

    const ALPHA_BLOCK: &str = r#"
# ─── alpha, shared session ───
[[services]]
id = "alpha"
name = "Alpha"
url = "https://alpha.example.com/"
profile = "default"
notifications = true
icon = { source = "favicon" }  # alpha uses its favicon
"#;

    const BETA_BLOCK: &str = r#"
# ─── beta, isolated ───
[[services]]
id = "beta"
name = "Beta"
url = "https://beta.example.com/"
profile = "isolated"
notifications = false
icon = { source = "file", value = "icons/beta.png" }
"#;

    const GAMMA_BLOCK: &str = r#"
# ─── gamma, isolated ───
[[services]]
id = "gamma"
name = "Gamma"
url = "https://gamma.example.com/"
profile = "isolated"
notifications = true
icon = { source = "url", value = "https://gamma.example.com/icon.png" }  # gamma icon
"#;

    fn round_trip_config() -> String {
        [HEADER, ALPHA_BLOCK, BETA_BLOCK, GAMMA_BLOCK].concat()
    }

    fn config_path(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("config.toml")
    }

    fn write_config(dir: &tempfile::TempDir, text: &str) -> PathBuf {
        let path = config_path(dir);
        fs::write(&path, text).expect("seed the config file");
        path
    }

    /// Every entry in `dir` other than `config.toml`, so tests can assert
    /// no temporary file was left behind.
    fn other_entries(dir: &tempfile::TempDir) -> Vec<std::ffi::OsString> {
        fs::read_dir(dir.path())
            .expect("list dir")
            .map(|entry| entry.expect("dir entry").file_name())
            .filter(|name| name != "config.toml")
            .collect()
    }

    fn id(value: &str) -> ServiceId {
        ServiceId::new(value).expect("valid id")
    }

    fn service(id_value: &str, name: &str, url: &str, profile: &str) -> ServiceConfig {
        ServiceConfig {
            id: id(id_value),
            name: name.to_string(),
            url: Url::parse(url).expect("valid url"),
            profile: ProfileName::new(profile).expect("valid profile"),
            notifications: true,
            icon: IconSource::Favicon,
        }
    }

    #[test]
    fn add_service_appends_after_the_last_one_and_keeps_existing_formatting() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = write_config(&dir, &round_trip_config());

        let delta = service("delta", "Delta", "https://delta.example.com/", "isolated");
        let result = apply(&path, ConfigEdit::AddService(delta)).expect("add should succeed");
        assert_eq!(result.services.len(), 4);
        assert_eq!(result.services[3].id.as_str(), "delta");

        let expected = format!(
            "{}\n[[services]]\nid = \"delta\"\nname = \"Delta\"\nurl = \"https://delta.example.com/\"\nprofile = \"isolated\"\nnotifications = true\nicon = {{ source = \"favicon\" }}\n",
            round_trip_config()
        );
        let on_disk = fs::read_to_string(&path).expect("read back");
        assert_eq!(on_disk, expected);
        assert_eq!(load(&path).expect("reload"), result);
    }

    #[test]
    fn concurrent_adds_are_all_kept() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = config_path(&dir);
        write_initial(&path).expect("write_initial should succeed");

        let handles: Vec<_> = (0..8)
            .map(|n| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let svc = service(
                        &format!("svc-{n}"),
                        "Svc",
                        "https://svc.example.com/",
                        "default",
                    );
                    apply(&path, ConfigEdit::AddService(svc)).expect("add should succeed");
                })
            })
            .collect();
        for handle in handles {
            handle.join().expect("thread should not panic");
        }

        assert_eq!(load(&path).expect("reload").services.len(), 8);
    }

    #[test]
    fn add_service_after_write_initial() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = config_path(&dir);
        write_initial(&path).expect("write_initial should succeed");

        let svc = service("first", "First", "https://first.example.com/", "default");
        let result = apply(&path, ConfigEdit::AddService(svc)).expect("add should succeed");

        assert_eq!(result.services.len(), 1);
        assert_eq!(result.services[0].id.as_str(), "first");
        assert_eq!(load(&path).expect("reload"), result);
    }

    #[test]
    fn update_service_changes_only_the_patched_fields_and_keeps_formatting() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = write_config(&dir, &round_trip_config());

        let patch = ServicePatch {
            name: Some("Beta Prime".to_string()),
            ..ServicePatch::default()
        };
        let result = apply(&path, ConfigEdit::UpdateService(id("beta"), patch))
            .expect("update should succeed");

        let expected = replace_once(
            &round_trip_config(),
            "name = \"Beta\"",
            "name = \"Beta Prime\"",
        );
        let on_disk = fs::read_to_string(&path).expect("read back");
        assert_eq!(on_disk, expected);

        let beta = result
            .services
            .iter()
            .find(|s| s.id.as_str() == "beta")
            .expect("beta still present");
        assert_eq!(beta.name, "Beta Prime");
        assert_eq!(load(&path).expect("reload"), result);
    }

    #[test]
    fn update_service_icon_preserves_the_trailing_comment_on_the_icon_line() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = write_config(&dir, &round_trip_config());

        let patch = ServicePatch {
            icon: Some(IconSource::Favicon),
            ..ServicePatch::default()
        };
        apply(&path, ConfigEdit::UpdateService(id("gamma"), patch)).expect("update should succeed");

        let expected = replace_once(
            &round_trip_config(),
            r#"icon = { source = "url", value = "https://gamma.example.com/icon.png" }  # gamma icon"#,
            r#"icon = { source = "favicon" }  # gamma icon"#,
        );
        let on_disk = fs::read_to_string(&path).expect("read back");
        assert_eq!(on_disk, expected);
    }

    #[test]
    fn update_settings_preserves_the_trailing_comment() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = write_config(&dir, &round_trip_config());

        let patch = SettingsPatch {
            reconcile_interval_seconds: Some(120),
            ..SettingsPatch::default()
        };
        let result =
            apply(&path, ConfigEdit::UpdateSettings(patch)).expect("update should succeed");
        assert_eq!(result.settings.reconcile_interval_seconds, 120);

        let expected = replace_once(
            &round_trip_config(),
            "reconcile_interval_seconds = 60   # sweep; observers are the primary signal",
            "reconcile_interval_seconds = 120   # sweep; observers are the primary signal",
        );
        let on_disk = fs::read_to_string(&path).expect("read back");
        assert_eq!(on_disk, expected);
        assert_eq!(load(&path).expect("reload"), result);
    }

    #[test]
    fn remove_service_from_the_middle_keeps_the_others_and_their_comments() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = write_config(&dir, &round_trip_config());

        let result =
            apply(&path, ConfigEdit::RemoveService(id("beta"))).expect("remove should succeed");
        assert_eq!(
            result
                .services
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "gamma"]
        );

        let expected = [HEADER, ALPHA_BLOCK, GAMMA_BLOCK].concat();
        let on_disk = fs::read_to_string(&path).expect("read back");
        assert_eq!(on_disk, expected);
        assert_eq!(load(&path).expect("reload"), result);
    }

    #[test]
    fn remove_last_service_writes_an_empty_services_array() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = write_config(&dir, MINIMAL_VALID_CONFIG);

        let result =
            apply(&path, ConfigEdit::RemoveService(id("svc"))).expect("remove should succeed");
        assert!(result.services.is_empty());
        assert_eq!(load(&path).expect("reload"), result);

        let on_disk = fs::read_to_string(&path).expect("read back");
        assert!(
            on_disk.contains("services = []"),
            "expected an empty services array, got:\n{on_disk}"
        );
    }

    #[test]
    fn reorder_moves_each_services_own_comments_and_round_trips_to_original_order() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let original = round_trip_config();
        let path = write_config(&dir, &original);

        let reordered = apply(
            &path,
            ConfigEdit::Reorder(vec![id("gamma"), id("alpha"), id("beta")]),
        )
        .expect("reorder should succeed");
        assert_eq!(
            reordered
                .services
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            ["gamma", "alpha", "beta"]
        );
        let expected_reordered = [HEADER, GAMMA_BLOCK, ALPHA_BLOCK, BETA_BLOCK].concat();
        assert_eq!(
            fs::read_to_string(&path).expect("read back"),
            expected_reordered
        );

        let restored = apply(
            &path,
            ConfigEdit::Reorder(vec![id("alpha"), id("beta"), id("gamma")]),
        )
        .expect("reorder back should succeed");
        assert_eq!(
            restored
                .services
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "beta", "gamma"]
        );
        assert_eq!(
            fs::read_to_string(&path).expect("read back"),
            original,
            "reordering back to the original order must be byte-identical"
        );
    }

    #[test]
    fn update_service_with_an_unknown_id_errors_and_leaves_the_file_unchanged() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let original = round_trip_config();
        let path = write_config(&dir, &original);

        let err = apply(
            &path,
            ConfigEdit::UpdateService(id("nonexistent"), ServicePatch::default()),
        )
        .expect_err("unknown id should fail");
        assert!(err.reason.contains("nonexistent"));
        assert_eq!(fs::read_to_string(&path).expect("read back"), original);
        assert_eq!(other_entries(&dir), Vec::<std::ffi::OsString>::new());
    }

    #[test]
    fn remove_service_with_an_unknown_id_errors_and_leaves_the_file_unchanged() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let original = round_trip_config();
        let path = write_config(&dir, &original);

        let err = apply(&path, ConfigEdit::RemoveService(id("nonexistent")))
            .expect_err("unknown id should fail");
        assert!(err.reason.contains("nonexistent"));
        assert_eq!(fs::read_to_string(&path).expect("read back"), original);
        assert_eq!(other_entries(&dir), Vec::<std::ffi::OsString>::new());
    }

    #[test]
    fn reorder_that_is_not_a_permutation_errors_and_leaves_the_file_unchanged() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let original = round_trip_config();
        let path = write_config(&dir, &original);

        // Missing "gamma", so this is not a permutation of the existing ids.
        let err = apply(&path, ConfigEdit::Reorder(vec![id("beta"), id("alpha")]))
            .expect_err("non-permutation reorder should fail");
        assert!(err.reason.contains("permutation"));
        assert_eq!(fs::read_to_string(&path).expect("read back"), original);
        assert_eq!(other_entries(&dir), Vec::<std::ffi::OsString>::new());
    }

    #[test]
    fn add_service_with_a_duplicate_id_errors_and_leaves_the_file_unchanged() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let original = round_trip_config();
        let path = write_config(&dir, &original);

        let duplicate = service(
            "alpha",
            "Alpha Again",
            "https://alpha2.example.com/",
            "default",
        );
        let err =
            apply(&path, ConfigEdit::AddService(duplicate)).expect_err("duplicate id should fail");
        assert!(err.reason.contains("duplicate"));
        assert_eq!(fs::read_to_string(&path).expect("read back"), original);
        assert_eq!(other_entries(&dir), Vec::<std::ffi::OsString>::new());
    }

    #[test]
    fn apply_on_an_invalid_existing_file_errors_and_leaves_the_file_unchanged() {
        let dir = tempfile::tempdir().expect("create temp dir");
        // An invalid scheme makes the file fail the pre-edit `parse`.
        let broken = replace_once(
            MINIMAL_VALID_CONFIG,
            r#"url = "https://mail.example.com/""#,
            r#"url = "ftp://mail.example.com/""#,
        );
        let path = write_config(&dir, &broken);

        let err = apply(&path, ConfigEdit::RemoveService(id("svc")))
            .expect_err("an invalid existing file must not be edited");
        assert_eq!(err.key.as_deref(), Some("services[0].url"));
        assert_eq!(fs::read_to_string(&path).expect("read back"), broken);
        assert_eq!(other_entries(&dir), Vec::<std::ffi::OsString>::new());
    }

    #[test]
    fn apply_on_a_missing_file_errors_and_creates_nothing() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = config_path(&dir);

        let err = apply(&path, ConfigEdit::RemoveService(id("svc")))
            .expect_err("apply on a missing file must fail");
        assert_eq!(err.file, path);
        assert!(!path.exists());
        assert_eq!(other_entries(&dir), Vec::<std::ffi::OsString>::new());
    }

    #[test]
    fn update_settings_with_an_unknown_key_shape_cannot_arise_from_a_valid_file() {
        // update_settings' defensive "expected a table" branch is
        // unreachable from any file that has already passed the pre-edit
        // `parse` in `apply`; this test just documents that a no-op patch
        // on an otherwise valid file is always a successful, unchanged
        // round trip.
        let dir = tempfile::tempdir().expect("create temp dir");
        let original = round_trip_config();
        let path = write_config(&dir, &original);

        let result = apply(&path, ConfigEdit::UpdateSettings(SettingsPatch::default()))
            .expect("no-op settings patch should succeed");
        assert_eq!(fs::read_to_string(&path).expect("read back"), original);
        assert_eq!(load(&path).expect("reload"), result);
    }

    #[test]
    fn reorder_with_a_no_op_order_round_trips_to_the_same_bytes() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let original = round_trip_config();
        let path = write_config(&dir, &original);

        apply(
            &path,
            ConfigEdit::Reorder(vec![id("alpha"), id("beta"), id("gamma")]),
        )
        .expect("reorder should succeed");
        assert_eq!(fs::read_to_string(&path).expect("read back"), original);
    }
}
