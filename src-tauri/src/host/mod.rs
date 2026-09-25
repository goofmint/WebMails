//! `WebviewHost`: the contract every webview-hosting backend implements
//! (design.md §2.2.4), plus the pieces every implementation shares —
//! [`ServiceWebviewSpec`], [`PageLoadHandler`], the `svc-<id>` label
//! helper, and the builder settings common to the shell and every service
//! webview.
//!
//! [`layout`] (and, behind Cargo feature `host-child-windows`,
//! [`child_geometry`]) is the only submodule with no Tauri dependency.
//! Everything else here, and all of [`multiwebview`] (and, behind that
//! same feature, [`child_windows`]), needs Tauri's `unstable` feature
//! (`WebviewBuilder`, `Window::add_child`); design.md §10 confines every
//! call to those two APIs to `host/multiwebview.rs` — this module only
//! *defines* the settings [`multiwebview`] applies with them.
//!
//! [`build_main_host`] is the one cfg branch point between the two
//! `WebviewHost` implementations: it builds the main window and picks
//! [`MultiwebviewHost`] (default) or [`ChildWindowHost`] (Cargo feature
//! `host-child-windows`, design.md §8.1's fallback; Task 1.7), so
//! `lib.rs`'s `setup` calls one function and gets back the same
//! `Arc<dyn WebviewHost>` either way, regardless of which concrete host
//! that build selected.

pub mod layout;
mod multiwebview;

#[cfg(feature = "host-child-windows")]
mod child_geometry;
#[cfg(feature = "host-child-windows")]
mod child_windows;

pub use layout::{Rect, SIDEBAR_WIDTH};
pub use multiwebview::MultiwebviewHost;

#[cfg(feature = "host-child-windows")]
pub use child_windows::ChildWindowHost;

use std::sync::Arc;

use tauri::webview::{PageLoadPayload, WebviewBuilder};
use tauri::{AppHandle, Webview, WebviewUrl, Window, Wry};
use url::Url;
use uuid::Uuid;

use crate::config::ServiceId;
use crate::error::AppResult;
use crate::profile::ProfileBackend;

/// The contract every webview-hosting backend implements (design.md
/// §2.2.4). [`MultiwebviewHost`] is the default, built on
/// `Window::add_child`. A second implementation, one borderless
/// `WebviewWindow` per service behind Cargo feature `host-child-windows`,
/// is Task 1.7 (design §8.1's fallback, `ChildWindowHost`).
pub trait WebviewHost: Send + Sync {
    /// Creates the service's webview. It is resident but not active — left
    /// offscreen (design.md §8.2) until [`activate`](Self::activate) is
    /// called. Errors if `spec.id` already has a webview.
    fn create(&self, spec: ServiceWebviewSpec) -> AppResult<()>;

    /// Destroys the service's webview and removes it from the host's
    /// registry. Errors if `id` has no webview.
    fn destroy(&self, id: &ServiceId) -> AppResult<()>;

    /// Reloads the service's current page. Errors if `id` has no webview.
    fn reload(&self, id: &ServiceId) -> AppResult<()>;

    /// Navigates the service's webview to `url`. Errors if `id` has no
    /// webview.
    fn navigate(&self, id: &ServiceId, url: Url) -> AppResult<()>;

    /// Moves `id`'s webview to the content rect and focuses it; every
    /// other registered service moves offscreen (design.md §2.2.4).
    /// Errors if `id` has no webview.
    fn activate(&self, id: &ServiceId) -> AppResult<()>;

    /// Repositions the shell and every registered service to match a new
    /// content rect — the active one at [`layout::active_rect`], every
    /// other one at [`layout::offscreen_rect`] — e.g. on window resize
    /// (design.md §2.2.4).
    fn relayout(&self, content: Rect) -> AppResult<()>;
}

/// A page-load callback for one service webview: reports origin changes to
/// the unread store (design.md §2.2.4's `on_page_load`; the store itself
/// is a later task, so this task only wires the callback shape through).
///
/// A boxed closure, not a named trait object like [`crate::profile::
/// ProfileBackend`]: `tauri::webview::WebviewBuilder::on_page_load` itself
/// expects exactly this `Fn(Webview<Wry>, PageLoadPayload<'_>)` shape, so
/// boxing it directly avoids inventing an extra trait around it.
pub type PageLoadHandler = Box<dyn Fn(Webview<Wry>, PageLoadPayload<'_>) + Send + Sync>;

/// Everything a host needs to create one service's webview (design.md
/// §2.2.4).
pub struct ServiceWebviewSpec {
    pub id: ServiceId,
    pub url: Url,
    pub profile: Uuid,
    /// The agent, plus injected config (design.md §2.2.6). The agent
    /// bundle and its injection format are later tasks; callers may pass
    /// an empty string until then.
    pub init_script: String,
    pub on_page_load: PageLoadHandler,
}

/// The `svc-<id>` webview label every host uses for a service (design.md
/// §2.2.4, §2.2.6), computed in exactly one place so `host` and
/// `agent_bridge` (a later task, which checks `webview.label() ==
/// format!("svc-{service_id}")`) never drift apart.
pub fn service_label(id: &ServiceId) -> String {
    format!("svc-{id}")
}

/// Windows browser arguments applied to *every* webview, shell included
/// (design.md §2.2.4, §10; SPEC.md §9.2). Quoted verbatim from design.md
/// §2.2.4.
///
/// `additional_browser_args` **replaces** wry's own default browser
/// arguments rather than appending to them (design.md §10), so this
/// constant re-includes wry's default
/// (`--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection`)
/// alongside the three throttling-survival flags that are SPEC.md §9.2's
/// entire Windows mitigation for background throttling.
///
/// **Unverified** (SPEC.md:303-333, spike SP4 in design.md §6.3): these
/// are Chromium switches that Microsoft does not document for WebView2,
/// and no authoritative source confirms they suppress WebView2's
/// background timer/rendering throttling. Treat this as an assumption
/// until SP4 validates it empirically — see SPEC.md §9.2's warning that
/// this "is the entire Windows mitigation."
///
/// Only referenced from `#[cfg(windows)]` code (in
/// [`apply_common_settings`]), so non-Windows builds would otherwise flag
/// it as dead code even though Windows builds use it.
#[cfg_attr(not(windows), allow(dead_code))]
pub const WEBVIEW2_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --disable-background-timer-throttling --disable-renderer-backgrounding --disable-backgrounding-occluded-windows";

/// Applies the per-webview settings design.md §2.2.4 lists as common to
/// every service — and, per this task's plan (`svc-<id>` and `shell`
/// alike need to survive backgrounding), applied identically to the shell
/// webview too: `background_throttling(Disabled)` on macOS,
/// `additional_browser_args(WEBVIEW2_ARGS)` on Windows (see
/// [`WEBVIEW2_ARGS`]'s doc comment for why, and for what is still
/// unverified). Every other builder setting — init script, page-load
/// handler, profile backend, navigation URL — is applied by the caller,
/// since those differ between the shell and each service.
pub fn apply_common_settings(builder: WebviewBuilder<Wry>) -> WebviewBuilder<Wry> {
    #[cfg(target_os = "macos")]
    let builder =
        builder.background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled);
    #[cfg(windows)]
    let builder = builder.additional_browser_args(WEBVIEW2_ARGS);
    builder
}

