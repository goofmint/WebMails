//! [`ChildWindowHost`]: the fallback [`super::WebviewHost`] implementation
//! behind Cargo feature `host-child-windows` (design.md §2.2.4's fallback
//! bullet, §8.1, §10; Task 1.7; SP1's contingency in design.md §6.3: "SP1
//! fails (multiwebview unusable): switch the default to `ChildWindowHost`.
//! The trait isolates the change.").
//!
//! Where [`super::multiwebview::MultiwebviewHost`] is one `Window` with a
//! `shell` child webview plus one child webview per service (all
//! `add_child`-ed onto that window), `ChildWindowHost` is one
//! `WebviewWindow` — labelled `shell`, so `capabilities/default.json`'s
//! existing `"webviews": ["shell", "settings"]` scope reaches it
//! unchanged — hosting the shell content directly, plus one borderless
//! `WebviewWindow` per service, each `.parent(&main)`. [`child_geometry`]
//! computes their physical, desktop-absolute frames; nothing else in this
//! module (or [`super::multiwebview`]) needs a `WebviewWindow::parent`
//! call for anything but the initial `.parent(&self.main)` on `create`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use tauri::webview::WebviewWindowBuilder;
use tauri::{
    AppHandle, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WindowEvent, Wry,
};
use url::Url;

use crate::config::ServiceId;
use crate::error::{AppError, AppResult};
use crate::profile::ProfileBackend;

use super::child_geometry::{self, PhysicalRect};
use super::{apply_common_settings_window, service_label, Rect, ServiceWebviewSpec, WebviewHost};

/// The label `ChildWindowHost`'s main `WebviewWindow` uses (design.md
/// §2.2.4's fallback bullet: "main is a WebviewWindow hosting the
/// shell"). Deliberately the *same* label
/// [`super::multiwebview::MultiwebviewHost`] gives its shell child
/// webview, not `"main"`: `capabilities/default.json` scopes
/// `"webviews": ["shell", "settings"]` by webview label, and a
/// `WebviewWindow` uses one label for both its window and its webview —
/// so labelling it `"shell"` here is what makes that same capabilities
/// file, unmodified, reach this host's shell too, while `svc-<id>`
/// service windows stay outside its scope exactly as they do for
/// `MultiwebviewHost` (design.md §9.2).
pub(crate) const MAIN_LABEL: &str = "shell";

/// The physical geometry of the main window this host lays services out
/// against — [`Registry`]'s cached copy, refreshed by
/// [`ChildWindowHost::new`] and every [`WebviewHost::relayout`] call.
#[derive(Debug, Clone, Copy)]
struct ParentGeometry {
    position: (i32, i32),
    size: (u32, u32),
    scale_factor: f64,
}

/// Registry state guarded by [`ChildWindowHost::registry`]: every resident
/// service window, which one (if any) is active, and the most recently
/// known parent geometry — needed for the same reason
/// [`super::multiwebview::MultiwebviewHost`]'s `Registry` caches its
/// content rect: [`WebviewHost::create`] must place a brand-new service
/// somewhere before the next [`WebviewHost::relayout`] call arrives.
struct Registry {
    windows: HashMap<ServiceId, WebviewWindow<Wry>>,
    active: Option<ServiceId>,
    parent: ParentGeometry,
}

/// The fallback [`WebviewHost`] (design.md §2.2.4, §8.1): one main
/// `WebviewWindow` hosting the shell, and one borderless child
/// `WebviewWindow` per resident service, each `.parent(&main)`.
///
/// Generic over the [`ProfileBackend`] implementation for the same reason
/// [`super::multiwebview::MultiwebviewHost`] is (see its own doc comment):
/// it keeps `create` monomorphic and testable against a fake backend.
pub struct ChildWindowHost<B: ProfileBackend> {
    app: AppHandle<Wry>,
    main: WebviewWindow<Wry>,
    profile_backend: B,
    /// `Arc`-wrapped (unlike `MultiwebviewHost`'s plain `Mutex`) because
    /// [`ChildWindowHost::new`] hands a clone to `main`'s own `Destroyed`
    /// listener, which outlives the `new` call and needs to reach the same
    /// registry without a back-reference to `self` (design.md §2.2.4's
    /// fallback bullet: service windows close when `main` is destroyed).
    registry: Arc<Mutex<Registry>>,
}

