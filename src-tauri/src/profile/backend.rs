//! Applies a resolved profile UUID to a webview under construction, and
//! removes its on-disk / data-store state afterward — one platform per
//! build, chosen with `cfg` (design.md §2.2.3, §2.2.4; SPEC.md §5.2).
//!
//! macOS and Windows use different, mutually exclusive mechanisms
//! (`data_store_identifier` vs. `data_directory`; see SPEC.md §5.2's
//! platform table), so [`PlatformProfileBackend`] is defined once per
//! platform behind `#[cfg(target_os = "macos")]` / `#[cfg(windows)]`: only
//! one definition exists in any given build, both implement the shared
//! [`ProfileBackend`] trait, and callers elsewhere in the crate (the future
//! `host`/`services` modules) write platform-agnostic code against the
//! trait.

use std::path::{Path, PathBuf};

use tauri::webview::WebviewBuilder;
use tauri::{AppHandle, Wry};
use uuid::Uuid;

use crate::error::AppResult;

/// Applies a resolved profile UUID to a webview being built, and removes
/// its on-disk / data-store state once the profile is no longer needed.
///
/// `#[allow(async_fn_in_trait)]`: this trait is only ever used within this
/// binary crate (via [`PlatformProfileBackend`]), never as a `dyn` object
/// across an API boundary, so the lint's "callers can't require `Send`"
/// concern does not apply here — both platform impls' `remove` futures are
/// `Send` in practice (they only await Tauri's own `Send` futures or do
/// synchronous, non-blocking-relevant filesystem calls).
#[allow(async_fn_in_trait)]
pub trait ProfileBackend {
    /// Attaches `uuid`'s data-store identity to a webview under
    /// construction. Called before the webview is created, alongside the
    /// other per-webview builder settings (design.md §2.2.4).
    fn apply(&self, builder: WebviewBuilder<Wry>, uuid: Uuid) -> WebviewBuilder<Wry>;

    /// Removes `uuid`'s on-disk / data-store state.
    ///
    /// # Preconditions (the caller's responsibility, not enforced here)
    ///
    /// - Every webview using this profile must already be closed. Removing
    ///   a data store or directory that a live webview still has open is
    ///   undefined behaviour on both platforms.
    /// - The caller has confirmed the profile is `isolated` (never remove a
    ///   `default` or `named` profile just because one service stops using
    ///   it — other services may still share it), that the user consented
    ///   (the settings UI's "delete session data" confirmation, off by
    ///   default), and that no other configured service still names this
    ///   profile.
    ///
    /// A `uuid` with nothing on disk is not an error: both platform impls
    /// treat "already removed / never created" as success.
    async fn remove(&self, app: &AppHandle<Wry>, uuid: Uuid) -> AppResult<()>;
}

/// The concrete [`ProfileBackend`] this build uses — exactly one of the two
/// definitions below compiles, chosen by `cfg(target_os = "macos")` /
/// `cfg(windows)`.
#[cfg(target_os = "macos")]
pub struct PlatformProfileBackend;

#[cfg(target_os = "macos")]
impl PlatformProfileBackend {
    /// Builds a backend for `app`. Infallible on macOS (nothing to
    /// resolve up front), but returns `AppResult` for parity with the
    /// Windows constructor, which does resolve a directory here.
    pub fn new(_app: &AppHandle<Wry>) -> AppResult<Self> {
        Ok(PlatformProfileBackend)
    }
}

#[cfg(target_os = "macos")]
impl ProfileBackend for PlatformProfileBackend {
    fn apply(&self, builder: WebviewBuilder<Wry>, uuid: Uuid) -> WebviewBuilder<Wry> {
        // Available on macOS >= 14 / iOS >= 17 only (tauri's own doc
        // comment on `data_store_identifier`); SPEC.md §5.2 asks the M1
        // spike to confirm two isolated stores run side by side without
        // crashing on the target OS.
        builder.data_store_identifier(uuid.into_bytes())
    }

    async fn remove(&self, app: &AppHandle<Wry>, uuid: Uuid) -> AppResult<()> {
        // `AppHandle::remove_data_store` schedules itself onto the main
        // thread internally (see its doc comment in the tauri source), so
        // this is safe to call from any thread/async task.
        app.remove_data_store(uuid.into_bytes())
            .await
            .map_err(|err| {
                crate::error::AppError::Profile(format!("remove_data_store({uuid}) failed: {err}"))
            })
    }
}

