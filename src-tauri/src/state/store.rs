//! Loading, atomically saving, and debounced background-saving of
//! `state.json` (design.md §2.2.2, §8.2, SPEC.md §13).

use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{AppError, AppResult};

use super::model::{State, SEEN_RING_CAPACITY};

/// How often [`StateStore`] writes its state to disk in the background, at
/// most (design.md §2.2.2: "at most once per second").
pub const SAVE_DEBOUNCE: Duration = Duration::from_secs(1);

/// Loads state from `path`.
///
/// Does not depend on Tauri; the caller resolves the path (see
/// `paths::state_file`).
///
/// - If `path` does not exist, returns [`State::empty`] without creating,
///   moving or deleting anything: an empty state is the expected shape on
///   first launch (design.md §2.2.2).
/// - If the file exists but cannot be parsed — corrupt syntax, a wrong
///   type, or a required field missing — returns `AppError::State` naming
///   the path and the underlying parse error. The file itself is left
///   untouched.
/// - Any other I/O failure becomes `AppError::Io`.
pub fn load(path: &Path) -> AppResult<State> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(State::empty()),
        Err(err) => return Err(AppError::Io(err)),
    };

    serde_json::from_slice(&bytes)
        .map_err(|err| AppError::State(format!("{}: {err}", path.display())))
}

/// Writes `state` to `path` atomically.
///
/// The parent directory is created if it does not exist. `state` is
/// serialized to pretty JSON and written to a temp file in the same
/// directory, `sync_all` is called on it, and it is then renamed onto
/// `path`. A serialization failure is `AppError::State`; any I/O failure is
/// `AppError::Io`.
pub fn save_atomic(path: &Path, state: &State) -> AppResult<()> {
    if let Some((id, ring)) = state
        .seen
        .iter()
        .find(|(_, ring)| ring.0.len() > SEEN_RING_CAPACITY)
    {
        return Err(AppError::State(format!(
            "{}: seen ring for `{}` has {} ids, over the capacity of {SEEN_RING_CAPACITY}",
            path.display(),
            id.as_str(),
            ring.0.len()
        )));
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| AppError::State(format!("{}: has no parent directory", path.display())))?;
    fs::create_dir_all(parent)?;

    let json = serde_json::to_vec_pretty(state)
        .map_err(|err| AppError::State(format!("{}: {err}", path.display())))?;

    let tmp_path = tmp_path_for(path)?;
    if let Err(err) = write_and_sync(&tmp_path, &json) {
        let _ = fs::remove_file(&tmp_path);
        return Err(err.into());
    }
    fs::rename(&tmp_path, path)?;
    sync_parent_dir(parent)?;
    Ok(())
}

