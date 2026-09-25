//! Owns the live set of service webviews and keeps it in sync with
//! `config.toml` (design.md §2.2.5).
//!
//! [`ServiceManager`] holds the last-applied [`Config`], the config file
//! path, the open [`StateStore`], a reference to the [`WebviewHost`] and
//! the profile backend, and enough bookkeeping — which service ids have a
//! live webview, which are still waiting for their staggered startup
//! turn, which is active, and any per-service creation error — to
//! reconcile a [`ConfigEdit`] into webview operations.
//! [`reconcile::diff`] computes *what* to do, purely; this module is the
//! only place that actually calls the [`WebviewHost`] or the profile
//! backend.
//!
//! Every public method locks [`ServiceManager::inner`] — a
//! [`tauri::async_runtime::Mutex`] (an async mutex, so the lock survives
//! across `.await`) — for its whole body, which serializes edits and
//! startup creation against each other.
//!
//! No `#[tauri::command]`s live here: Task 1.9 wraps these methods as
//! commands and defines the full snapshot the shell receives.

mod reconcile;
mod slug;

use reconcile::WebviewOp;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tauri::async_runtime::Mutex;
use tauri::{AppHandle, Emitter, Wry};
use url::Url;

use crate::config::{self, Config, ConfigEdit, IconSource, ProfileName, ServiceConfig, ServiceId};
use crate::error::{AppError, AppResult};
use crate::host::{ServiceWebviewSpec, WebviewHost};
use crate::profile::{self, PlatformProfileBackend, ProfileBackend, ProfileKey};
use crate::state::StateStore;

/// Delay between each service's staggered creation at startup (design.md
/// §2.2.5).
pub const STARTUP_STAGGER: Duration = Duration::from_millis(1500);

/// The event emitted to the `shell` webview whenever an edit changes the
/// set, order or per-service metadata of services (design.md §2.2.5).
/// Task 1.9 defines its real payload (a full snapshot); this task emits
/// it with a minimal placeholder payload.
const SERVICES_CHANGED_EVENT: &str = "services-changed";

/// The webview label every shell-only event (this one included) targets
/// (design.md §2.2.4).
const SHELL_LABEL: &str = "shell";

/// Whether the removed service's `profile` name marks it as having a
/// private, per-service profile (design.md §2.2.3's `"isolated"`
/// reserved name) — the only case [`ServiceManager::remove_service`] ever
/// considers removing on-disk profile data for.
const ISOLATED_PROFILE_NAME: &str = "isolated";

/// Everything [`ServiceManager`] needs once its `Config` and
/// [`StateStore`] have loaded successfully.
struct Ready {
    /// The most recently applied `Config` (design.md §2.2.1). Updated by
    /// every successful [`ServiceManager::apply_edit_inner`] call, even
    /// when a webview operation the edit implies then fails.
    config: Config,
    state: StateStore,
    /// Service ids with a live webview: `host.create` has succeeded for
    /// them and `host.destroy` has not since.
    created: BTreeSet<ServiceId>,
    /// Service ids from the `Config` `ServiceManager::start` began with,
    /// in sidebar order, still waiting for their staggered startup turn.
    /// Drained as `run_startup` reaches each one; empty once startup has
    /// processed every id it started with.
    pending: Vec<ServiceId>,
    active: Option<ServiceId>,
    /// Per-service webview creation failures (design.md §5.1's
    /// `NeedsAttention(CreateFailed)`), keyed by service id, valued by
    /// the failure's display message. Task 2.3 moves this into the
    /// `unread` store's `ServiceStatus` instead; until then this is just
    /// where the failure is kept.
    create_errors: BTreeMap<ServiceId, String>,
}

/// Either [`Ready`], or the error [`config::load_or_init`] /
/// [`StateStore::open`] failed with at startup (design.md §5.1).
enum ManagerState {
    /// No services were started and the file on disk was never touched.
    /// Kept so a later command (Task 1.9) can surface it to the shell.
    /// Editing (`apply_edit`, `add_service`, `remove_service`) is refused
    /// in this state: there is no valid `Config` to build an edit
    /// against, and this app never repairs an existing file.
    Failed(AppError),
    Ready(Ready),
}

