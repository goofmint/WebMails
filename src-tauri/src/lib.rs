mod agent;
pub mod config;
pub mod error;
pub mod host;
pub mod paths;
pub mod profile;
pub mod state;

use tauri::{Manager, WebviewUrl};

use config::{ProfileName, ServiceId};
use host::{build_main_host, layout, ServiceWebviewSpec};
use profile::PlatformProfileBackend;
use state::State;

/// Fixed identifier and public URL for the one hard-coded service this
/// task creates at startup, so the content area shows something real
/// before service configuration exists.
///
/// Task 1.8 replaces this block with services built from `Config.services`
/// (design.md §2.2.5); this constant pair and the code that uses it go
/// away then.
const TEST_SERVICE_ID: &str = "test-service";
const TEST_SERVICE_URL: &str = "https://example.com";

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

            // One hard-coded test service, so M1 shows the shell plus a
            // real webview in the content area before service
            // configuration (Task 1.8) exists. The default profile's UUID
            // is resolved the same way a real service's will be — via
            // `profile::resolve` — but against an in-memory `State::empty()`
            // rather than the persisted store, since state persistence
            // isn't wired into startup yet (a later task).
            let default_profile = ProfileName::new("default")?;
            let test_service_id = ServiceId::new(TEST_SERVICE_ID)?;
            let mut scratch_state = State::empty();
            let profile_uuid =
                profile::resolve(&default_profile, &test_service_id, &mut scratch_state);

            host.create(ServiceWebviewSpec {
                id: test_service_id.clone(),
                url: TEST_SERVICE_URL.parse()?,
                profile: profile_uuid,
                init_script: String::new(),
                on_page_load: Box::new(|_webview, _payload| {}),
            })?;
            host.activate(&test_service_id)?;

            // Initial relayout, so the shell, the test service, and any
            // resize that happened between window creation and here all
            // agree on the current content rect.
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
