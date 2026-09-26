//! The nine app commands the `shell` and `settings` webviews call (design.md
//! §2.2.12; Task 1.9): `get_snapshot`, `add_service`, `update_service`,
//! `remove_service`, `reorder_services`, `select_service`,
//! `update_settings`, `open_settings`, `reload_service`.
//!
//! Every command here does the minimum needed to type-check its IPC
//! boundary — parse/validate the input, call one [`crate::services::
//! ServiceManager`] method, convert the result — since `ServiceManager`
//! (extended by this task rather than duplicated: see `services/mod.rs`)
//! already owns config-editing, webview reconciliation and event emission
//! end to end (Task 1.8). No command here talks to `config`, `state` or
//! `host` directly.
//!
//! `set_icon_override`, `refresh_icon`, `get_diagnostics` and the
//! `status-changed` event (design.md §2.2.12's remaining rows) are out of
//! scope for this task — so is the `shell`/settings frontend UI that would
//! call any of this (Task 1.10, 1.12).
//!
//! [`COMMAND_NAMES`] is the single list every one of these three has to
//! agree with: `build.rs`'s `AppManifest::commands(&[...])` (which is what
//! makes Tauri autogenerate the `allow-<kebab-name>` permission for each —
//! see `git show spike/m0-harness:src-tauri/build.rs`), `lib.rs`'s
//! `tauri::generate_handler![...]`, and `capabilities/shell.json`'s
//! `permissions` array (checked against this list by this module's own
//! test, below, since a build script cannot depend on the crate it
//! builds).

mod dto;
mod snapshot;

use std::sync::Arc;

use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
use url::Url;

use crate::config::{IconSource, ProfileName, ServiceConfig, ServiceId, Settings};
use crate::error::AppError;
use crate::host::layout::SIDEBAR_WIDTH;
use crate::icons::IconOverride;
use crate::services::ServiceManager;

use dto::{ServicePatchDto, SettingsPatchDto};
use snapshot::build_snapshot;
pub use snapshot::SnapshotDto;

/// Every command name this module registers, in the order `lib.rs`'s
/// `tauri::generate_handler!` lists them — see the module doc comment for
/// why this list exists and what else has to match it. Only referenced
/// from this module's own `#[cfg(test)]` (the `shell.json` cross-check
/// below); `build.rs` and `lib.rs` each spell the same nine names out
/// separately, since a build script cannot depend on the crate it builds
/// and `generate_handler!` needs a literal list, not a runtime slice.
#[allow(dead_code)]
pub const COMMAND_NAMES: &[&str] = &[
    "get_snapshot",
    "add_service",
    "update_service",
    "remove_service",
    "reorder_services",
    "select_service",
    "update_settings",
    "open_settings",
    "reload_service",
    "set_icon_override",
    "refresh_icon",
];

/// The webview label `open_settings` creates/focuses its window under
/// (design.md §2.2.12, §2.2.13), and the label `services-changed`'s second
/// emit target (`services::SETTINGS_LABEL`) names — fixed independently of
/// whatever label the main window happens to use, per the implementation
/// plan's instruction ("両バックエンドでラベルを settings に固定し、メインウィンドウのラベルには依存しません").
const SETTINGS_WINDOW_LABEL: &str = "settings";

/// The settings window's entry point: the same bundled `index.html` the
/// main window loads, routed to the settings screen via a hash fragment
/// (Task 1.12 builds the screen itself; this task only opens the window).
const SETTINGS_WINDOW_PATH: &str = "index.html#/settings";

/// Returns the current settings, services (sidebar order), the `unread`
/// status map (design.md §2.2.7), the sidebar width, the active service
/// id (Task 1.10), and — only when this app failed to start — a
/// structured `configError` (design.md §2.2.12).
#[tauri::command]
pub async fn get_snapshot(
    manager: State<'_, Arc<ServiceManager>>,
) -> Result<SnapshotDto, AppError> {
    let snapshot = manager.snapshot().await;
    Ok(build_snapshot(
        snapshot.settings,
        snapshot.services,
        snapshot.config_error,
        snapshot.statuses,
        SIDEBAR_WIDTH,
        snapshot.active,
        snapshot.icons,
    ))
}