/// Reconciles service webviews with `config.toml` (module doc has the
/// full picture).
pub struct ServiceManager {
    host: Arc<dyn WebviewHost>,
    profile_backend: PlatformProfileBackend,
    app_handle: AppHandle<Wry>,
    config_path: PathBuf,
    inner: Mutex<ManagerState>,
}

impl ServiceManager {
    /// Builds a manager already holding a loaded `Config` and open
    /// `StateStore` — the normal case.
    pub fn ready(
        host: Arc<dyn WebviewHost>,
        profile_backend: PlatformProfileBackend,
        app_handle: AppHandle<Wry>,
        config_path: PathBuf,
        config: Config,
        state: StateStore,
    ) -> Self {
        ServiceManager {
            host,
            profile_backend,
            app_handle,
            config_path,
            inner: Mutex::new(ManagerState::Ready(Ready {
                config,
                state,
                created: BTreeSet::new(),
                pending: Vec::new(),
                active: None,
                create_errors: BTreeMap::new(),
            })),
        }
    }

    /// Builds a manager that failed to load its configuration or state at
    /// startup (design.md §5.1): no services are started, the file is
    /// never repaired, and `error` is kept for a later command to
    /// surface.
    pub fn failed(
        host: Arc<dyn WebviewHost>,
        profile_backend: PlatformProfileBackend,
        app_handle: AppHandle<Wry>,
        config_path: PathBuf,
        error: AppError,
    ) -> Self {
        ServiceManager {
            host,
            profile_backend,
            app_handle,
            config_path,
            inner: Mutex::new(ManagerState::Failed(error)),
        }
    }

    /// Starts staggered service creation (design.md §2.2.5). If `manager`
    /// failed to load its configuration or state, logs the kept error
    /// and starts nothing. Otherwise stages every configured service's
    /// id, in sidebar order, and spawns a `tauri::async_runtime` task
    /// that creates them one at a time, [`STARTUP_STAGGER`] apart,
    /// activating the first one that succeeds. One service's creation
    /// failure never stops the others.
    ///
    /// Returns immediately; the caller (`lib.rs`'s `setup` hook) never
    /// blocks on this.
    pub fn start(manager: Arc<Self>) {
        tauri::async_runtime::spawn(async move {
            manager.run_startup().await;
        });
    }

    async fn run_startup(&self) {
        let pending = {
            let mut guard = self.inner.lock().await;
            match &mut *guard {
                ManagerState::Failed(err) => {
                    tracing::error!("service manager did not start: {err}");
                    return;
                }
                ManagerState::Ready(ready) => {
                    let ids: Vec<ServiceId> =
                        ready.config.services.iter().map(|s| s.id.clone()).collect();
                    ready.pending = ids.clone();
                    ids
                }
            }
        };

        let total = pending.len();
        let mut activated_first = false;

        for (index, id) in pending.into_iter().enumerate() {
            match self.create_one(&id).await {
                Ok(()) => {
                    if !activated_first && self.is_created(&id).await {
                        match self.host.activate(&id) {
                            Ok(()) => {
                                self.set_active(&id).await;
                                activated_first = true;
                            }
                            Err(err) => tracing::error!(
                                "failed to activate first started service '{id}': {err}"
                            ),
                        }
                    }
                }
                Err(err) => {
                    tracing::error!("failed to create service '{id}' at startup: {err}");
                }
            }

            if index + 1 < total {
                tokio::time::sleep(STARTUP_STAGGER).await;
            }
        }
    }

    async fn is_created(&self, id: &ServiceId) -> bool {
        let guard = self.inner.lock().await;
        matches!(&*guard, ManagerState::Ready(ready) if ready.created.contains(id))
    }

    async fn set_active(&self, id: &ServiceId) {
        let mut guard = self.inner.lock().await;
        if let ManagerState::Ready(ready) = &mut *guard {
            ready.active = Some(id.clone());
        }
    }

    /// Locks, then delegates to [`Self::create_one_locked`].
    async fn create_one(&self, id: &ServiceId) -> AppResult<()> {
        let mut guard = self.inner.lock().await;
        match &mut *guard {
            ManagerState::Failed(err) => Err(not_ready(err)),
            ManagerState::Ready(ready) => self.create_one_locked(ready, id),
        }
    }

