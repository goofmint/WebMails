//! Resolves a service's sidebar icon (design.md §2.2.10, §11.1) and caches
//! it on disk as a 128×128 PNG.
//!
//! [`resolution_order`] is the pure "what to try, in what order" step:
//! given an optional user override and the agent's already-ordered
//! candidates, it returns the list of sources to attempt, first success
//! wins. [`fetch`] downloads a candidate safely; [`normalize`] decodes and
//! normalises the result. [`IconService`] ties the three together, keeps
//! the small amount of state resolution needs (the latest candidates seen
//! per service, and which services are currently resolving), and is
//! generic over an injected [`IconFetcher`] so none of this needs a real
//! network to unit-test.
//!
//! Nothing here depends on Tauri: [`crate::services::ServiceManager`] is
//! the only caller, and it owns spawning a resolution in the background
//! and emitting `service-icon-changed` once one finishes (design.md
//! §2.2.10's "After it succeeds, the shell receives" that event) — see its
//! module doc comment.

pub mod fetch;
mod normalize;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use tauri::async_runtime::Mutex;
use url::Url;

use crate::config::{IconSource, ServiceId};
use crate::error::AppError;
use crate::paths;

/// The `source` argument of the `set_icon_override` command (design.md
/// §2.2.12: input `{ id, source: favicon | file(path) | url }`). Unlike
/// [`IconSource`] (whose `File` variant stores a path already relative to
/// `data_dir`), this `File` variant carries an arbitrary, user-chosen
/// absolute path — wherever they picked the image from —
/// `services::ServiceManager::set_icon_override` copies it into
/// `{data_dir}/icons/<id>.src` via [`IconService::install_override_file`]
/// before ever building the [`IconSource`] to patch the service's config
/// with.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "source", content = "value", rename_all = "lowercase")]
pub enum IconOverride {
    Favicon,
    File(PathBuf),
    Url(Url),
}

pub use fetch::{FetchError, IconFetcher, ReqwestIconFetcher};
pub use normalize::{NormalizeError, ICON_SIZE};

/// One source [`resolution_order`] says to try, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionStep {
    /// The user's file override: a path relative to `data_dir` (always
    /// `icons/<id>.src` in practice — [`crate::paths::icon_override_relative_path`]),
    /// joined onto `data_dir` when actually read (see [`read_step`]).
    OverrideFile(PathBuf),
    /// A URL to fetch: either the user's URL override, or one of the
    /// agent's `iconCandidates`.
    Fetch(Url),
}

/// Why a `set_icon_override` URL override was rejected — the same rules
/// design.md §2.2.6/§2.2.10 apply to an agent's `iconCandidates`, run here
/// for a user-supplied URL override instead (`services::ServiceManager::
/// set_icon_override`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IconOverrideError {
    #[error("icon override URL must be http or https")]
    InvalidScheme,
    #[error("icon override URL host is loopback, private or link-local")]
    DisallowedHost,
}

/// Validates a `set_icon_override` URL override exactly like an agent's
/// `iconCandidates` (design.md §2.2.6, §2.2.10): `http`/`https` scheme,
/// and a literal host that is not loopback, private or link-local. No DNS
/// resolution is performed here either — the download step later on
/// ([`fetch::ReqwestIconFetcher`]) re-checks the *resolved* address too.
pub fn validate_override_url(url: &Url) -> Result<(), IconOverrideError> {
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err(IconOverrideError::InvalidScheme),
    }
    let host = url.host().ok_or(IconOverrideError::InvalidScheme)?;
    if crate::net_guard::is_disallowed_host(&host) {
        return Err(IconOverrideError::DisallowedHost);
    }
    Ok(())
}

impl From<IconOverrideError> for AppError {
    fn from(err: IconOverrideError) -> Self {
        AppError::Icon(err.to_string())
    }
}