#[cfg(windows)]
pub struct PlatformProfileBackend {
    /// `{data_dir}/webview`'s parent: the app's data directory
    /// (`paths::data_dir`). Held so `apply`/`remove` don't need to
    /// re-resolve it (and don't need an `AppHandle` at all, past
    /// construction).
    root: PathBuf,
}

#[cfg(windows)]
impl PlatformProfileBackend {
    /// Builds a backend for `app`, resolving its data directory up front
    /// via `paths::data_dir` (never hard-coded — see `paths.rs`).
    pub fn new(app: &AppHandle<Wry>) -> AppResult<Self> {
        Ok(PlatformProfileBackend {
            root: crate::paths::data_dir(app)?,
        })
    }
}

#[cfg(windows)]
impl ProfileBackend for PlatformProfileBackend {
    fn apply(&self, builder: WebviewBuilder<Wry>, uuid: Uuid) -> WebviewBuilder<Wry> {
        // Every webview shares one browser-argument constant (design.md
        // §2.2.4, SPEC.md §5.2's Windows constraint): WebView2 requires
        // that webviews with different `additional_browser_args` also use
        // different data directories, so differing args here would
        // silently break profile sharing. That constant is applied by the
        // `host` module, not here.
        builder.data_directory(webview_data_dir(&self.root, uuid))
    }

    async fn remove(&self, _app: &AppHandle<Wry>, uuid: Uuid) -> AppResult<()> {
        remove_webview_data_dir(&self.root, uuid)
    }
}

/// Computes the on-disk directory for `uuid`'s webview data on Windows:
/// `<root>/webview/<uuid>`, using the UUID's canonical lower-case hyphenated
/// form (design.md §2.2.3, §2.2.4).
///
/// A free function, not a `PlatformProfileBackend` method, and not
/// `cfg(windows)`-gated: it is pure path arithmetic, so it — and
/// [`remove_webview_data_dir`] — can be exercised with `tempfile` on any
/// host, not only Windows.
pub fn webview_data_dir(root: &Path, uuid: Uuid) -> PathBuf {
    root.join("webview").join(uuid.to_string())
}

/// Removes the on-disk directory computed by [`webview_data_dir`] for
/// `uuid`.
///
/// A target that does not exist is treated as already removed (`Ok`, not an
/// error): the profile may never have been used, or the caller may be
/// retrying a previous removal. Any other I/O failure is `AppError::Io`.
pub fn remove_webview_data_dir(root: &Path, uuid: Uuid) -> AppResult<()> {
    match std::fs::remove_dir_all(webview_data_dir(root, uuid)) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(crate::error::AppError::Io(err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn uuid_a() -> Uuid {
        Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap()
    }

    fn uuid_b() -> Uuid {
        Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap()
    }

    #[test]
    fn webview_data_dir_uses_lowercase_hyphenated_uuid_under_webview() {
        let root = Path::new("/data");
        let dir = webview_data_dir(root, uuid_a());
        assert_eq!(
            dir,
            Path::new("/data/webview/11111111-1111-1111-1111-111111111111")
        );
    }

    #[test]
    fn remove_webview_data_dir_deletes_a_directory_with_contents() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = webview_data_dir(tmp.path(), uuid_a());
        fs::create_dir_all(&dir).expect("create dir");
        fs::write(dir.join("Cookies"), b"data").expect("write file");
        assert!(dir.exists());

        remove_webview_data_dir(tmp.path(), uuid_a()).expect("remove");

        assert!(!dir.exists());
    }

    #[test]
    fn remove_webview_data_dir_missing_target_is_ok() {
        let tmp = tempfile::tempdir().expect("tempdir");
        remove_webview_data_dir(tmp.path(), uuid_a()).expect("missing target is not an error");
    }

    #[test]
    fn remove_webview_data_dir_leaves_other_uuids_alone() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir_a = webview_data_dir(tmp.path(), uuid_a());
        let dir_b = webview_data_dir(tmp.path(), uuid_b());
        fs::create_dir_all(&dir_a).expect("create dir a");
        fs::create_dir_all(&dir_b).expect("create dir b");

        remove_webview_data_dir(tmp.path(), uuid_a()).expect("remove a");

        assert!(!dir_a.exists());
        assert!(dir_b.exists());
    }
}