impl<B: ProfileBackend> ChildWindowHost<B> {
    /// Builds the host around an already-constructed `main`
    /// [`WebviewWindow`] (built by [`super::build_main_host`]'s
    /// `host-child-windows` branch, labelled [`MAIN_LABEL`]).
    ///
    /// Registers `main`'s own `Destroyed` handler here — closing every
    /// resident service window — rather than asking the caller to extend
    /// its own window-event wiring for it (Task 1.7's brief: keep
    /// `lib.rs`'s diff minimal, since Task 1.8 edits `setup` concurrently
    /// on another branch). `Window::on_window_event` supports more than
    /// one registered listener per window (each call returns its own
    /// `WindowEventId`), so this coexists with the caller's own
    /// Resized/ScaleFactorChanged/Moved listener on the same window
    /// without either replacing the other.
    pub fn new(
        app: AppHandle<Wry>,
        main: WebviewWindow<Wry>,
        profile_backend: B,
    ) -> AppResult<Self> {
        let parent = read_parent_geometry(&main)?;

        let registry = Arc::new(Mutex::new(Registry {
            windows: HashMap::new(),
            active: None,
            parent,
        }));

        let destroy_registry = registry.clone();
        main.on_window_event(move |event| {
            if !matches!(event, WindowEvent::Destroyed) {
                return;
            }
            let mut registry = match destroy_registry.lock() {
                Ok(registry) => registry,
                Err(_) => {
                    tracing::error!(
                        "child window host: registry mutex poisoned while closing service windows on main destroy"
                    );
                    return;
                }
            };
            for (id, window) in registry.windows.drain() {
                if let Err(err) = window.close() {
                    tracing::error!("child window host: closing service '{id}' on main destroy failed: {err}");
                }
            }
            registry.active = None;
        });

        Ok(ChildWindowHost {
            app,
            main,
            profile_backend,
            registry,
        })
    }

    /// Locks [`Self::registry`], turning mutex poisoning into an
    /// `AppError` instead of panicking (project rule: no `unwrap`/`expect`
    /// outside tests) — same reasoning as
    /// [`super::multiwebview::MultiwebviewHost::lock`].
    fn lock(&self) -> AppResult<MutexGuard<'_, Registry>> {
        self.registry
            .lock()
            .map_err(|_| AppError::Webview("host registry mutex poisoned".to_string()))
    }

    /// Reads every current monitor's physical bounding rect fresh
    /// (`Window::available_monitors`), converted to [`PhysicalRect`].
    /// Queried live on every call that needs an offscreen placement,
    /// rather than cached alongside [`ParentGeometry`], because the
    /// monitor arrangement can change independently of the main window
    /// (a display connected or disconnected) between layout calls.
    fn physical_monitors(&self) -> AppResult<Vec<PhysicalRect>> {
        let monitors = self.main.available_monitors().map_err(AppError::from)?;
        Ok(monitors
            .iter()
            .map(|monitor| PhysicalRect {
                x: monitor.position().x,
                y: monitor.position().y,
                width: monitor.size().width,
                height: monitor.size().height,
            })
            .collect())
    }
}

/// Reads `main`'s current physical inner (client-area) position, physical
/// inner size and scale factor — the three inputs
/// [`child_geometry::service_frame`] needs.
fn read_parent_geometry(main: &WebviewWindow<Wry>) -> AppResult<ParentGeometry> {
    let position = main.inner_position().map_err(AppError::from)?;
    let size = main.inner_size().map_err(AppError::from)?;
    let scale_factor = main.scale_factor().map_err(AppError::from)?;
    Ok(ParentGeometry {
        position: (position.x, position.y),
        size: (size.width, size.height),
        scale_factor,
    })
}

/// Applies a computed [`PhysicalRect`] to a service `WebviewWindow` with
/// explicit `Physical` position/size types (design.md §10: conversions
/// like this stay explicit, never implicitly logical, since these are
/// desktop-absolute physical coordinates, not window-relative logical
/// ones).
fn apply_frame(window: &WebviewWindow<Wry>, frame: PhysicalRect) -> AppResult<()> {
    window
        .set_position(PhysicalPosition::new(frame.x, frame.y))
        .map_err(AppError::from)?;
    window
        .set_size(PhysicalSize::new(frame.width, frame.height))
        .map_err(AppError::from)
}

/// The error every method below returns for an `id` with no registered
/// window — same `AppError` variant and message
/// [`super::multiwebview::unknown_id`] uses, kept as its own copy here
/// rather than exported from `multiwebview`, since that module is 1.6's
/// and this task's brief is not to touch it.
fn unknown_id(id: &ServiceId) -> AppError {
    AppError::Webview(format!("service '{id}' has no webview"))
}

/// The error `create`/`activate`/`relayout` return when
/// [`child_geometry::service_frame`] returns `None` for every branch they
/// need: the parent's physical size is zero, no wider than the physical
/// sidebar width, or (offscreen placement) there are no monitors to place
/// it left of. Never silently falls back to some other placement (project
/// rule: no fallback defaults) — the caller sees an explicit error
/// instead.
fn cannot_layout() -> AppError {
    AppError::Webview(
        "child window host: cannot compute layout (zero-sized parent, parent no wider than the \
         sidebar, or no monitors)"
            .to_string(),
    )
}