    /// Creates `id`'s webview, assuming `self.inner`'s lock is already
    /// held (`ready` borrows through it).
    ///
    /// Re-checks the *current* `ready.config` right before creating, and
    /// only proceeds if `id` still names a configured service and does
    /// not already have a webview — both a no-op `Ok(())`, not an error,
    /// so a service removed or already created before its startup turn
    /// (or before a `Create`/`Recreate` op runs) is silently skipped. On
    /// the remaining path, resolves the profile UUID inside
    /// `StateStore::update` (design.md §2.2.3), so a freshly minted UUID
    /// is captured by that update's own dirty flag, then calls
    /// `host.create`. Records the outcome in `created`/`create_errors`
    /// either way; callers log the error themselves, with context-specific
    /// wording (startup vs. an edit).
    fn create_one_locked(&self, ready: &mut Ready, id: &ServiceId) -> AppResult<()> {
        // A service's startup turn has now arrived, whether or not it
        // still exists to be created — see `pending`'s doc comment.
        ready.pending.retain(|pending_id| pending_id != id);

        let Some(service) = ready.config.services.iter().find(|s| s.id == *id).cloned() else {
            return Ok(());
        };
        if ready.created.contains(id) {
            return Ok(());
        }

        let uuid_result = ready
            .state
            .update(|state| profile::resolve(&service.profile, &service.id, state));
        let uuid = match uuid_result {
            Ok(uuid) => uuid,
            Err(err) => {
                ready.create_errors.insert(id.clone(), err.to_string());
                return Err(err);
            }
        };

        let spec = ServiceWebviewSpec {
            id: service.id.clone(),
            url: service.url.clone(),
            profile: uuid,
            init_script: String::new(),
            on_page_load: Box::new(|_webview, _payload| {}),
        };

        match self.host.create(spec) {
            Ok(()) => {
                ready.created.insert(id.clone());
                ready.create_errors.remove(id);
                Ok(())
            }
            Err(err) => {
                ready.create_errors.insert(id.clone(), err.to_string());
                Err(err)
            }
        }
    }

    /// Applies every op in `plan.ops`, assuming `self.inner`'s lock is
    /// already held.
    fn execute_op(&self, ready: &mut Ready, op: &WebviewOp) {
        match op {
            WebviewOp::Create(id) => {
                if let Err(err) = self.create_one_locked(ready, id) {
                    tracing::error!("failed to create service '{id}': {err}");
                }
            }
            WebviewOp::Destroy(id) => self.execute_destroy(ready, id),
            WebviewOp::Recreate(id) => self.execute_recreate(ready, id),
            WebviewOp::UpdateInPlace(_id) => {
                // `WebviewHost` (Task 1.6) exposes no "update the live
                // display" method — `name`/`icon`/`notifications` are
                // sidebar-only metadata. The `Config` update already
                // applied by the caller, plus the single
                // `services-changed` emitted after every op, are the
                // whole story for this case.
            }
        }
    }

    fn execute_destroy(&self, ready: &mut Ready, id: &ServiceId) {
        // Not yet created (still waiting for its startup turn): nothing
        // to destroy. It is already gone from `ready.config` by the time
        // this runs (the edit that produced this op removed it), so
        // `create_one_locked`'s own re-check would skip it anyway even
        // without this early return.
        ready.pending.retain(|pending_id| pending_id != id);
        if !ready.created.contains(id) {
            return;
        }

        if let Err(err) = self.host.destroy(id) {
            tracing::error!("failed to destroy service '{id}': {err}");
            // The registry still thinks it's live; leave `created` as-is
            // so a retry (or a future op) is not fooled into thinking
            // there is nothing left to destroy.
            return;
        }
        ready.created.remove(id);
        ready.create_errors.remove(id);

        if ready.active.as_ref() == Some(id) {
            ready.active = None;
            self.activate_first_remaining(ready);
        }
    }

