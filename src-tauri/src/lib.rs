mod agent;
pub mod agent_bridge;
mod commands;
pub mod config;
pub mod error;
pub mod host;
mod notify;
pub mod paths;
mod platform;
pub mod profile;
pub mod services;
pub mod state;

use std::sync::Arc;

use tauri::{Manager, WebviewUrl};

use host::{build_main_host, layout};
use profile::PlatformProfileBackend;
use services::ServiceManager;
use state::StateStore;

/// Reads `window`'s current physical size and scale factor and converts
/// them to a logical [`layout::content_rect`], the same conversion
/// `host::multiwebview` uses internally. Kept here (rather than exported
/// from `host`) since `setup` is the only other call site that needs a
/// content rect from a live `Window`.
fn current_content_rect(
    window: &tauri::Window,
) -> Result<layout::Rect, Box<dyn std::error::Error>> {
    let physical = window.inner_size()?;
    let scale = window.scale_factor()?;
    let (width, height) =
        layout::physical_to_logical(physical.width as f64, physical.height as f64, scale);
    Ok(layout::content_rect(width, height))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::add_service,
            commands::update_service,
            commands::remove_service,
            commands::reorder_services,
            commands::select_service,
            commands::update_settings,
            commands::open_settings,
            commands::reload_service,
            agent_bridge::report_unread,
        ])
        .setup(|app| {
            // `app.windows` is empty in `tauri.conf.json` (design.md §9.2):
            // the main window is built here in code because it needs
            // `add_child` (design.md §10). `build_main_host` picks the
            // concrete `WebviewHost` (`MultiwebviewHost`, or
            // `ChildWindowHost` behind Cargo feature `host-child-windows`,
            // design.md §8.1; Task 1.7) behind one cfg branch, so this
            // stays the same call either way.
            let profile_backend = PlatformProfileBackend::new(app.handle())?;
            let (window, host) = build_main_host(
                app.handle(),
                WebviewUrl::App("index.html".into()),
                "Eluma",
                (800.0, 600.0),
                profile_backend,
            )?;
            app.manage(host.clone());

            // Relayout on resize and on display-scale change, skipping
            // zero-size events (e.g. minimising), which would otherwise
            // collapse every webview to nothing (design.md §2.2.4).
            let relayout_window = window.clone();
            let relayout_host = host.clone();
            window.on_window_event(move |event| {
                let (physical_width, physical_height, scale_factor) = match event {
                    tauri::WindowEvent::Resized(size) => {
                        let scale_factor = match relayout_window.scale_factor() {
                            Ok(scale_factor) => scale_factor,
                            Err(err) => {
                                tracing::error!("relayout: could not read scale factor: {err}");
                                return;
                            }
                        };
                        (size.width, size.height, scale_factor)
                    }
                    tauri::WindowEvent::ScaleFactorChanged {
                        scale_factor,
                        new_inner_size,
                        ..
                    } => (new_inner_size.width, new_inner_size.height, *scale_factor),
                    // `ChildWindowHost`'s service windows are separate OS
                    // windows tracking `main`'s screen position, not just
                    // its size (design.md §2.2.4's fallback bullet), so
                    // only that host needs a `Moved` relayout too; gating
                    // the arm on the feature (rather than matching it
                    // unconditionally) keeps `MultiwebviewHost`'s own
                    // behaviour — no relayout on move, since its child
                    // webviews are window-relative already — unchanged
                    // when the feature is off (Task 1.7).
                    #[cfg(feature = "host-child-windows")]
                    tauri::WindowEvent::Moved(_) => {
                        let scale_factor = match relayout_window.scale_factor() {
                            Ok(scale_factor) => scale_factor,
                            Err(err) => {
                                tracing::error!("relayout: could not read scale factor: {err}");
                                return;
                            }
                        };
                        let size = match relayout_window.inner_size() {
                            Ok(size) => size,
                            Err(err) => {
                                tracing::error!("relayout: could not read inner size: {err}");
                                return;
                            }
                        };
                        (size.width, size.height, scale_factor)
                    }
                    // `WindowEvent` is not exhaustive on every platform
                    // (some variants are `cfg(mobile)`-only), and this
                    // handler only cares about the ones above.
                    _ => return,
                };
                if physical_width == 0 || physical_height == 0 {
                    return;
                }
                let (width, height) = layout::physical_to_logical(
                    physical_width as f64,
                    physical_height as f64,
                    scale_factor,
                );
                let content = layout::content_rect(width, height);
                if let Err(err) = relayout_host.relayout(content) {
                    tracing::error!("relayout failed: {err}");
                }
            });

            // Resolve `config.toml` / `state.json` (never hard-coded —
            // `paths.rs`), load or initialize them, and build the
            // `ServiceManager` that owns every service webview from here
            // on (design.md §2.2.5). A second, independent
            // `PlatformProfileBackend` is built for the manager: the one
            // above was already moved into `MultiwebviewHost::new`, and
            // the type has no `Clone` impl to share one between them.
            let config_dir = paths::config_dir(app)?;
            let config_path = paths::config_file(&config_dir);
            let data_dir = paths::data_dir(app)?;
            let state_path = paths::state_file(&data_dir);
            let manager_profile_backend = PlatformProfileBackend::new(app.handle())?;

            // On either failure, no services are started, the file is
            // never repaired or overwritten, and the error is kept on the
            // manager (logged here at error level, and surfaced verbatim
            // — for the config.toml case — or rebuilt into an equivalent
            // `ConfigError` — for the state.json case, since
            // `StateStore::open` returns a plain `AppError` with no
            // `file`/`key`/`reason` structure of its own — as `commands::
            // get_snapshot`'s `configError` field, Task 1.9).
            let manager: Arc<ServiceManager> = match config::load_or_init(&config_path) {
                Ok(loaded_config) => match StateStore::open(state_path.clone()) {
                    Ok(state) => Arc::new(ServiceManager::ready(
                        host.clone(),
                        manager_profile_backend,
                        app.handle().clone(),
                        config_path,
                        loaded_config,
                        state,
                    )),
                    Err(err) => {
                        tracing::error!("failed to open state store: {err}");
                        let state_error = config::ConfigError {
                            file: state_path,
                            key: None,
                            reason: err.to_string(),
                        };
                        Arc::new(ServiceManager::failed(
                            host.clone(),
                            manager_profile_backend,
                            app.handle().clone(),
                            config_path,
                            state_error,
                        ))
                    }
                },
                Err(err) => {
                    tracing::error!("failed to load configuration: {err}");
                    Arc::new(ServiceManager::failed(
                        host.clone(),
                        manager_profile_backend,
                        app.handle().clone(),
                        config_path,
                        err,
                    ))
                }
            };

            app.manage(manager.clone());
            // Spawns staggered service creation in the background
            // (`tauri::async_runtime`) and returns immediately; this hook
            // never blocks on it.
            ServiceManager::start(manager);

            // Initial relayout, so the shell and any service the
            // background startup task has already created by the time
            // this runs — plus any resize that happened between window
            // creation and here — all agree on the current content rect.
            let content = current_content_rect(&window)?;
            host.relayout(content)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_is_defined() {
        // `run()` starts the Tauri event loop and never returns in a real
        // application, so it cannot be called here. This test only checks
        // that the function exists with the expected signature.
        let _ = run as fn();
    }
}