/// Adds a new service named `name` at `url`, using profile `profile`
/// (design.md §2.2.12: input `{ name, url, profile }`). The input has no
/// `notifications`/`icon` fields of its own — every newly added service
/// starts with notifications on (matching `Settings::initial`'s own
/// project-wide default) and its icon set to `Favicon` (design.md
/// §2.2.1's "use the service's own favicon", the natural starting point
/// before a user picks an override in the settings UI, Task 1.12's
/// `set_icon_override`). Returns the new service's full config.
#[tauri::command]
pub async fn add_service(
    manager: State<'_, Arc<ServiceManager>>,
    name: String,
    url: Url,
    profile: ProfileName,
) -> Result<ServiceConfig, AppError> {
    manager
        .add_service(&name, url, profile, true, IconSource::Favicon)
        .await
}

/// Applies `patch`'s `Some` fields to service `id` (design.md §2.2.12:
/// input `{ id, patch }`). Returns the updated service config.
#[tauri::command]
pub async fn update_service(
    manager: State<'_, Arc<ServiceManager>>,
    id: ServiceId,
    patch: ServicePatchDto,
) -> Result<ServiceConfig, AppError> {
    manager.update_service(&id, patch.into()).await
}

/// Removes service `id` (design.md §2.2.12: input `{ id, deleteSessionData
/// }`). `delete_session_data` only ever removes on-disk profile data when
/// the removed service's profile was `isolated` and no remaining service
/// still resolves to the same data store — `ServiceManager::remove_service`
/// (Task 1.8) already makes and tests that determination.
#[tauri::command(rename_all = "camelCase")]
pub async fn remove_service(
    manager: State<'_, Arc<ServiceManager>>,
    id: ServiceId,
    delete_session_data: bool,
) -> Result<(), AppError> {
    manager.remove_service(&id, delete_session_data).await
}

/// Reorders the sidebar to `ids`, which must be a permutation of the
/// existing service ids (design.md §2.2.12: input `{ ids }`;
/// `config::apply` itself enforces the permutation).
#[tauri::command]
pub async fn reorder_services(
    manager: State<'_, Arc<ServiceManager>>,
    ids: Vec<ServiceId>,
) -> Result<(), AppError> {
    manager.reorder_services(ids).await
}

/// Activates service `id` and emits `select-service` to the shell
/// (design.md §2.2.12: input `{ id }`).
#[tauri::command]
pub async fn select_service(
    manager: State<'_, Arc<ServiceManager>>,
    id: ServiceId,
) -> Result<(), AppError> {
    manager.select_service(&id).await
}

/// Applies `patch`'s `Some` fields to `[settings]` (design.md §2.2.12:
/// input `{ patch }`). Returns the updated settings.
#[tauri::command]
pub async fn update_settings(
    manager: State<'_, Arc<ServiceManager>>,
    patch: SettingsPatchDto,
) -> Result<Settings, AppError> {
    manager.update_settings(patch.into()).await
}

/// Opens the settings window (design.md §2.2.12), or focuses it if one
/// already exists — so calling this twice in a row never produces two
/// windows. The settings *screen* itself (its route, `#/settings`) is
/// Task 1.12; this command only owns the window.
#[tauri::command]
pub async fn open_settings(app: AppHandle) -> Result<(), AppError> {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
        window.show()?;
        window.set_focus()?;
        return Ok(());
    }

    WebviewWindowBuilder::new(
        &app,
        SETTINGS_WINDOW_LABEL,
        WebviewUrl::App(SETTINGS_WINDOW_PATH.into()),
    )
    .title("Eluma Settings")
    .build()?;
    Ok(())
}

/// Reloads service `id`'s current page (design.md §2.2.12: input `{ id
/// }`).
#[tauri::command]
pub async fn reload_service(
    manager: State<'_, Arc<ServiceManager>>,
    id: ServiceId,
) -> Result<(), AppError> {
    manager.reload_service(&id).await
}

