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

use std::collections::{BTreeMap, BTreeSet};
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
    /// Services a resolution is currently in flight for, so a burst of
    /// `report_unread` calls (or a refresh arriving mid-resolution) never
    /// starts a second, redundant resolution for the same id.
    resolving: BTreeSet<ServiceId>,
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
    fn begin_resolving(&self, guard: &mut State, id: &ServiceId) -> bool {
        guard.resolving.insert(id.clone())
    }

    async fn end_resolving(&self, id: &ServiceId) {
        let mut guard = self.inner.lock().await;
        guard.resolving.remove(id);
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

    /// Copies `source_path` (an arbitrary, user-chosen absolute path) into
    /// `{data_dir}/icons/<id>.src` (design.md §2.2.10, §2.2.12's file
    /// override), creating the `icons` directory first if needed, and
    /// returns the relative path (`icons/<id>.src`) to store as the
    /// service's `config.toml` `icon.value` (`config::validate`'s "must
    /// stay inside `data_dir`" rule, already enforced there, is satisfied
    /// by construction: this is always the same fixed relative path).
    pub fn install_override_file(
        &self,
        id: &ServiceId,
        source_path: &Path,
    ) -> std::io::Result<PathBuf> {
        let relative = paths::icon_override_relative_path(id);
        let destination = self.data_dir.join(&relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(source_path, &destination)?;
        Ok(relative)
    }

    /// Runs resolution for `id` now: builds the order from `override_source`
    /// and `id`'s last-known candidates, tries each step, and — on the
    /// first success — writes the normalised PNG to `id`'s cache path
    /// (via a temp file renamed into place, so a reader never observes a
    /// partially written file). Returns whether the cache changed (a new
    /// PNG was written), which the caller uses to decide whether to emit
    /// `service-icon-changed`.
    ///
    /// Always clears the "resolving" flag for `id` before returning, even
    /// on failure, so a later `record_candidates`/`refresh` can try again.
    pub async fn resolve(&self, id: ServiceId, override_source: IconSource) -> bool {
        let candidates = self.known_candidates(&id).await;
        let order = resolution_order(Some(&override_source), &candidates);
        let result = resolve_bytes(&self.data_dir, self.fetcher.as_ref(), &order).await;
        self.end_resolving(&id).await;

        let Some(png) = result else {
            return false;
        };
        match self.write_cache(&id, &png) {
            Ok(()) => true,
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
    /// place, creating `{data_dir}/icons` first if needed.
    fn write_cache(&self, id: &ServiceId, png: &[u8]) -> std::io::Result<()> {
        let dir = paths::icons_dir(&self.data_dir);
        std::fs::create_dir_all(&dir)?;
        let final_path = paths::icon_cache_file(&self.data_dir, id);
        let tmp_path = dir.join(format!("{id}.png.tmp"));
        std::fs::write(&tmp_path, png)?;
        std::fs::rename(&tmp_path, &final_path)?;
        Ok(())
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
}