/// [`apply_common_settings`]'s `WebviewWindowBuilder` sibling, for
/// [`ChildWindowHost`]'s main window and per-service windows (Task 1.7):
/// the same two settings, applied to `tauri::webview::WebviewWindowBuilder`
/// rather than `WebviewBuilder`, since `WebviewWindow` exposes
/// `background_throttling`/`additional_browser_args` with the identical
/// name and signature (design.md §2.2.4's "Common builder settings, per
/// service" — these two apply to every webview regardless of which host
/// hosts it).
///
/// Only reachable behind Cargo feature `host-child-windows`, so it is
/// gated the same way rather than left for `#[allow(dead_code)]`.
#[cfg(feature = "host-child-windows")]
pub fn apply_common_settings_window<'a, M: tauri::Manager<Wry>>(
    builder: tauri::webview::WebviewWindowBuilder<'a, Wry, M>,
) -> tauri::webview::WebviewWindowBuilder<'a, Wry, M> {
    #[cfg(target_os = "macos")]
    let builder =
        builder.background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled);
    #[cfg(windows)]
    let builder = builder.additional_browser_args(WEBVIEW2_ARGS);
    builder
}

/// Builds the app's main window and the `WebviewHost` this build uses,
/// behind one cfg branch (design.md §8.1; Task 1.7's own instructions:
/// keep `lib.rs`'s diff to this call, since Task 1.8 edits `setup`
/// concurrently on another branch).
///
/// Returns the plain `Window<Wry>` underlying the host's main surface —
/// identical in shape for both implementations, since `ChildWindowHost`'s
/// main is a `WebviewWindow` and `Webview::window()` recovers its
/// `Window<Wry>` — so the caller's own resize/move event wiring (reading
/// `inner_size`/`scale_factor`, calling `on_window_event`) is the same
/// code regardless of which host this returns — together with the host
/// itself as `Arc<dyn WebviewHost>`, the same type `lib.rs` has `manage`d
/// since Task 1.6.
#[cfg(not(feature = "host-child-windows"))]
pub fn build_main_host<B: ProfileBackend + Send + Sync + 'static>(
    app: &AppHandle<Wry>,
    shell_url: WebviewUrl,
    title: &str,
    inner_size: (f64, f64),
    profile_backend: B,
) -> AppResult<(Window<Wry>, Arc<dyn WebviewHost>)> {
    let window = tauri::window::WindowBuilder::new(app, "main")
        .title(title)
        .inner_size(inner_size.0, inner_size.1)
        .build()
        .map_err(crate::error::AppError::from)?;
    let host = MultiwebviewHost::new(window.clone(), shell_url, profile_backend)?;
    Ok((window, Arc::new(host)))
}

/// [`build_main_host`]'s `host-child-windows` branch: builds `main` as a
/// `WebviewWindow` labelled [`child_windows::MAIN_LABEL`] (`"shell"`) and
/// wraps it in a [`ChildWindowHost`] (design.md §2.2.4's fallback bullet).
#[cfg(feature = "host-child-windows")]
pub fn build_main_host<B: ProfileBackend + Send + Sync + 'static>(
    app: &AppHandle<Wry>,
    shell_url: WebviewUrl,
    title: &str,
    inner_size: (f64, f64),
    profile_backend: B,
) -> AppResult<(Window<Wry>, Arc<dyn WebviewHost>)> {
    let builder =
        tauri::webview::WebviewWindowBuilder::new(app, child_windows::MAIN_LABEL, shell_url)
            .title(title)
            .inner_size(inner_size.0, inner_size.1);
    let builder = apply_common_settings_window(builder);
    let main = builder.build().map_err(crate::error::AppError::from)?;
    let window = AsRef::<Webview<Wry>>::as_ref(&main).window();
    let host = ChildWindowHost::new(app.clone(), main, profile_backend)?;
    Ok((window, Arc::new(host)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_label_prefixes_with_svc_dash() {
        let id = ServiceId::new("gmail-personal").expect("valid service id");
        assert_eq!(service_label(&id), "svc-gmail-personal");
    }
}