fn write_and_sync(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Fsyncs `parent` itself after the rename, so the directory entry change
/// (the new name pointing at the renamed file) is durable too, not just the
/// file's own contents. Only meaningful on Unix, where a directory can be
/// opened and synced like a file; Windows has no equivalent and does not
/// need it (`fs::rename` there is already a durable metadata operation).
#[cfg(unix)]
fn sync_parent_dir(parent: &Path) -> io::Result<()> {
    fs::File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent_dir(_parent: &Path) -> io::Result<()> {
    Ok(())
}

fn tmp_path_for(path: &Path) -> AppResult<PathBuf> {
    let file_name = path
        .file_name()
        .ok_or_else(|| AppError::State(format!("{}: has no file name", path.display())))?;
    let mut tmp_name = file_name.to_os_string();
    tmp_name.push(".tmp");
    Ok(path.with_file_name(tmp_name))
}

/// Flags shared between [`StateStore`] and its background worker thread,
/// guarded together so the worker can wait on both with one [`Condvar`].
#[derive(Debug)]
struct WorkerFlags {
    dirty: bool,
    shutdown: bool,
}

#[derive(Debug)]
struct Shared {
    path: PathBuf,
    state: Mutex<State>,
    flags: Mutex<WorkerFlags>,
    condvar: Condvar,
    /// Serializes the "clear dirty, snapshot state, save_atomic" sequence
    /// between [`StateStore::flush`] and the background worker, so the two
    /// never interleave (e.g. both racing to save at once, or one clearing
    /// `dirty` out from under the other's in-flight save). Always acquired
    /// before `flags`.
    save_lock: Mutex<()>,
}

fn poison_err() -> AppError {
    AppError::State("state store mutex poisoned".to_string())
}

/// An in-memory `State`, debounced-saved to disk in the background.
///
/// Reads and updates go through an in-memory `Mutex<State>`; updates mark
/// the store dirty and wake the worker thread, which saves at most once per
/// [`SAVE_DEBOUNCE`] interval. [`StateStore::flush`] saves synchronously,
/// and is also called from `Drop` so nothing is lost on shutdown.
#[derive(Debug)]
pub struct StateStore {
    inner: Arc<Shared>,
    worker: Option<thread::JoinHandle<()>>,
}

impl StateStore {
    /// Loads `path` and starts the background save worker.
    ///
    /// The store (and its worker thread) are created only if the initial
    /// `load` succeeds; a corrupt file is returned as an error and no
    /// worker is started.
    pub fn open(path: PathBuf) -> AppResult<Self> {
        Self::open_with_interval(path, SAVE_DEBOUNCE)
    }

    fn open_with_interval(path: PathBuf, interval: Duration) -> AppResult<Self> {
        let state = load(&path)?;
        let shared = Arc::new(Shared {
            path,
            state: Mutex::new(state),
            flags: Mutex::new(WorkerFlags {
                dirty: false,
                shutdown: false,
            }),
            condvar: Condvar::new(),
            save_lock: Mutex::new(()),
        });

        let worker_shared = Arc::clone(&shared);
        let handle = thread::spawn(move || run_worker(worker_shared, interval));

        Ok(StateStore {
            inner: shared,
            worker: Some(handle),
        })
    }

    /// Runs `f` against the current in-memory state and returns its result.
    pub fn read<T>(&self, f: impl FnOnce(&State) -> T) -> AppResult<T> {
        let guard = self.inner.state.lock().map_err(|_| poison_err())?;
        Ok(f(&guard))
    }

    /// Runs `f` against the current in-memory state, marks the store dirty
    /// and wakes the background worker so the change is saved within
    /// [`SAVE_DEBOUNCE`].
    pub fn update<T>(&self, f: impl FnOnce(&mut State) -> T) -> AppResult<T> {
        let result = {
            let mut guard = self.inner.state.lock().map_err(|_| poison_err())?;
            f(&mut guard)
        };
        {
            let mut flags = self.inner.flags.lock().map_err(|_| poison_err())?;
            flags.dirty = true;
        }
        self.inner.condvar.notify_all();
        Ok(result)
    }

    /// Saves synchronously if the store is dirty; a no-op otherwise.
    pub fn flush(&self) -> AppResult<()> {
        // Held across clearing `dirty`, snapshotting and saving, so this
        // never interleaves with the background worker doing the same
        // (see `Shared::save_lock`). Acquired before `flags`.
        let _save_guard = self.inner.save_lock.lock().map_err(|_| poison_err())?;
        {
            let mut flags = self.inner.flags.lock().map_err(|_| poison_err())?;
            if !flags.dirty {
                return Ok(());
            }
            flags.dirty = false;
        }

        let snapshot = {
            let guard = self.inner.state.lock().map_err(|_| poison_err())?;
            guard.clone()
        };

        match save_atomic(&self.inner.path, &snapshot) {
            Ok(()) => Ok(()),
            Err(err) => {
                // Keep the change marked dirty so a later flush (or the
                // background worker) retries it.
                if let Ok(mut flags) = self.inner.flags.lock() {
                    flags.dirty = true;
                }
                self.inner.condvar.notify_all();
                Err(err)
            }
        }
    }
}

impl Drop for StateStore {
    fn drop(&mut self) {
        // A poisoned mutex is recovered (rather than left locked forever)
        // so the worker thread can still observe the shutdown request and
        // `join()` below does not hang.
        let mut flags = match self.inner.flags.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        flags.shutdown = true;
        drop(flags);
        self.inner.condvar.notify_all();

        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }

        // A change that raced with shutdown (marked dirty after the worker
        // last checked, but before it exited) is still captured by the
        // `dirty` flag; flush it synchronously so it is not lost.
        if let Err(err) = self.flush() {
            tracing::warn!(error = %err, "final state flush on shutdown failed");
        }
    }
}

/// Waits for a dirty (non-shutdown) state, debounces against `interval`
/// since the last save, snapshots and saves, then loops.
///
/// Exits as soon as shutdown is requested, even if a change is still
/// pending (dirty): a shutdown always wins the race with a save, so
/// `StateStore::drop` can join this thread promptly and do the final flush
/// itself, rather than this loop trying to sneak one more save in first.
fn run_worker(shared: Arc<Shared>, interval: Duration) {
    let mut last_saved: Option<Instant> = None;

    loop {
        let mut flags = lock_flags(&shared);
        loop {
            if flags.shutdown {
                return;
            }
            if flags.dirty {
                break;
            }
            flags = match shared.condvar.wait(flags) {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
        }

        if let Some(last) = last_saved {
            // Wait out the rest of the debounce interval. `wait_timeout`
            // can wake early on any `condvar.notify_all()` — including an
            // ordinary `update()` — not just a timeout or shutdown, so loop
            // and recompute the remaining wait rather than treating any
            // early wake as "the interval elapsed". Only a timeout or a
            // shutdown request ends the wait early; plain dirty-notifies do
            // not.
            loop {
                let elapsed = last.elapsed();
                if elapsed >= interval {
                    break;
                }
                let remaining = interval - elapsed;
                let (g, timeout) = match shared.condvar.wait_timeout(flags, remaining) {
                    Ok(v) => v,
                    Err(poisoned) => poisoned.into_inner(),
                };
                flags = g;
                if flags.shutdown {
                    return;
                }
                if timeout.timed_out() {
                    break;
                }
            }
        }
        drop(flags);

        // Held across clearing `dirty`, snapshotting and saving, so this
        // never interleaves with `StateStore::flush` doing the same (see
        // `Shared::save_lock`). Acquired before `flags`.
        let save_guard = lock_save(&shared);

        let mut flags = lock_flags(&shared);
        if !flags.dirty {
            // `flush()` already saved this change while we were waiting for
            // `save_lock`; nothing left to do this round.
            drop(flags);
            drop(save_guard);
            continue;
        }
        flags.dirty = false;
        drop(flags);

        let snapshot = {
            let guard = match shared.state.lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            guard.clone()
        };

        match save_atomic(&shared.path, &snapshot) {
            Ok(()) => last_saved = Some(Instant::now()),
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    path = %shared.path.display(),
                    "background state save failed; will retry"
                );
                let mut flags = lock_flags(&shared);
                flags.dirty = true;
                drop(flags);
                // Count this failed attempt as a "save" for debounce
                // purposes too, so a persistently failing save (e.g. disk
                // full) retries at most once per `interval` instead of
                // spinning.
                last_saved = Some(Instant::now());
            }
        }
        drop(save_guard);
    }
}