    fn execute_recreate(&self, ready: &mut Ready, id: &ServiceId) {
        ready.pending.retain(|pending_id| pending_id != id);
        // Still waiting for its startup turn: `create_one_locked` always
        // builds from the latest `Config`, so the eventual startup
        // creation already reflects the new `url`/`profile`; nothing to
        // recreate yet.
        if !ready.created.contains(id) {
            return;
        }

        let was_active = ready.active.as_ref() == Some(id);
        if let Err(err) = self.host.destroy(id) {
            tracing::error!("failed to destroy service '{id}' before recreating: {err}");
            ready.create_errors.insert(id.clone(), err.to_string());
            return;
        }
        ready.created.remove(id);
        if was_active {
            ready.active = None;
        }

        // Recreating never removes the old profile's on-disk data
        // (design.md §2.2.5's own instruction), even though `url`/
        // `profile` just changed — only `remove_service` ever does that,
        // and only for an `isolated` profile the caller confirmed.
        match self.create_one_locked(ready, id) {
            Ok(()) => {
                if was_active {
                    match self.host.activate(id) {
                        Ok(()) => ready.active = Some(id.clone()),
                        Err(err) => {
                            tracing::error!("failed to reactivate recreated service '{id}': {err}")
                        }
                    }
                }
            }
            Err(err) => {
                tracing::error!("failed to recreate service '{id}': {err}");
            }
        }
    }

    /// Activates the first service in sidebar order that already has a
    /// webview (design.md §2.2.5: removing the active service activates
    /// "the first remaining service" in sidebar order). Leaves `active`
    /// as `None` if no service currently has a webview.
    fn activate_first_remaining(&self, ready: &mut Ready) {
        let next = ready
            .config
            .services
            .iter()
            .map(|s| &s.id)
            .find(|id| ready.created.contains(*id))
            .cloned();
        let Some(next_id) = next else {
            return;
        };
        match self.host.activate(&next_id) {
            Ok(()) => ready.active = Some(next_id),
            Err(err) => {
                tracing::error!("failed to activate next service '{next_id}' after removal: {err}")
            }
        }
    }

    /// Applies `edit` to `config.toml`, then reconciles the live
    /// webviews to match (design.md §2.2.5). On success, returns the new
    /// `Config` (used by [`Self::remove_service`] to check profile
    /// sharing without a second lock round-trip).
    ///
    /// Locks, then calls `config::apply(path, edit)` first. If that
    /// fails (an invalid edit, a concurrent external change that makes
    /// the file invalid, an I/O error), no webview is touched and the
    /// error is returned; the in-memory `Config` this manager holds is
    /// left unchanged too, since nothing on disk changed either.
    ///
    /// On success, the new `Config` always replaces the old one — even
    /// if an individual webview operation below then fails, since the
    /// on-disk edit already happened and cannot be undone here.
    /// Per-service webview failures are logged and recorded in
    /// `create_errors` (design.md §5.1); they do not fail this call.
    /// `services-changed` is emitted at most once, after every op has
    /// run, exactly when `reconcile::diff` says the service list
    /// changed.
    async fn apply_edit_inner(&self, edit: ConfigEdit) -> AppResult<Config> {
        let mut guard = self.inner.lock().await;
        let ready = match &mut *guard {
            ManagerState::Failed(err) => return Err(not_ready(err)),
            ManagerState::Ready(ready) => ready,
        };

        let old_config = ready.config.clone();
        let new_config = config::apply(&self.config_path, edit)?;
        let plan = reconcile::diff(&old_config, &new_config);
        ready.config = new_config.clone();

        for op in &plan.ops {
            self.execute_op(ready, op);
        }

        drop(guard);

        if plan.services_changed {
            self.emit_services_changed();
        }

        Ok(new_config)
    }

    /// Applies `edit` and reconciles the live webviews (design.md
    /// §2.2.5); see [`Self::apply_edit_inner`] for the full contract.
    pub async fn apply_edit(&self, edit: ConfigEdit) -> AppResult<()> {
        self.apply_edit_inner(edit).await.map(|_config| ())
    }