impl<B: ProfileBackend + Send + Sync> WebviewHost for ChildWindowHost<B> {
    fn create(&self, spec: ServiceWebviewSpec) -> AppResult<()> {
        let mut registry = self.lock()?;
        if registry.windows.contains_key(&spec.id) {
            return Err(AppError::Webview(format!(
                "service '{}' already has a webview",
                spec.id
            )));
        }

        // A newly created service is resident but inactive until
        // `activate` is called (design.md §8.2), so it starts offscreen.
        let monitors = self.physical_monitors()?;
        let offscreen = child_geometry::service_frame(
            registry.parent.position,
            registry.parent.size,
            registry.parent.scale_factor,
            &monitors,
            false,
        )
        .ok_or_else(cannot_layout)?;

        let label = service_label(&spec.id);
        let on_page_load = spec.on_page_load;
        let builder = WebviewWindowBuilder::new(&self.app, label, WebviewUrl::External(spec.url))
            .parent(&self.main)
            .map_err(AppError::from)?
            .decorations(false)
            .focused(false)
            .position(f64::from(offscreen.x), f64::from(offscreen.y))
            .inner_size(f64::from(offscreen.width), f64::from(offscreen.height))
            .initialization_script(spec.init_script)
            .on_page_load(move |window, payload| on_page_load(window.as_ref().clone(), payload));
        let builder = apply_common_settings_window(builder);
        let builder = self.profile_backend.apply_window(builder, spec.profile);

        let window = builder.build().map_err(AppError::from)?;

        registry.windows.insert(spec.id, window);
        Ok(())
    }

    fn destroy(&self, id: &ServiceId) -> AppResult<()> {
        let mut registry = self.lock()?;
        // Close first: if closing fails, the registry still tracks the
        // window, so the caller can retry or report it (mirrors
        // `MultiwebviewHost::destroy`).
        registry
            .windows
            .get(id)
            .ok_or_else(|| unknown_id(id))?
            .close()
            .map_err(AppError::from)?;
        registry.windows.remove(id);
        if registry.active.as_ref() == Some(id) {
            registry.active = None;
        }
        Ok(())
    }

    fn reload(&self, id: &ServiceId) -> AppResult<()> {
        let registry = self.lock()?;
        let window = registry.windows.get(id).ok_or_else(|| unknown_id(id))?;
        window.reload().map_err(AppError::from)
    }

    fn navigate(&self, id: &ServiceId, url: Url) -> AppResult<()> {
        let registry = self.lock()?;
        let window = registry.windows.get(id).ok_or_else(|| unknown_id(id))?;
        window.navigate(url).map_err(AppError::from)
    }

    fn activate(&self, id: &ServiceId) -> AppResult<()> {
        let mut registry = self.lock()?;
        if !registry.windows.contains_key(id) {
            return Err(unknown_id(id));
        }
        registry.active = Some(id.clone());

        let monitors = self.physical_monitors()?;
        let active_frame = child_geometry::service_frame(
            registry.parent.position,
            registry.parent.size,
            registry.parent.scale_factor,
            &monitors,
            true,
        )
        .ok_or_else(cannot_layout)?;
        let offscreen_frame = child_geometry::service_frame(
            registry.parent.position,
            registry.parent.size,
            registry.parent.scale_factor,
            &monitors,
            false,
        )
        .ok_or_else(cannot_layout)?;

        for (svc_id, window) in registry.windows.iter() {
            let frame = if svc_id == id {
                active_frame
            } else {
                offscreen_frame
            };
            apply_frame(window, frame)?;
        }

        // Present in `registry.windows` (checked above) and not removed
        // since, so this lookup cannot fail.
        if let Some(window) = registry.windows.get(id) {
            window.set_focus().map_err(AppError::from)?;
        }
        Ok(())
    }

    /// Ignores `_content`: unlike `MultiwebviewHost`, whose child webviews
    /// live in the main window's own logical coordinate space, this
    /// host's service windows are separate top-level OS windows placed in
    /// *physical*, desktop-absolute coordinates. `_content` — the logical,
    /// window-relative rect the shared resize/move handler computes for
    /// `MultiwebviewHost` — is the wrong coordinate space for that, so
    /// this reads `self.main`'s live physical position, size and scale
    /// factor instead (the plan's own instruction: relayout "calls Phase
    /// 1's function from the parent's physical geometry and monitor
    /// list").
    fn relayout(&self, _content: Rect) -> AppResult<()> {
        let mut registry = self.lock()?;
        registry.parent = read_parent_geometry(&self.main)?;
        let parent = registry.parent;

        let monitors = self.physical_monitors()?;
        let active_frame = child_geometry::service_frame(
            parent.position,
            parent.size,
            parent.scale_factor,
            &monitors,
            true,
        )
        .ok_or_else(cannot_layout)?;
        let offscreen_frame = child_geometry::service_frame(
            parent.position,
            parent.size,
            parent.scale_factor,
            &monitors,
            false,
        )
        .ok_or_else(cannot_layout)?;

        for (svc_id, window) in registry.windows.iter() {
            let frame = if registry.active.as_ref() == Some(svc_id) {
                active_frame
            } else {
                offscreen_frame
            };
            apply_frame(window, frame)?;
        }
        Ok(())
    }
}