/// Sets `id`'s icon override (design.md §2.2.12: input `{ id, source:
/// favicon | file(path) | url }`, output `—`): persists it via
/// [`ServiceManager::set_icon_override`] (which already applies the
/// ordinary `update_service` edit-and-reconcile path, so `services-changed`
/// still fires), then invalidates the current cache and resolves the new
/// source in the background — [`ServiceManager::spawn_icon_resolve`] emits
/// `service-icon-changed` once that finishes, if it changed the cache.
#[tauri::command]
pub async fn set_icon_override(
    manager: State<'_, Arc<ServiceManager>>,
    id: ServiceId,
    source: IconOverride,
) -> Result<(), AppError> {
    let manager = Arc::clone(manager.inner());
    let icon_source = manager.set_icon_override(&id, source).await?;
    manager.clear_icon_cache(&id);
    ServiceManager::spawn_icon_resolve(&manager, id, icon_source);
    Ok(())
}

/// Re-resolves `id`'s icon from scratch (design.md §2.2.12: input `{ id
/// }`, output `—`): deletes the current cache so resolution cannot skip
/// re-fetching, then resolves in the background using `id`'s currently
/// configured icon source and last-known `iconCandidates` — see
/// [`set_icon_override`]'s doc comment for the shared resolve/emit path.
#[tauri::command]
pub async fn refresh_icon(
    manager: State<'_, Arc<ServiceManager>>,
    id: ServiceId,
) -> Result<(), AppError> {
    let manager = Arc::clone(manager.inner());
    let icon_source = manager
        .icon_source_for(&id)
        .await
        .ok_or_else(|| AppError::Config(format!("service '{id}' does not exist")))?;
    manager.clear_icon_cache(&id);
    ServiceManager::spawn_icon_resolve(&manager, id, icon_source);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::COMMAND_NAMES;

    /// Reads the real `capabilities/shell.json` (not a copy) and checks it
    /// against [`COMMAND_NAMES`]: exactly `shell`/`settings` as webviews,
    /// no `windows` scoping (design.md §4.3's "scoped by webview label, not
    /// window label"), and exactly one `allow-<kebab-name>` permission per
    /// command, nothing else. This is the practical substitute for
    /// validating `build.rs`'s `AppManifest::commands(&[...])` list, which
    /// a `#[cfg(test)]` in this crate cannot itself introspect (it runs in
    /// a build script, before this crate exists) — the task's "generated
    /// `src-tauri/gen/schemas`" verification (see the task report) is the
    /// other half of that check, done once by hand after a real build.
    #[test]
    fn shell_capability_is_scoped_to_shell_and_settings_with_the_expected_permissions() {
        let raw = include_str!("../../capabilities/shell.json");
        let value: serde_json::Value = serde_json::from_str(raw).expect("shell.json is valid json");

        assert_eq!(
            value["identifier"].as_str(),
            Some("shell"),
            "capability identifier"
        );

        let webviews: Vec<&str> = value["webviews"]
            .as_array()
            .expect("webviews is an array")
            .iter()
            .map(|v| v.as_str().expect("webview label is a string"))
            .collect();
        assert_eq!(
            webviews,
            vec!["shell", "settings"],
            "must be scoped to exactly the shell and settings webviews"
        );
        assert!(
            value.get("windows").is_none(),
            "must be scoped by webview label only (design.md §4.3), not by window label"
        );

        let mut permissions: Vec<String> = value["permissions"]
            .as_array()
            .expect("permissions is an array")
            .iter()
            .map(|v| v.as_str().expect("permission is a string").to_string())
            .collect();
        permissions.sort();

        let mut expected: Vec<String> = COMMAND_NAMES
            .iter()
            .map(|name| format!("allow-{}", name.replace('_', "-")))
            .chain(
                // design.md §4.3: the shell also needs events and window focus.
                ["core:event:default", "core:window:allow-set-focus"]
                    .into_iter()
                    .map(String::from),
            )
            .collect();
        expected.sort();

        assert_eq!(
            permissions, expected,
            "shell.json's permissions must be one allow-<command> per COMMAND_NAMES entry plus the design §4.3 core permissions"
        );
    }
}