    /// Adds a new service: derives its id from `name` with
    /// [`slug::slugify`] against the current config's ids, then applies
    /// [`ConfigEdit::AddService`] the same way any other edit is applied.
    /// Returns the new service's id on success.
    ///
    /// A collision between two concurrent `add_service` calls that
    /// happen to derive the same slug is caught by `config::apply`'s own
    /// duplicate-id validation (design.md §2.2.1), not by this method —
    /// the slug is computed from a snapshot of the existing ids taken
    /// while holding the lock, but `config::apply` re-validates against
    /// the file's latest contents.
    pub async fn add_service(
        &self,
        name: &str,
        url: Url,
        profile: ProfileName,
        notifications: bool,
        icon: IconSource,
    ) -> AppResult<ServiceId> {
        let existing = {
            let guard = self.inner.lock().await;
            match &*guard {
                ManagerState::Failed(err) => return Err(not_ready(err)),
                ManagerState::Ready(ready) => ready
                    .config
                    .services
                    .iter()
                    .map(|s| s.id.clone())
                    .collect::<Vec<_>>(),
            }
        };
        let id = slug::slugify(name, &existing);

        let service = ServiceConfig {
            id: id.clone(),
            name: name.to_string(),
            url,
            profile,
            notifications,
            icon,
        };
        self.apply_edit_inner(ConfigEdit::AddService(service))
            .await?;
        Ok(id)
    }

    /// Removes an existing service (design.md §2.2.5): applies
    /// [`ConfigEdit::RemoveService`] — which, as an ordinary part of the
    /// edit-and-reconcile path, destroys the live webview and activates
    /// the first remaining service in sidebar order if the removed one
    /// was active — then, only if every one of these holds, removes the
    /// profile's on-disk data:
    ///
    /// - `delete_session_data` is `true` (the caller's confirmation —
    ///   the settings UI's off-by-default checkbox),
    /// - the removed service's `profile` was literally `"isolated"`, so
    ///   its `ProfileKey` (design.md §2.2.3) was private to this one
    ///   service — never `default` or a shared named profile, and so
    ///   never shared with a remaining service by construction.
    ///
    /// A profile-removal failure is logged at warn level and does not
    /// fail this call (design.md §5.1: "Profile removal failure ... the
    /// data store is left in place"); Task 1.9's settings UI is where
    /// that failure is meant to surface to the user.
    pub async fn remove_service(&self, id: &ServiceId, delete_session_data: bool) -> AppResult<()> {
        let removed_profile = {
            let guard = self.inner.lock().await;
            match &*guard {
                ManagerState::Failed(err) => return Err(not_ready(err)),
                ManagerState::Ready(ready) => ready
                    .config
                    .services
                    .iter()
                    .find(|s| s.id == *id)
                    .map(|s| s.profile.clone()),
            }
        };

        self.apply_edit_inner(ConfigEdit::RemoveService(id.clone()))
            .await?;

        if !delete_session_data {
            return Ok(());
        }
        let Some(profile_name) = removed_profile else {
            return Ok(());
        };
        if profile_name.as_str() != ISOLATED_PROFILE_NAME {
            return Ok(());
        }

        let key = ProfileKey::Isolated(id.clone());
        let uuid = {
            let guard = self.inner.lock().await;
            let ManagerState::Ready(ready) = &*guard else {
                return Ok(());
            };
            match ready.state.read(|state| state.profiles.get(&key).copied()) {
                Ok(uuid) => uuid,
                Err(err) => {
                    tracing::warn!("failed to read profile uuid for service '{id}': {err}");
                    None
                }
            }
        };
        let Some(uuid) = uuid else {
            // Never resolved (the service was removed before it was ever
            // created), so there is nothing on disk to remove.
            return Ok(());
        };

        if let Err(err) = self.profile_backend.remove(&self.app_handle, uuid).await {
            tracing::warn!("failed to remove profile data for service '{id}': {err}");
        }
        Ok(())
    }

    fn emit_services_changed(&self) {
        if let Err(err) = self
            .app_handle
            .emit_to(SHELL_LABEL, SERVICES_CHANGED_EVENT, ())
        {
            tracing::warn!("failed to emit {SERVICES_CHANGED_EVENT}: {err}");
        }
    }
}

/// The error returned when an edit is attempted while this manager is
/// [`ManagerState::Failed`]: config/state never loaded, so there is
/// nothing valid to build an edit on top of (design.md §5.1 — this app
/// never repairs an existing file). Builds a fresh `AppError` from the
/// kept one's message rather than moving or cloning it out of
/// `ManagerState` (`AppError` is not `Clone`).
fn not_ready(startup_error: &AppError) -> AppError {
    AppError::Config(format!(
        "service manager did not start (config or state failed to load): {startup_error}"
    ))
}
