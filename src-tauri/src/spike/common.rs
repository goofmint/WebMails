//! Shared pieces used by every `ELUMA_SPIKE` mode: the sidebar-width
//! constant, the offscreen/active layout math, the `switch_service`
//! command, and the per-service builder settings design §2.2.4 calls
//! "common ... per service" (background throttling on macOS, the shared
//! Windows browser-argument constant everywhere).
//!
//! All API used here was confirmed by reading the vendored source under
//! `~/.cargo/registry/src/*/tauri-2.11.6` (not guessed) — see `SPIKE.md`
//! for the specific citations (`Window::add_child`, `WebviewBuilder`,
//! `CapabilityBuilder`, `WindowEvent`, `Uuid::new_v5`, etc.).

use std::sync::Mutex;

use tauri::webview::WebviewBuilder;
use tauri::{LogicalPosition, LogicalSize, Webview, Window, Wry};

/// Width of the shell sidebar, in logical pixels (design §2.2.4:
/// "SIDEBAR_WIDTH is a single Rust constant (64px)"). Every mode reads
/// this one constant instead of duplicating the number.
pub const SIDEBAR_WIDTH: f64 = 64.0;

/// Windows-only browser arguments applied to **every** webview, shell
/// included (design §2.2.4 / SP4, tasks.md Task 0.6). Quoted verbatim
/// from design.md §2.2.4. `additional_browser_args` *replaces* wry's own
/// default arguments rather than adding to them (design §10); this
/// constant re-includes the ones wry normally sets, per that note.
// Only referenced from `#[cfg(windows)]` code (here and in `sp4.rs`'s
// control-run toggle), so non-Windows builds would otherwise flag it as
// dead code even though Windows builds use it.
#[cfg_attr(not(windows), allow(dead_code))]
pub const WEBVIEW2_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --disable-background-timer-throttling --disable-renderer-backgrounding --disable-backgrounding-occluded-windows";

/// Per-window harness state: the labels of the service children, in
/// sidebar order, and which one is active. `switch_service` and the
/// resize/scale-factor handlers both read and mutate this through
/// managed Tauri state.
pub struct HostState {
    pub window: Window,
    pub shell: Webview,
    pub services: Vec<Webview>,
    pub active_index: Mutex<usize>,
}

/// A child service description used only to build the shell's button
/// list (`window.__ELUMA_SPIKE__`). Not the same as the `Webview` handle.
/// `label` is kept alongside `display`/`url` (the two fields
/// `shell_init_script` currently serializes) so callers can build this
/// struct from the same webview label used to create the child, without
/// every caller needing it read back out here too.
pub struct ChildInfo {
    #[allow(dead_code)]
    pub label: String,
    pub display: String,
    pub url: String,
}

/// Current logical size of `window` (physical size converted with the
/// window's own `scale_factor()`). Used by mode `setup` functions to size
/// the shell sidebar, which spans the window's full height before any
/// service child exists yet.
pub fn logical_window_size(window: &Window) -> tauri::Result<LogicalSize<f64>> {
    let physical = window.inner_size()?;
    let scale = window.scale_factor()?;
    Ok(physical.to_logical(scale))
}

/// Builds a child `WebviewBuilder` with the settings design §2.2.4 lists
/// as common to every service webview. Mode-specific code (`sp1`..`sp4`)
/// chains its own additions (init scripts, profile identifiers) on top.
///
/// Always applies the platform settings — SP4's "control run, no
/// constant" comparison (`docs/spikes/SP4.md` Task 4) opts *out* of this
/// helper for its one test page instead (see `sp4.rs`'s
/// `ELUMA_SPIKE_SP4_NO_ARGS`), so every other call site here keeps the
/// simple, unconditional behaviour.
pub fn base_child_builder(label: &str, url: tauri::WebviewUrl) -> WebviewBuilder<Wry> {
    let builder = WebviewBuilder::new(label, url);
    #[cfg(target_os = "macos")]
    let builder =
        builder.background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled);
    #[cfg(windows)]
    let builder = builder.additional_browser_args(WEBVIEW2_ARGS);
    builder
}