/// Why [`IconService::install_override_file`] failed: either the source
/// file could not be read at all, or it could — but its bytes did not
/// decode as a supported image ([`normalize::normalize_to_png`]'s own
/// [`NormalizeError::Decode`]/[`NormalizeError::Encode`]).
#[derive(Debug, thiserror::Error)]
pub enum InstallOverrideFileError {
    #[error("could not read icon override file: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not normalize icon override file: {0}")]
    Normalize(#[from] NormalizeError),
}

/// Builds the ordered list of sources to try for one service (design.md
/// §11.1's "first success wins"): the user override first (if any and not
/// `Favicon`, which defers entirely to `candidates`), then every candidate
/// in the order the agent already sorted them.
///
/// Pure and independent of any I/O — this is the "candidate ordering" this
/// task's done-when criterion tests directly.
pub fn resolution_order(
    override_source: Option<&IconSource>,
    candidates: &[Url],
) -> Vec<ResolutionStep> {
    let mut steps = Vec::with_capacity(candidates.len() + 1);
    match override_source {
        Some(IconSource::File(relative_path)) => {
            steps.push(ResolutionStep::OverrideFile(relative_path.clone()));
        }
        Some(IconSource::Url(url)) => steps.push(ResolutionStep::Fetch(url.clone())),
        Some(IconSource::Favicon) | None => {}
    }
    steps.extend(candidates.iter().cloned().map(ResolutionStep::Fetch));
    steps
}

/// Reads bytes for one [`ResolutionStep`]: [`ResolutionStep::OverrideFile`]
/// is read straight off disk (relative to `data_dir`), [`ResolutionStep::Fetch`]
/// goes through `fetcher`. A step's own failure (missing file, failed
/// download) is not fatal to resolution as a whole — [`resolve_bytes`]
/// just moves on to the next step.
async fn read_step(
    data_dir: &Path,
    fetcher: &dyn IconFetcher,
    step: &ResolutionStep,
) -> Option<Vec<u8>> {
    match step {
        ResolutionStep::OverrideFile(relative_path) => {
            std::fs::read(data_dir.join(relative_path)).ok()
        }
        ResolutionStep::Fetch(url) => fetcher.fetch(url).await.ok(),
    }
}

/// Tries every step in `order`, returning the first one whose bytes decode
/// and normalise to a PNG successfully. `None` if every step fails (or
/// `order` is empty) — the caller then leaves no cached PNG in place, and
/// the shell falls back to the generated letter icon (design.md §11.1's
/// step 5).
async fn resolve_bytes(
    data_dir: &Path,
    fetcher: &dyn IconFetcher,
    order: &[ResolutionStep],
) -> Option<Vec<u8>> {
    for step in order {
        let Some(bytes) = read_step(data_dir, fetcher, step).await else {
            continue;
        };
        if let Ok(png) = normalize::normalize_to_png(&bytes) {
            return Some(png);
        }
    }
    None
}

/// A cached icon's location and cache-busting version, for `get_snapshot`'s
/// `icons` map (design.md §2.2.10, §2.2.12). `Serialize` (nested under the
/// snapshot's own `icons` map, so plain snake_case field names — see
/// `commands::snapshot`) lets it reach the shell as JSON directly.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CachedIcon {
    /// Absolute filesystem path to the cached PNG.
    pub path: PathBuf,
    /// The file's modification time (Unix seconds) — bumped every time the
    /// PNG is rewritten, so the shell can bust its `convertFileSrc` cache
    /// with a `?v=` query parameter.
    pub version: u64,
}

/// Resolves and caches service icons (module doc comment has the full
/// picture). Every method locks [`IconService::inner`] for its whole body,
/// same pattern as [`crate::services::ServiceManager`].
pub struct IconService {
    data_dir: PathBuf,
    fetcher: Arc<dyn IconFetcher>,
    inner: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// The most recent `iconCandidates` seen per service (design.md
    /// §2.2.6), kept so [`IconService::refresh`] can re-resolve without
    /// waiting for the agent to report again.
    candidates: BTreeMap<ServiceId, Vec<Url>>,
    /// Services a resolution is currently in flight for, mapped to that
    /// resolution's generation number (design.md §2.2.10). A burst of
    /// `report_unread` calls (or a refresh arriving mid-resolution) never
    /// starts a second, redundant resolution for the same id while an
    /// entry is present here — `record_candidates`'s pre-spawn dedup
    /// check ([`IconService::begin_resolving`]). The value lets
    /// [`IconService::end_resolving`] tell whether it still owns `id`'s
    /// entry: every `resolve` call — regardless of which entry point
    /// (the auto-resolve path off `record_candidates`, `refresh`, or a
    /// direct call from the `refresh_icon`/`set_icon_override` commands)
    /// triggered it — allocates a fresh generation for `id`
    /// ([`IconService::begin_generation`]) and overwrites this entry with
    /// it, so a slower, now-superseded resolution's `end_resolving` never
    /// clears a newer, still-running resolution's marker out from under
    /// it.
    resolving: BTreeMap<ServiceId, u64>,
    /// The last generation number ever handed out per service (design.md
    /// §2.2.10), kept even once `resolving`'s entry for that id is
    /// cleared, so the next resolution's generation for the same id is
    /// always strictly greater than every one before it — a generation
    /// is never reused, which is what lets [`IconService::write_cache`]
    /// tell a stale write apart from a current one after `resolving`'s
    /// entry has already moved on or been cleared.
    next_generation: BTreeMap<ServiceId, u64>,
}

impl IconService {
    /// Builds a service backed by the real network fetcher
    /// ([`ReqwestIconFetcher`]). Fails only if that fetcher's underlying
    /// `reqwest::Client` cannot be built at all (see its own doc comment).
    pub fn new(data_dir: PathBuf) -> Result<Self, AppError> {
        Ok(Self::with_fetcher(
            data_dir,
            Arc::new(ReqwestIconFetcher::new()?),
        ))
    }

    /// Builds a service backed by an injected `fetcher` — the constructor
    /// every unit test in this module (and `services::tests`) uses instead
    /// of touching the network.
    pub fn with_fetcher(data_dir: PathBuf, fetcher: Arc<dyn IconFetcher>) -> Self {
        IconService {
            data_dir,
            fetcher,
            inner: Mutex::new(State::default()),
        }
    }