fn lock_flags(shared: &Shared) -> std::sync::MutexGuard<'_, WorkerFlags> {
    match shared.flags.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn lock_save(shared: &Shared) -> std::sync::MutexGuard<'_, ()> {
    match shared.save_lock.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::config::{ProfileName, ServiceId};
    use tempfile::tempdir;
    use uuid::Uuid;

    fn sample_state() -> State {
        let mut profiles = BTreeMap::new();
        profiles.insert(
            ProfileName::new("default").expect("valid profile"),
            Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
        );
        State {
            profiles,
            seen: BTreeMap::new(),
            staleness: BTreeMap::new(),
        }
    }

    // --- load / save_atomic --------------------------------------------

    #[test]
    fn load_missing_file_returns_empty_state_and_creates_nothing() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("nested").join("state.json");

        let state = load(&path).expect("load");
        assert_eq!(state, State::empty());
        assert!(!path.exists());
        assert!(!path.parent().unwrap().exists());
    }

    #[test]
    fn save_atomic_creates_parent_dir() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("nested").join("state.json");

        save_atomic(&path, &State::empty()).expect("save");

        assert!(path.exists());
        assert!(path.parent().unwrap().is_dir());
    }

    #[test]
    fn load_reads_known_json() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("state.json");
        fs::write(&path, r#"{"profiles":{},"seen":{},"staleness":{}}"#).expect("write");

        let state = load(&path).expect("load");
        assert_eq!(state, State::empty());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("state.json");
        let state = sample_state();

        save_atomic(&path, &state).expect("save");
        let loaded = load(&path).expect("load");

        assert_eq!(loaded, state);
    }

    #[test]
    fn save_atomic_leaves_no_temp_file() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("state.json");

        save_atomic(&path, &sample_state()).expect("save");

        let names: Vec<String> = fs::read_dir(dir.path())
            .expect("read_dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["state.json".to_string()]);
    }

    #[test]
    fn load_corrupt_syntax_is_state_error_and_leaves_file_unchanged() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("state.json");
        let original = b"{ this is not json".to_vec();
        fs::write(&path, &original).expect("write");

        let err = load(&path).expect_err("should fail");
        assert_eq!(err.kind(), "state");
        assert_eq!(fs::read(&path).expect("read"), original);
    }

    #[test]
    fn load_wrong_type_is_state_error_and_leaves_file_unchanged() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("state.json");
        let original = br#"{"profiles":[],"seen":{},"staleness":{}}"#.to_vec();
        fs::write(&path, &original).expect("write");

        let err = load(&path).expect_err("should fail");
        assert_eq!(err.kind(), "state");
        assert_eq!(fs::read(&path).expect("read"), original);
    }

    #[test]
    fn load_missing_field_is_state_error_and_leaves_file_unchanged() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("state.json");
        let original = br#"{"profiles":{},"seen":{}}"#.to_vec();
        fs::write(&path, &original).expect("write");

        let err = load(&path).expect_err("should fail");
        assert_eq!(err.kind(), "state");
        assert_eq!(fs::read(&path).expect("read"), original);
    }

    #[test]
    fn load_invalid_service_id_key_is_state_error_and_leaves_file_unchanged() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("state.json");
        let original = br#"{"profiles":{},"seen":{"Not Valid":[]},"staleness":{}}"#.to_vec();
        fs::write(&path, &original).expect("write");

        let err = load(&path).expect_err("should fail");
        assert_eq!(err.kind(), "state");
        assert_eq!(fs::read(&path).expect("read"), original);
    }

    #[test]
    fn save_rejects_an_over_capacity_seen_ring_and_writes_nothing() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("state.json");
        let mut state = State::empty();
        let ring = crate::state::model::SeenRing(
            (0..=SEEN_RING_CAPACITY).map(|n| n.to_string()).collect(),
        );
        state
            .seen
            .insert(ServiceId::new("svc-1").expect("valid id"), ring);

        let err = save_atomic(&path, &state).expect_err("over-capacity ring must be rejected");
        assert_eq!(err.kind(), "state");
        assert!(!path.exists());
    }

    // --- StateStore ------------------------------------------------------

    fn short_interval() -> Duration {
        Duration::from_millis(20)
    }

    #[test]
    fn open_missing_file_starts_with_empty_state() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("state.json");

        let store = StateStore::open_with_interval(path.clone(), short_interval()).expect("open");
        let state = store.read(|s| s.clone()).expect("read");
        assert_eq!(state, State::empty());
    }

    #[test]
    fn update_then_flush_persists_to_disk() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("state.json");
        let store = StateStore::open_with_interval(path.clone(), short_interval()).expect("open");

        store
            .update(|s| {
                s.profiles.insert(
                    ProfileName::new("default").expect("valid profile"),
                    Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap(),
                );
            })
            .expect("update");
        store.flush().expect("flush");

        let on_disk = load(&path).expect("load");
        let in_memory = store.read(|s| s.clone()).expect("read");
        assert_eq!(on_disk, in_memory);
        assert_eq!(
            on_disk
                .profiles
                .get(&ProfileName::new("default").expect("valid profile")),
            Some(&Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap())
        );
    }

    #[test]
    fn drop_flushes_pending_update() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("state.json");

        {
            let store = StateStore::open_with_interval(path.clone(), Duration::from_secs(60))
                .expect("open");
            store
                .update(|s| {
                    s.staleness.insert(
                        ServiceId::new("svc-1").expect("valid id"),
                        super::super::model::StalenessStats {
                            count: 3,
                            last_at: Some(42),
                        },
                    );
                })
                .expect("update");
            // Store is dropped here, before the long debounce interval
            // elapses; Drop must flush synchronously.
        }

        let on_disk = load(&path).expect("load");
        assert_eq!(
            on_disk
                .staleness
                .get(&ServiceId::new("svc-1").expect("valid id"))
                .map(|s| s.count),
            Some(3)
        );
    }

    #[test]
    fn drop_while_dirty_completes_promptly() {
        // The worker must exit as soon as shutdown is requested, even with
        // a pending dirty change, so Drop doesn't block waiting for the
        // worker to finish an in-progress debounce wait/save first; it does
        // the final flush itself instead. With a long debounce interval,
        // a `Drop` that still waited on the worker would take (close to)
        // the whole interval; one that hands off to its own synchronous
        // flush completes almost immediately.
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("state.json");
        let long_interval = Duration::from_secs(60);

        let store = StateStore::open_with_interval(path.clone(), long_interval).expect("open");
        store
            .update(|s| {
                s.staleness.insert(
                    ServiceId::new("svc-1").expect("valid id"),
                    super::super::model::StalenessStats {
                        count: 1,
                        last_at: None,
                    },
                );
            })
            .expect("update");

        let start = Instant::now();
        drop(store);
        let elapsed = start.elapsed();

        assert!(
            elapsed < Duration::from_secs(5),
            "drop took {elapsed:?}, expected it to complete promptly rather than \
             waiting out the {long_interval:?} debounce interval"
        );

        let on_disk = load(&path).expect("load");
        assert_eq!(
            on_disk
                .staleness
                .get(&ServiceId::new("svc-1").expect("valid id"))
                .map(|s| s.count),
            Some(1)
        );
    }

    #[test]
    fn open_corrupt_syntax_is_state_error_and_leaves_file_unchanged() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("state.json");
        let original = b"not json at all".to_vec();
        fs::write(&path, &original).expect("write");

        let err = StateStore::open_with_interval(path.clone(), short_interval())
            .expect_err("should fail");
        assert_eq!(err.kind(), "state");
        assert_eq!(fs::read(&path).expect("read"), original);
    }

    #[test]
    fn open_wrong_type_is_state_error_and_leaves_file_unchanged() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("state.json");
        let original = br#"{"profiles":{},"seen":[],"staleness":{}}"#.to_vec();
        fs::write(&path, &original).expect("write");

        let err = StateStore::open_with_interval(path.clone(), short_interval())
            .expect_err("should fail");
        assert_eq!(err.kind(), "state");
        assert_eq!(fs::read(&path).expect("read"), original);
    }

    #[test]
    fn open_missing_field_is_state_error_and_leaves_file_unchanged() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("state.json");
        let original = br#"{"seen":{},"staleness":{}}"#.to_vec();
        fs::write(&path, &original).expect("write");

        let err = StateStore::open_with_interval(path.clone(), short_interval())
            .expect_err("should fail");
        assert_eq!(err.kind(), "state");
        assert_eq!(fs::read(&path).expect("read"), original);
    }

    #[test]
    fn background_worker_saves_dirty_state_within_debounce() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("state.json");
        let store = StateStore::open_with_interval(path.clone(), short_interval()).expect("open");

        store
            .update(|s| {
                s.profiles.insert(
                    ProfileName::new("default").expect("valid profile"),
                    Uuid::parse_str("00000000-0000-0000-0000-000000000003").unwrap(),
                );
            })
            .expect("update");

        // Give the background worker time to wake up and save, well beyond
        // the short debounce interval used in this test.
        thread::sleep(short_interval() * 10);

        let on_disk = load(&path).expect("load");
        assert_eq!(
            on_disk
                .profiles
                .get(&ProfileName::new("default").expect("valid profile")),
            Some(&Uuid::parse_str("00000000-0000-0000-0000-000000000003").unwrap())
        );
    }

    #[test]
    fn rapid_updates_within_interval_do_not_shorten_debounce() {
        // Each `update()` notifies the worker's condvar. If that early wake
        // were mistaken for "the debounce interval elapsed" (as it used to
        // be), a burst of updates would cause a save well before `interval`
        // has actually passed since the worker's last save. Use generous
        // margins throughout so this isn't sensitive to exact scheduling.
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("state.json");
        let interval = Duration::from_millis(300);
        let store = StateStore::open_with_interval(path.clone(), interval).expect("open");

        let first = ProfileName::new("first").expect("valid profile");
        let second = ProfileName::new("second").expect("valid profile");

        // The very first save has no prior save to debounce against, so it
        // lands promptly. Wait for it, so the worker's internal
        // `last_saved` is set before the part of this test that matters.
        store
            .update(|s| {
                s.profiles.insert(first.clone(), Uuid::nil());
            })
            .expect("update");
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if load(&path).expect("load").profiles.contains_key(&first) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "initial (non-debounced) save did not land in time"
            );
            thread::sleep(Duration::from_millis(5));
        }

        // Fire off a burst of updates, each notifying the worker while it
        // should be sitting in its post-save debounce wait.
        for _ in 0..5 {
            thread::sleep(Duration::from_millis(10));
            store
                .update(|s| {
                    s.profiles.insert(second.clone(), Uuid::nil());
                })
                .expect("update");
        }

        // Well before `interval` has elapsed since the worker's last save,
        // the second save must not have landed yet.
        thread::sleep(Duration::from_millis(100));
        assert!(
            !load(&path).expect("load").profiles.contains_key(&second),
            "background worker saved before the debounce interval elapsed"
        );

        // Well after the interval has elapsed, the second save must have
        // landed.
        thread::sleep(interval);
        assert!(load(&path).expect("load").profiles.contains_key(&second));
    }
}