/// Computes the logical position and size for service child `index`,
/// given which index is currently active. The active child fills the
/// content area to the right of the sidebar; every other child is placed
/// fully outside the window's bounds (never hidden — design §2.2.4, and
/// this also sidesteps depending on z-order between children, tauri#11376).
pub fn geometry(
    window: &Window,
    index: usize,
    active_index: usize,
) -> tauri::Result<(LogicalPosition<f64>, LogicalSize<f64>)> {
    let physical = window.inner_size()?;
    let scale = window.scale_factor()?;
    let logical: LogicalSize<f64> = physical.to_logical(scale);
    let service_w = (logical.width - SIDEBAR_WIDTH).max(0.0);
    let service_h = logical.height;
    let pos = if index == active_index {
        LogicalPosition::new(SIDEBAR_WIDTH, 0.0)
    } else {
        // Shove it fully past the left edge: window width plus the
        // sidebar width, so it never overlaps the visible content rect
        // regardless of the window's current size.
        LogicalPosition::new(-(service_w + SIDEBAR_WIDTH), 0.0)
    };
    Ok((pos, LogicalSize::new(service_w, service_h)))
}

/// Repositions the shell and every service child to match the window's
/// current size and the currently active index. Called once at startup
/// (with the freshly-created geometry) and again from
/// `WindowEvent::Resized` / `WindowEvent::ScaleFactorChanged`.
pub fn relayout(state: &HostState) -> tauri::Result<()> {
    let physical = state.window.inner_size()?;
    let scale = state.window.scale_factor()?;
    let logical: LogicalSize<f64> = physical.to_logical(scale);

    state.shell.set_position(LogicalPosition::new(0.0, 0.0))?;
    state
        .shell
        .set_size(LogicalSize::new(SIDEBAR_WIDTH, logical.height))?;

    let active = *state.active_index.lock().unwrap();
    for (i, webview) in state.services.iter().enumerate() {
        let (pos, size) = geometry(&state.window, i, active)?;
        webview.set_position(pos)?;
        webview.set_size(size)?;
    }
    if let Some(active_webview) = state.services.get(active) {
        active_webview.set_focus()?;
    }
    Ok(())
}

/// Registers the resize/scale-factor relayout handler for `window`. Every
/// mode calls this once, after `HostState` has been `app.manage()`-d.
pub fn install_relayout_handler(app_handle: tauri::AppHandle, window: &Window) {
    use tauri::Manager;
    window.on_window_event(move |event| {
        if matches!(
            event,
            tauri::WindowEvent::Resized(_) | tauri::WindowEvent::ScaleFactorChanged { .. }
        ) {
            let state = app_handle.state::<HostState>();
            if let Err(err) = relayout(&state) {
                eprintln!("[spike] relayout on resize/scale-factor-change failed: {err}");
            }
        }
    });
}

/// Escapes an arbitrary Rust string into a double-quoted JS/JSON string
/// literal, using `{:?}` (`Debug` for `&str`), so it can be embedded in a
/// generated `initialization_script` without pulling in `serde_json` for
/// a handful of internally-controlled values (labels, URLs).
pub fn js_string(value: &str) -> String {
    format!("{value:?}")
}

/// Builds the `window.__ELUMA_SPIKE__ = {...}` initialization script for
/// the shell page: which mode is running, which index is active, and the
/// ordered list of child labels/urls the shell renders as buttons.
pub fn shell_init_script(mode: &str, children: &[ChildInfo], active_index: usize) -> String {
    let children_js = children
        .iter()
        .map(|c| {
            format!(
                "{{\"label\":{label},\"url\":{url}}}",
                label = js_string(&c.display),
                url = js_string(&c.url),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "window.__ELUMA_SPIKE__ = {{\"mode\":{mode},\"activeIndex\":{active_index},\"children\":[{children_js}]}};",
        mode = js_string(mode),
    )
}

/// `switch_service(index)` — the only command the shell webview may call
/// to change which service child is active. The ACL restricts *who* may
/// call it (only the `shell` webview label, via
/// `capabilities/spike-local.json`); this command additionally checks the
/// index bounds and reports the calling webview's label for the harness
/// log, matching the manual-checklist expectation that this is visibly
/// shell-only.
#[tauri::command]
pub fn switch_service(
    webview: tauri::Webview,
    state: tauri::State<'_, HostState>,
    index: usize,
) -> Result<String, String> {
    if index >= state.services.len() {
        return Err(format!(
            "index {index} out of range (0..{})",
            state.services.len()
        ));
    }
    *state.active_index.lock().unwrap() = index;
    relayout(&state).map_err(|err| err.to_string())?;
    Ok(format!(
        "switch_service({index}) ok, called from webview '{}'",
        webview.label()
    ))
}