    /// Whether `id` already has a cached PNG on disk, and if so, its path
    /// and version (design.md §2.2.10, §2.2.12's `get_snapshot`).
    pub fn cached_icon(&self, id: &ServiceId) -> Option<CachedIcon> {
        let path = paths::icon_cache_file(&self.data_dir, id);
        let metadata = std::fs::metadata(&path).ok()?;
        let version = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        Some(CachedIcon { path, version })
    }

    /// Records `candidates` as the latest seen for `id` (design.md
    /// §2.2.6's `report_unread`, called only when validated and
    /// non-empty — see `services::ServiceManager::record_icon_candidates`).
    /// Returns `true` if resolution should now run: no cached PNG exists
    /// yet and nothing is already resolving `id`. The caller
    /// (`ServiceManager`) is responsible for actually spawning
    /// [`IconService::resolve`] when this returns `true`, since only it
    /// can emit `service-icon-changed` afterwards.
    pub async fn record_candidates(&self, id: ServiceId, candidates: Vec<Url>) -> bool {
        let mut guard = self.inner.lock().await;
        guard.candidates.insert(id.clone(), candidates);
        if self.cached_icon(&id).is_some() {
            return false;
        }
        self.begin_resolving(&mut guard, &id)
    }

    /// The candidates last recorded for `id` (empty if none yet) — used to
    /// re-resolve on `refresh_icon` without needing a fresh agent report.
    pub async fn known_candidates(&self, id: &ServiceId) -> Vec<Url> {
        let guard = self.inner.lock().await;
        guard.candidates.get(id).cloned().unwrap_or_default()
    }

    /// Marks `id` as resolving if it is not already, returning whether the
    /// caller should proceed. Must be called with `guard` already held.
    /// Purely `record_candidates`'s pre-spawn dedup check: the placeholder
    /// generation (`0`) it inserts is always overwritten by the real
    /// resolution's own [`Self::begin_generation`] once `resolve` actually
    /// runs, so it never risks colliding with a real generation number
    /// (which starts at `1`).
    fn begin_resolving(&self, guard: &mut State, id: &ServiceId) -> bool {
        if guard.resolving.contains_key(id) {
            return false;
        }
        guard.resolving.insert(id.clone(), 0);
        true
    }

    /// Allocates a new generation for `id` — always strictly greater than
    /// every generation handed out for it before, even a since-cleared
    /// one (design.md §2.2.10) — and records it as the one currently
    /// resolving. Called once, at the very start of every [`Self::resolve`]
    /// call, regardless of which entry point triggered it. Returns the
    /// generation this call now owns.
    async fn begin_generation(&self, id: &ServiceId) -> u64 {
        let mut guard = self.inner.lock().await;
        let generation = guard.next_generation.get(id).copied().unwrap_or(0) + 1;
        guard.next_generation.insert(id.clone(), generation);
        guard.resolving.insert(id.clone(), generation);
        generation
    }

    /// Clears `id`'s resolving marker, but only if `generation` is still
    /// the one recorded there — i.e. only if no newer resolution has
    /// since started for `id` (design.md §2.2.10). A stale, superseded
    /// resolution finishing after a newer one has already begun must
    /// never clear the newer one's still-in-flight marker out from under
    /// it.
    async fn end_resolving(&self, id: &ServiceId, generation: u64) {
        let mut guard = self.inner.lock().await;
        if guard.resolving.get(id) == Some(&generation) {
            guard.resolving.remove(id);
        }
    }

