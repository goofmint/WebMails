//! [`MultiwebviewHost`]: the design's default [`super::WebviewHost`]
//! implementation, built on Tauri's `unstable` `Window::add_child` /
//! `WebviewBuilder` (design.md §2.2.4, §6.3 SP1, §8.1, §10).
//!
//! Every `add_child` / `WebviewBuilder` call in this crate lives in this
//! file (design.md §10). [`super::layout`] computes the rects; the
//! `to_position`/`to_size` helpers below are the only place a [`Rect`]
//! becomes a Tauri `LogicalPosition`/`LogicalSize`.
//!
//! **SP1 status:** design.md §6.3 lists SP1 ("does `add_child` multiwebview
//! ... behave on macOS 14 and Windows 11?") as a spike still to run before
//! M1 depends on its answer; `docs/spikes/SP1.md`'s decision is recorded
//! as "Pending — owner to fill in" as of this task. This module implements
//! `MultiwebviewHost` as design.md's stated default regardless, per this
//! task's own instructions — **the owner still needs to run SP1 and
//! confirm `MultiwebviewHost` (rather than the `ChildWindowHost` fallback,
//! design §8.1) before relying on this in a released build.**

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use tauri::webview::WebviewBuilder;
use tauri::{LogicalPosition, LogicalSize, Webview, WebviewUrl, Window, Wry};
use url::Url;

use crate::config::ServiceId;
use crate::error::{AppError, AppResult};
use crate::profile::ProfileBackend;

use super::layout::{self, Rect};
use super::{apply_common_settings, service_label, ServiceWebviewSpec, WebviewHost};

/// The label of the shell child webview (design.md §2.2.4).
const SHELL_LABEL: &str = "shell";

/// Registry state guarded by [`MultiwebviewHost::registry`]: every
/// resident service webview, which one (if any) is active, and the most
/// recently applied content rect — needed because [`WebviewHost::create`]
/// must place a brand-new service somewhere before the next
/// [`WebviewHost::relayout`] call arrives.
struct Registry {
    webviews: HashMap<ServiceId, Webview<Wry>>,
    active: Option<ServiceId>,
    content: Rect,
}

/// The design's default [`WebviewHost`]: one main [`Window`], one `shell`
/// child webview, and one child webview per resident service, all
/// `add_child`-ed onto the same window (design.md §2.2.4).
///
/// Generic over the [`ProfileBackend`] implementation rather than storing
/// `dyn ProfileBackend` — Eluma's own platform backend
/// (`profile::PlatformProfileBackend`) is already the single concrete
/// type `cfg(target_os = ...)` selects per build, so a generic parameter
/// here keeps `create` monomorphic and lets it be exercised in tests
/// against a fake backend without any platform-specific webview data
/// store.
pub struct MultiwebviewHost<B: ProfileBackend> {
    window: Window<Wry>,
    shell: Webview<Wry>,
    profile_backend: B,
    registry: Mutex<Registry>,
}

impl<B: ProfileBackend> MultiwebviewHost<B> {
    /// Builds the host for `window`: computes the initial content rect
    /// from the window's current physical size and scale factor, then
    /// creates the `shell` child at [`layout::shell_rect`] (design.md
    /// §2.2.4: "The shell is `add_child` at `(0, 0, SIDEBAR_WIDTH,
    /// height)`"), loading `url`.
    ///
    /// Does not create any service webviews — callers add those
    /// afterward with [`WebviewHost::create`] — and does not call
    /// [`WebviewHost::relayout`] itself; the caller (`lib.rs`'s `setup`)
    /// is responsible for the app's one initial `relayout` call, after it
    /// has finished creating whatever services it wants resident at
    /// startup.
    pub fn new(window: Window<Wry>, shell_url: WebviewUrl, profile_backend: B) -> AppResult<Self> {
        let content = current_content_rect(&window)?;

        let shell_rect = layout::shell_rect(content.height);
        let shell_builder = apply_common_settings(WebviewBuilder::new(SHELL_LABEL, shell_url));
        let shell = window
            .add_child(shell_builder, to_position(shell_rect), to_size(shell_rect))
            .map_err(AppError::from)?;

        Ok(MultiwebviewHost {
            window,
            shell,
            profile_backend,
            registry: Mutex::new(Registry {
                webviews: HashMap::new(),
                active: None,
                content,
            }),
        })
    }

    /// Locks [`Self::registry`], turning mutex poisoning into an
    /// `AppError` instead of panicking (project rule: no `unwrap`/`expect`
    /// outside tests). A poisoned lock means an earlier host call panicked
    /// while holding it — every method here returns before that could
    /// happen in normal operation, so this path is a last resort, not a
    /// path this module expects to hit.
    fn lock(&self) -> AppResult<MutexGuard<'_, Registry>> {
        self.registry
            .lock()
            .map_err(|_| AppError::Webview("host registry mutex poisoned".to_string()))
    }
}

