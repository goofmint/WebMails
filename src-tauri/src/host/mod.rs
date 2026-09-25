//! `WebviewHost`: the contract every webview-hosting backend implements
//! (design.md §2.2.4), plus the pieces every implementation shares —
//! [`ServiceWebviewSpec`], [`PageLoadHandler`], the `svc-<id>` label
//! helper, and the builder settings common to the shell and every service
//! webview.
//!
//! [`layout`] is the only submodule with no Tauri dependency. Everything
//! else here, and all of [`multiwebview`], needs Tauri's `unstable`
//! feature (`WebviewBuilder`, `Window::add_child`); design.md §10 confines
//! every call to those two APIs to `host/multiwebview.rs` — this module
//! only *defines* the settings [`multiwebview`] applies with them.

pub mod layout;
mod multiwebview;

pub use layout::{Rect, SIDEBAR_WIDTH};
pub use multiwebview::MultiwebviewHost;

use tauri::webview::{PageLoadPayload, WebviewBuilder};
use tauri::{Webview, Wry};
use url::Url;
use uuid::Uuid;

use crate::config::ServiceId;
use crate::error::AppResult;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_label_prefixes_with_svc_dash() {
        let id = ServiceId::new("gmail-personal").expect("valid service id");
        assert_eq!(service_label(&id), "svc-gmail-personal");
    }
}