    /// Deletes `id`'s cached PNG, if any, so the next resolution is never
    /// skipped for "already cached" (`refresh_icon`, design.md §2.2.12).
    /// Not an error if there was no cache to remove.
    pub fn clear_cache(&self, id: &ServiceId) {
        let path = paths::icon_cache_file(&self.data_dir, id);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                tracing::warn!("failed to remove cached icon for '{id}': {err}");
            }
        }
    }

    /// Removes every on-disk trace of `id`'s icon — the cached PNG, an
    /// installed file override, and its remembered candidates (design.md
    /// §2.2.10; `ServiceManager::remove_service`'s cleanup).
    pub async fn forget(&self, id: &ServiceId) {
        self.clear_cache(id);
        let override_path = paths::icon_override_file(&self.data_dir, id);
        match std::fs::remove_file(&override_path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                tracing::warn!("failed to remove icon override file for '{id}': {err}");
            }
        }
        let mut guard = self.inner.lock().await;
        guard.candidates.remove(id);
        guard.resolving.remove(id);
    }

    /// Reads `source_path` (an arbitrary, user-chosen absolute path),
    /// decodes and normalises it with [`normalize::normalize_to_png`], and
    /// writes the resulting `ICON_SIZE`×`ICON_SIZE` PNG — never the
    /// original file's bytes — to `{data_dir}/icons/<id>.src` (design.md
    /// §2.2.10, §2.2.12's file override), creating the `icons` directory
    /// first if needed. Returns the relative path (`icons/<id>.src`) to
    /// store as the service's `config.toml` `icon.value` (`config::
    /// validate`'s "must stay inside `data_dir`" rule, already enforced
    /// there, is satisfied by construction: this is always the same fixed
    /// relative path). A file that cannot be read or decoded is rejected
    /// ([`InstallOverrideFileError`]) rather than installed as-is: only a
    /// normalised PNG is ever stored for a file override, exactly like
    /// every other icon source.
    pub fn install_override_file(
        &self,
        id: &ServiceId,
        source_path: &Path,
    ) -> Result<PathBuf, InstallOverrideFileError> {
        let bytes = std::fs::read(source_path)?;
        let png = normalize::normalize_to_png(&bytes)?;
        let relative = paths::icon_override_relative_path(id);
        let destination = self.data_dir.join(&relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&destination, png)?;
        Ok(relative)
    }

    /// Runs resolution for `id` now: allocates a fresh generation for it
    /// (design.md §2.2.10; [`Self::begin_generation`]) — regardless of
    /// which entry point (`record_candidates`'s auto-resolve, `refresh`,
    /// or a direct call from the `refresh_icon`/`set_icon_override`
    /// commands) called this — then builds the order from
    /// `override_source` and `id`'s last-known candidates, tries each
    /// step, and — on the first success — writes the normalised PNG to
    /// `id`'s cache path (via a temp file renamed into place, so a reader
    /// never observes a partially written file), but only if this
    /// generation is still current: a slower resolution finishing after a
    /// newer one has since started for the same `id` has its write
    /// discarded ([`Self::write_cache`]), so it can never clobber the
    /// newer one's result. Returns whether the cache actually changed (a
    /// new PNG was written), which the caller uses to decide whether to
    /// emit `service-icon-changed`.
    ///
    /// Always clears the "resolving" flag for `id` before returning, even
    /// on failure — but only if this is still the generation that owns
    /// it — so a later `record_candidates`/`refresh` can try again.
    pub async fn resolve(&self, id: ServiceId, override_source: IconSource) -> bool {
        let generation = self.begin_generation(&id).await;
        let candidates = self.known_candidates(&id).await;
        let order = resolution_order(Some(&override_source), &candidates);
        let result = resolve_bytes(&self.data_dir, self.fetcher.as_ref(), &order).await;
        self.end_resolving(&id, generation).await;

        let Some(png) = result else {
            return false;
        };
        match self.write_cache(&id, generation, &png).await {
            Ok(true) => true,
            Ok(false) => false,
            Err(err) => {
                tracing::warn!("failed to cache resolved icon for '{id}': {err}");
                false
            }
        }
    }

    /// `refresh_icon` (design.md §2.2.12): deletes the current cache so
    /// [`Self::resolve`] cannot skip re-fetching, then resolves using
    /// `id`'s last-known candidates. The caller decides whether resolution
    /// itself should run inline or be spawned in the background.
    pub async fn refresh(&self, id: ServiceId, override_source: IconSource) -> bool {
        self.clear_cache(&id);
        {
            let mut guard = self.inner.lock().await;
            self.begin_resolving(&mut guard, &id);
        }
        self.resolve(id, override_source).await
    }

    /// Writes `png` to `id`'s cache path via a temp file renamed into
    /// place, creating `{data_dir}/icons` first if needed — but only if
    /// `generation` is still the latest one handed out for `id` (design.md
    /// §2.2.10): if a newer resolution has since begun, this one has been
    /// superseded and its result is discarded without touching the
    /// filesystem. Returns whether it actually wrote.
    async fn write_cache(
        &self,
        id: &ServiceId,
        generation: u64,
        png: &[u8],
    ) -> std::io::Result<bool> {
        {
            let guard = self.inner.lock().await;
            if guard.next_generation.get(id) != Some(&generation) {
                return Ok(false);
            }
        }
        let dir = paths::icons_dir(&self.data_dir);
        std::fs::create_dir_all(&dir)?;
        let final_path = paths::icon_cache_file(&self.data_dir, id);
        let tmp_path = dir.join(format!("{id}.png.tmp"));
        std::fs::write(&tmp_path, png)?;
        std::fs::rename(&tmp_path, &final_path)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Mutex as StdMutex;

    fn id(value: &str) -> ServiceId {
        ServiceId::new(value).expect("valid id")
    }

    fn url(value: &str) -> Url {
        Url::parse(value).expect("valid url")
    }

    // --- IconOverride deserialization ---------------------------------------

    #[test]
    fn icon_override_deserializes_favicon() {
        let value: IconOverride = serde_json::from_str(r#"{"source":"favicon"}"#).expect("de");
        assert_eq!(value, IconOverride::Favicon);
    }

    #[test]
    fn icon_override_deserializes_file_with_an_arbitrary_absolute_path() {
        let value: IconOverride =
            serde_json::from_str(r#"{"source":"file","value":"/Users/me/Downloads/icon.png"}"#)
                .expect("de");
        assert_eq!(
            value,
            IconOverride::File(PathBuf::from("/Users/me/Downloads/icon.png"))
        );
    }

    #[test]
    fn icon_override_deserializes_url() {
        let value: IconOverride =
            serde_json::from_str(r#"{"source":"url","value":"https://example.com/icon.png"}"#)
                .expect("de");
        assert_eq!(
            value,
            IconOverride::Url(Url::parse("https://example.com/icon.png").expect("url"))
        );
    }

    #[test]
    fn icon_override_rejects_unknown_source() {
        let result: Result<IconOverride, _> = serde_json::from_str(r#"{"source":"gravatar"}"#);
        assert!(result.is_err());
    }

    // --- validate_override_url ----------------------------------------------

    #[test]
    fn accepts_an_https_public_host_override_url() {
        assert!(validate_override_url(&url("https://example.com/icon.png")).is_ok());
    }

    #[test]
    fn rejects_a_non_http_scheme_override_url() {
        let err = validate_override_url(&url("ftp://example.com/icon.png")).unwrap_err();
        assert_eq!(err, IconOverrideError::InvalidScheme);
    }

    #[test]
    fn rejects_a_loopback_host_override_url() {
        let err = validate_override_url(&url("http://127.0.0.1/icon.png")).unwrap_err();
        assert_eq!(err, IconOverrideError::DisallowedHost);
    }

    // --- resolution_order --------------------------------------------------

    #[test]
    fn no_override_orders_candidates_only() {
        let candidates = vec![
            url("https://example.com/apple.png"),
            url("https://example.com/icon.png"),
        ];
        let order = resolution_order(None, &candidates);
        assert_eq!(
            order,
            vec![
                ResolutionStep::Fetch(candidates[0].clone()),
                ResolutionStep::Fetch(candidates[1].clone()),
            ]
        );
    }

    #[test]
    fn favicon_override_defers_entirely_to_candidates() {
        let candidates = vec![url("https://example.com/icon.png")];
        let order = resolution_order(Some(&IconSource::Favicon), &candidates);
        assert_eq!(order, vec![ResolutionStep::Fetch(candidates[0].clone())]);
    }

    #[test]
    fn file_override_comes_before_candidates() {
        let candidates = vec![url("https://example.com/icon.png")];
        let over = IconSource::File(PathBuf::from("icons/svc.src"));
        let order = resolution_order(Some(&over), &candidates);
        assert_eq!(
            order,
            vec![
                ResolutionStep::OverrideFile(PathBuf::from("icons/svc.src")),
                ResolutionStep::Fetch(candidates[0].clone()),
            ]
        );
    }

    #[test]
    fn url_override_comes_before_candidates() {
        let over_url = url("https://user.example.com/mine.png");
        let candidates = vec![url("https://example.com/icon.png")];
        let over = IconSource::Url(over_url.clone());
        let order = resolution_order(Some(&over), &candidates);
        assert_eq!(
            order,
            vec![
                ResolutionStep::Fetch(over_url),
                ResolutionStep::Fetch(candidates[0].clone()),
            ]
        );
    }

    #[test]
    fn no_override_and_no_candidates_orders_nothing() {
        assert_eq!(resolution_order(None, &[]), Vec::new());
    }

    // --- resolve / caching, with an injected fetcher ------------------------

    /// A 1x1 solid PNG, small enough to embed as a byte literal, used as
    /// "successful download" fixture bytes throughout this module's tests.
    fn one_pixel_png() -> Vec<u8> {
        use image::{Rgba, RgbaImage};
        let img = RgbaImage::from_pixel(4, 4, Rgba([1, 2, 3, 255]));
        let mut out = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .expect("encode fixture");
        out
    }

    type FetchMap = StdMutex<BTreeMap<String, Result<Vec<u8>, FetchError>>>;

    /// A fake [`IconFetcher`] whose responses are fixed in advance per URL
    /// — the "指定された取得関数" (a given fetch function) the
    /// implementation plan asks the ordering tests to use, so
    /// [`IconService::resolve`] is exercised end-to-end with no network.
    struct FakeFetcher {
        responses: FetchMap,
        calls: StdMutex<Vec<String>>,
    }

    impl FakeFetcher {
        fn new(responses: Vec<(&str, Result<Vec<u8>, FetchError>)>) -> Self {
            FakeFetcher {
                responses: StdMutex::new(
                    responses
                        .into_iter()
                        .map(|(url, result)| (url.to_string(), result))
                        .collect(),
                ),
                calls: StdMutex::new(Vec::new()),
            }
        }

        fn call_count(&self) -> usize {
            self.calls.lock().expect("lock").len()
        }
    }

    impl IconFetcher for FakeFetcher {
        fn fetch<'a>(&'a self, url: &'a Url) -> fetch::FetchFuture<'a> {
            let key = url.to_string();
            self.calls.lock().expect("lock").push(key.clone());
            let result = self
                .responses
                .lock()
                .expect("lock")
                .get(&key)
                .cloned()
                .unwrap_or(Err(FetchError::Request("no fixture for url".to_string())));
            Box::pin(async move { result }) as Pin<Box<dyn Future<Output = _> + Send>>
        }
    }

    fn service(fetcher: FakeFetcher) -> (IconService, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let service = IconService::with_fetcher(dir.path().to_path_buf(), Arc::new(fetcher));
        (service, dir)
    }

    #[tokio::test]
    async fn resolve_caches_the_first_successful_candidate() {
        let good_url = "https://example.com/good.png";
        let fetcher = FakeFetcher::new(vec![
            ("https://example.com/bad.png", Err(FetchError::Timeout)),
            (good_url, Ok(one_pixel_png())),
        ]);
        let (service, _dir) = service(fetcher);
        let service_id = id("alpha");

        service
            .record_candidates(
                service_id.clone(),
                vec![url("https://example.com/bad.png"), url(good_url)],
            )
            .await;

        let changed = service
            .resolve(service_id.clone(), IconSource::Favicon)
            .await;
        assert!(changed);
        let cached = service.cached_icon(&service_id).expect("should be cached");
        assert!(cached.path.exists());
    }

    #[tokio::test]
    async fn resolve_returns_false_when_every_candidate_fails() {
        let fetcher = FakeFetcher::new(vec![(
            "https://example.com/bad.png",
            Err(FetchError::Timeout),
        )]);
        let (service, _dir) = service(fetcher);
        let service_id = id("beta");
        service
            .record_candidates(service_id.clone(), vec![url("https://example.com/bad.png")])
            .await;

        let changed = service
            .resolve(service_id.clone(), IconSource::Favicon)
            .await;
        assert!(!changed);
        assert!(service.cached_icon(&service_id).is_none());
    }

    #[tokio::test]
    async fn record_candidates_reports_resolution_needed_only_without_a_cache() {
        let fetcher = FakeFetcher::new(vec![("https://example.com/a.png", Ok(one_pixel_png()))]);
        let (service, _dir) = service(fetcher);
        let service_id = id("gamma");

        let should_resolve = service
            .record_candidates(service_id.clone(), vec![url("https://example.com/a.png")])
            .await;
        assert!(should_resolve, "no cache yet, so resolution should run");

        service
            .resolve(service_id.clone(), IconSource::Favicon)
            .await;

        let should_resolve_again = service
            .record_candidates(service_id.clone(), vec![url("https://example.com/a.png")])
            .await;
        assert!(
            !should_resolve_again,
            "a cached PNG already exists; a fresh report must not re-resolve"
        );
    }

    #[tokio::test]
    async fn record_candidates_does_not_trigger_twice_while_already_resolving() {
        let fetcher = FakeFetcher::new(vec![("https://example.com/a.png", Ok(one_pixel_png()))]);
        let (service, _dir) = service(fetcher);
        let service_id = id("iota");

        let first = service
            .record_candidates(service_id.clone(), vec![url("https://example.com/a.png")])
            .await;
        assert!(first, "no cache and nothing resolving yet");

        let second = service
            .record_candidates(service_id.clone(), vec![url("https://example.com/a.png")])
            .await;
        assert!(
            !second,
            "already resolving; must not trigger a second resolve"
        );
    }

    #[tokio::test]
    async fn refresh_clears_the_cache_and_forces_re_resolution() {
        let fetcher = FakeFetcher::new(vec![("https://example.com/a.png", Ok(one_pixel_png()))]);
        let (service, _dir) = service(fetcher);
        let service_id = id("delta");
        service
            .record_candidates(service_id.clone(), vec![url("https://example.com/a.png")])
            .await;
        service
            .resolve(service_id.clone(), IconSource::Favicon)
            .await;
        assert!(service.cached_icon(&service_id).is_some());

        let changed = service
            .refresh(service_id.clone(), IconSource::Favicon)
            .await;
        assert!(changed);
        assert!(service.cached_icon(&service_id).is_some());
    }

    #[tokio::test]
    async fn resolve_prefers_the_url_override_over_agent_candidates() {
        let override_url = "https://user.example.com/mine.png";
        let fetcher = FakeFetcher::new(vec![
            (override_url, Ok(one_pixel_png())),
            ("https://example.com/agent.png", Ok(one_pixel_png())),
        ]);
        let (service, _dir) = service(fetcher);
        let service_id = id("epsilon");
        service
            .record_candidates(
                service_id.clone(),
                vec![url("https://example.com/agent.png")],
            )
            .await;

        let over = IconSource::Url(url(override_url));
        service.resolve(service_id.clone(), over).await;
        assert!(service.cached_icon(&service_id).is_some());
    }

    #[tokio::test]
    async fn file_override_is_read_from_data_dir_relative_path() {
        let fetcher = FakeFetcher::new(vec![]);
        let (service, dir) = service(fetcher);
        let service_id = id("zeta");

        let relative = service
            .install_override_file(&service_id, &{
                let src = dir.path().join("source-icon.png");
                std::fs::write(&src, one_pixel_png()).expect("write fixture");
                src
            })
            .expect("install override");
        assert_eq!(relative, paths::icon_override_relative_path(&service_id));

        let changed = service
            .resolve(service_id.clone(), IconSource::File(relative))
            .await;
        assert!(changed);
        assert!(service.cached_icon(&service_id).is_some());
    }

    #[tokio::test]
    async fn install_override_file_normalizes_instead_of_copying_the_source_bytes() {
        let fetcher = FakeFetcher::new(vec![]);
        let (service, dir) = service(fetcher);
        let service_id = id("zeta-normalized");

        // A source image that is *not* already ICON_SIZE×ICON_SIZE -- if
        // `install_override_file` merely copied it, the stored bytes would
        // equal the source's; since it normalizes instead, the stored PNG
        // must decode to exactly ICON_SIZE×ICON_SIZE regardless of the
        // source's own dimensions.
        let src = dir.path().join("source-icon.png");
        std::fs::write(&src, one_pixel_png()).expect("write fixture");

        let relative = service
            .install_override_file(&service_id, &src)
            .expect("install override");
        let installed_bytes =
            std::fs::read(dir.path().join(&relative)).expect("read installed file");
        assert_ne!(
            installed_bytes,
            one_pixel_png(),
            "the installed file must be the normalized PNG, not a raw copy"
        );
        let decoded = image::load_from_memory(&installed_bytes).expect("decode installed file");
        assert_eq!(decoded.width(), ICON_SIZE);
        assert_eq!(decoded.height(), ICON_SIZE);
    }

    #[test]
    fn install_override_file_rejects_an_undecodable_source() {
        let fetcher = FakeFetcher::new(vec![]);
        let (service, dir) = service(fetcher);
        let service_id = id("zeta-undecodable");

        let src = dir.path().join("not-an-image.bin");
        std::fs::write(&src, b"not an image").expect("write fixture");

        let err = service
            .install_override_file(&service_id, &src)
            .expect_err("undecodable source must be rejected");
        assert!(matches!(err, InstallOverrideFileError::Normalize(_)));
        assert!(
            !dir.path()
                .join(paths::icon_override_relative_path(&service_id))
                .exists(),
            "no file must be installed on rejection"
        );
    }

    #[tokio::test]
    async fn forget_removes_cache_override_and_candidates() {
        let fetcher = FakeFetcher::new(vec![("https://example.com/a.png", Ok(one_pixel_png()))]);
        let (service, dir) = service(fetcher);
        let service_id = id("eta");
        service
            .record_candidates(service_id.clone(), vec![url("https://example.com/a.png")])
            .await;
        service
            .resolve(service_id.clone(), IconSource::Favicon)
            .await;
        let src = dir.path().join("source.png");
        std::fs::write(&src, one_pixel_png()).expect("write fixture");
        service
            .install_override_file(&service_id, &src)
            .expect("install override");

        service.forget(&service_id).await;

        assert!(service.cached_icon(&service_id).is_none());
        assert!(!dir
            .path()
            .join(paths::icon_override_relative_path(&service_id))
            .exists());
        assert!(service.known_candidates(&service_id).await.is_empty());
    }

    #[tokio::test]
    async fn resolve_stops_at_the_first_success_without_trying_later_candidates() {
        let fetcher = FakeFetcher::new(vec![
            ("https://example.com/first.png", Ok(one_pixel_png())),
            ("https://example.com/second.png", Ok(one_pixel_png())),
        ]);
        let fetcher = Arc::new(fetcher);
        let dir = tempfile::tempdir().expect("tempdir");
        let service = IconService::with_fetcher(dir.path().to_path_buf(), fetcher.clone());
        let service_id = id("theta");
        service
            .record_candidates(
                service_id.clone(),
                vec![
                    url("https://example.com/first.png"),
                    url("https://example.com/second.png"),
                ],
            )
            .await;
        service.resolve(service_id, IconSource::Favicon).await;
        assert_eq!(
            fetcher.call_count(),
            1,
            "must not fetch the second candidate"
        );
    }

    // --- per-service generation counter -------------------------------------

    #[tokio::test]
    async fn begin_generation_is_strictly_increasing_per_service() {
        let fetcher = FakeFetcher::new(vec![]);
        let (service, _dir) = service(fetcher);
        let service_id = id("kappa");

        let first = service.begin_generation(&service_id).await;
        let second = service.begin_generation(&service_id).await;
        assert!(second > first, "each call must allocate a newer generation");
    }

    #[tokio::test]
    async fn write_cache_commits_when_its_generation_is_still_current() {
        let fetcher = FakeFetcher::new(vec![]);
        let (service, _dir) = service(fetcher);
        let service_id = id("lambda");

        let generation = service.begin_generation(&service_id).await;
        let wrote = service
            .write_cache(&service_id, generation, &one_pixel_png())
            .await
            .expect("write_cache should not fail");
        assert!(wrote, "the only, current generation must commit");
        assert!(service.cached_icon(&service_id).is_some());
    }

    #[tokio::test]
    async fn write_cache_discards_a_stale_generation_superseded_by_a_newer_one() {
        let fetcher = FakeFetcher::new(vec![]);
        let (service, _dir) = service(fetcher);
        let service_id = id("mu");

        // A slow resolution starts first...
        let stale_generation = service.begin_generation(&service_id).await;
        // ...but a newer one starts (and, in this test, finishes) before it
        // does, superseding it.
        let current_generation = service.begin_generation(&service_id).await;
        assert!(service
            .write_cache(&service_id, current_generation, &one_pixel_png())
            .await
            .expect("write_cache should not fail"));

        // The stale resolution finally finishes: its write must be
        // discarded, and the newer generation's cached PNG must survive
        // untouched.
        let wrote = service
            .write_cache(&service_id, stale_generation, &one_pixel_png())
            .await
            .expect("write_cache should not fail");
        assert!(
            !wrote,
            "a superseded generation must not overwrite the cache"
        );
    }

    #[tokio::test]
    async fn end_resolving_only_clears_its_own_generation() {
        let fetcher = FakeFetcher::new(vec![]);
        let (service, _dir) = service(fetcher);
        let service_id = id("nu");

        let stale_generation = service.begin_generation(&service_id).await;
        let current_generation = service.begin_generation(&service_id).await;

        // The stale resolution finishing must not clear the newer
        // resolution's still-in-flight marker.
        service.end_resolving(&service_id, stale_generation).await;
        {
            let guard = service.inner.lock().await;
            assert_eq!(
                guard.resolving.get(&service_id),
                Some(&current_generation),
                "a stale end_resolving must leave the current generation's marker in place"
            );
        }

        // The current resolution finishing does clear its own marker.
        service.end_resolving(&service_id, current_generation).await;
        {
            let guard = service.inner.lock().await;
            assert_eq!(guard.resolving.get(&service_id), None);
        }
    }

    /// A fetcher whose response for one specific URL blocks on a
    /// `oneshot` receiver until the test signals it to proceed — lets a
    /// test control exactly when a slow candidate's fetch completes,
    /// relative to a second, faster `resolve` call racing it.
    struct GatedFetcher {
        gated_url: String,
        gated_bytes: Vec<u8>,
        gate: StdMutex<Option<tokio::sync::oneshot::Receiver<()>>>,
        others: FetchMap,
    }

    impl IconFetcher for GatedFetcher {
        fn fetch<'a>(&'a self, url: &'a Url) -> fetch::FetchFuture<'a> {
            let key = url.to_string();
            if key == self.gated_url {
                let gate = self.gate.lock().expect("lock").take();
                let bytes = self.gated_bytes.clone();
                return Box::pin(async move {
                    if let Some(gate) = gate {
                        let _ = gate.await;
                    }
                    Ok(bytes)
                });
            }
            let result = self
                .others
                .lock()
                .expect("lock")
                .get(&key)
                .cloned()
                .unwrap_or(Err(FetchError::Request("no fixture for url".to_string())));
            Box::pin(async move { result })
        }
    }

    #[tokio::test]
    async fn a_slow_resolution_finishing_after_a_newer_one_never_clobbers_its_cache() {
        let slow_url = "https://example.com/slow.png";
        let fast_url = "https://example.com/fast.png";
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let fetcher = Arc::new(GatedFetcher {
            gated_url: slow_url.to_string(),
            gated_bytes: one_pixel_png(),
            gate: StdMutex::new(Some(release_rx)),
            others: StdMutex::new(BTreeMap::from([(
                fast_url.to_string(),
                Ok(one_pixel_png()),
            )])),
        });
        let dir = tempfile::tempdir().expect("tempdir");
        let service = Arc::new(IconService::with_fetcher(dir.path().to_path_buf(), fetcher));
        let service_id = id("omicron");

        // The slow resolution (using the gated URL) starts first, taking
        // its generation before it ever awaits the network.
        service
            .record_candidates(service_id.clone(), vec![url(slow_url)])
            .await;
        let slow = tokio::spawn({
            let service = service.clone();
            let service_id = service_id.clone();
            async move { service.resolve(service_id, IconSource::Favicon).await }
        });

        // Give the slow task a chance to reach the gated fetch and
        // allocate its generation before the newer one starts.
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;

        // A newer resolution (the fast URL) starts and finishes while the
        // slow one is still blocked on the network.
        service
            .record_candidates(service_id.clone(), vec![url(fast_url)])
            .await;
        let fast_changed = service
            .resolve(service_id.clone(), IconSource::Favicon)
            .await;
        assert!(fast_changed, "the newer resolution must commit");
        let fast_cached = service
            .cached_icon(&service_id)
            .expect("newer resolution cached a PNG");

        // Now let the slow resolution finish: its write must be discarded,
        // leaving the newer resolution's cached file untouched.
        release_tx.send(()).expect("release the slow fetch");
        let slow_changed = slow.await.expect("slow task did not panic");
        assert!(
            !slow_changed,
            "a stale resolution must not report a cache change"
        );
        let after_cached = service
            .cached_icon(&service_id)
            .expect("cache must still be present");
        assert_eq!(
            after_cached.path, fast_cached.path,
            "the cache path is unchanged"
        );
    }
}