/// Reads `window`'s current physical size and scale factor and converts
/// them to a logical [`layout::content_rect`] via
/// [`layout::physical_to_logical`].
fn current_content_rect(window: &Window<Wry>) -> AppResult<Rect> {
    let physical = window.inner_size().map_err(AppError::from)?;
    let scale = window.scale_factor().map_err(AppError::from)?;
    let (width, height) =
        layout::physical_to_logical(physical.width as f64, physical.height as f64, scale);
    Ok(layout::content_rect(width, height))
}

/// The one place a [`Rect`] becomes a Tauri `LogicalPosition` (design.md
/// §10 confines conversions like this to `host/multiwebview.rs`).
fn to_position(rect: Rect) -> LogicalPosition<f64> {
    LogicalPosition::new(rect.x, rect.y)
}

/// The one place a [`Rect`] becomes a Tauri `LogicalSize`.
fn to_size(rect: Rect) -> LogicalSize<f64> {
    LogicalSize::new(rect.width, rect.height)
}

/// The error every method below returns for an `id` with no registered
/// webview.
fn unknown_id(id: &ServiceId) -> AppError {
    AppError::Webview(format!("service '{id}' has no webview"))
}

impl<B: ProfileBackend + Send + Sync> WebviewHost for MultiwebviewHost<B> {
    fn create(&self, spec: ServiceWebviewSpec) -> AppResult<()> {
        let mut registry = self.lock()?;
        if registry.webviews.contains_key(&spec.id) {
            return Err(AppError::Webview(format!(
                "service '{}' already has a webview",
                spec.id
            )));
        }

        // A newly created service is resident but inactive until
        // `activate` is called (design.md §8.2), so it starts offscreen
        // at the most recently known content rect.
        let offscreen = layout::offscreen_rect(registry.content);

        let label = service_label(&spec.id);
        let builder = WebviewBuilder::new(label, WebviewUrl::External(spec.url))
            .initialization_script(spec.init_script)
            .on_page_load(spec.on_page_load);
        let builder = apply_common_settings(builder);
        let builder = self.profile_backend.apply(builder, spec.profile);

        let webview = self
            .window
            .add_child(builder, to_position(offscreen), to_size(offscreen))
            .map_err(AppError::from)?;

        registry.webviews.insert(spec.id, webview);
        Ok(())
    }

    fn destroy(&self, id: &ServiceId) -> AppResult<()> {
        let mut registry = self.lock()?;
        // Close first: if closing fails, the registry still tracks the
        // webview, so the caller can retry or report it.
        registry
            .webviews
            .get(id)
            .ok_or_else(|| unknown_id(id))?
            .close()
            .map_err(AppError::from)?;
        registry.webviews.remove(id);
        if registry.active.as_ref() == Some(id) {
            registry.active = None;
        }
        Ok(())
    }

    fn reload(&self, id: &ServiceId) -> AppResult<()> {
        let registry = self.lock()?;
        let webview = registry.webviews.get(id).ok_or_else(|| unknown_id(id))?;
        webview.reload().map_err(AppError::from)
    }

    fn navigate(&self, id: &ServiceId, url: Url) -> AppResult<()> {
        let registry = self.lock()?;
        let webview = registry.webviews.get(id).ok_or_else(|| unknown_id(id))?;
        webview.navigate(url).map_err(AppError::from)
    }

    fn activate(&self, id: &ServiceId) -> AppResult<()> {
        let mut registry = self.lock()?;
        if !registry.webviews.contains_key(id) {
            return Err(unknown_id(id));
        }
        registry.active = Some(id.clone());

        let active_rect = layout::active_rect(registry.content);
        let offscreen_rect = layout::offscreen_rect(registry.content);
        for (svc_id, webview) in registry.webviews.iter() {
            let rect = if svc_id == id {
                active_rect
            } else {
                offscreen_rect
            };
            webview
                .set_position(to_position(rect))
                .map_err(AppError::from)?;
            webview.set_size(to_size(rect)).map_err(AppError::from)?;
        }

        // Present in `registry.webviews` (checked above) and not removed
        // since, so this lookup cannot fail.
        if let Some(webview) = registry.webviews.get(id) {
            webview.set_focus().map_err(AppError::from)?;
        }
        Ok(())
    }

    fn relayout(&self, content: Rect) -> AppResult<()> {
        let mut registry = self.lock()?;
        registry.content = content;

        let shell_rect = layout::shell_rect(content.height);
        self.shell
            .set_position(to_position(shell_rect))
            .map_err(AppError::from)?;
        self.shell
            .set_size(to_size(shell_rect))
            .map_err(AppError::from)?;

        let active_rect = layout::active_rect(content);
        let offscreen_rect = layout::offscreen_rect(content);
        for (svc_id, webview) in registry.webviews.iter() {
            let rect = if registry.active.as_ref() == Some(svc_id) {
                active_rect
            } else {
                offscreen_rect
            };
            webview
                .set_position(to_position(rect))
                .map_err(AppError::from)?;
            webview.set_size(to_size(rect)).map_err(AppError::from)?;
        }
        Ok(())
    }
}
