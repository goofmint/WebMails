//! macOS App Nap assertion (design.md §2.2.8, §2.2.11; SPEC.md §9.1).
//!
//! While Eluma has at least one *created* service webview, an
//! `NSProcessInfo` activity assertion tells macOS not to App-Nap this
//! process, so backgrounded service webviews keep polling for unread
//! counts even when Eluma is not the frontmost app. [`AppNapGuard::sync`]
//! is the single entry point: `ServiceManager` calls it with the current
//! resident (created) service count every time that count changes, and
//! the guard acquires or releases the assertion exactly when the count
//! crosses the 0/1 boundary.
//!
//! Non-macOS builds compile the same [`AppNapGuard`] API to no-ops:
//! there is no App Nap outside macOS, so [`begin`] and [`end`] do
//! nothing and hold no real OS resource. This is a deliberate,
//! documented no-op — not a cfg-less stub that pretends to hold an
//! assertion it does not have.

use std::sync::Mutex;

/// What [`AppNapGuard::sync`] should do, given the resident service
/// count before and after a change.
///
/// A pure function of the two counts (design.md's Task 3.1 note): only
/// whether the count crosses the 0/1 boundary matters, not its absolute
/// value, so e.g. 1→2 or 2→1 is always [`HoldAction::None`] — only the
/// first service to appear, or the last one to disappear, touches the
/// assertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldAction {
    /// The count went from 0 to non-zero: acquire the assertion.
    Acquire,
    /// The count went from non-zero to 0: release the assertion.
    Release,
    /// Neither boundary was crossed: leave the current assertion state
    /// (held or not) as it is.
    None,
}

/// Pure decision function backing [`AppNapGuard::sync`], kept separate
/// so it can be unit-tested (see the `tests` module below) without
/// touching any native API.
pub fn decide_hold_action(previous_count: usize, new_count: usize) -> HoldAction {
    match (previous_count == 0, new_count == 0) {
        (true, false) => HoldAction::Acquire,
        (false, true) => HoldAction::Release,
        _ => HoldAction::None,
    }
}

#[cfg(target_os = "macos")]
mod platform_impl {
    use objc2::rc::Retained;
    use objc2::runtime::{NSObjectProtocol, ProtocolObject};
    use objc2_foundation::{ns_string, NSActivityOptions, NSProcessInfo};

    /// The activity option passed to `beginActivityWithOptions:reason:`
    /// (design.md §10 open point 1; SPEC.md §9.1).
    ///
    /// SPEC.md §9.1 specifies `NSActivityUserInitiated`, which is what
    /// this constant holds. **Trade-off:** `NSActivityUserInitiated`
    /// also sets `NSActivityIdleSystemSleepDisabled`, so besides
    /// exempting the process from App Nap it also prevents the Mac from
    /// going to *idle system sleep* on its own for as long as the
    /// assertion is held — i.e. for as long as Eluma has at least one
    /// service. design.md §10 proposes
    /// `NSActivityUserInitiatedAllowingIdleSystemSleep` instead, which
    /// keeps the App Nap exemption without blocking idle system sleep,
    /// but **the owner has not yet confirmed that change** (open point
    /// 1, still open as of Task 3.1). Do not switch this constant
    /// without also updating design.md §10's decision record.
    pub(super) const ACTIVITY_OPTIONS: NSActivityOptions = NSActivityOptions::UserInitiated;

    /// Logged alongside [`super::AppNapGuard::sync`]'s acquire/release
    /// lines, so the option in effect can be cross-checked against
    /// `pmset -g assertions` output at runtime.
    pub(super) const ACTIVITY_OPTIONS_NAME: &str = "NSActivityUserInitiated";

    /// An opaque handle to a held `NSProcessInfo` activity assertion.
    /// The only valid use of the wrapped value is handing it back to
    /// `endActivity:` via [`end`]; dropping it without calling [`end`]
    /// leaks the assertion for the rest of the process's lifetime.
    pub struct ActivityToken(Retained<ProtocolObject<dyn NSObjectProtocol>>);

    // SAFETY: `ActivityToken` wraps the opaque object `NSProcessInfo`
    // returns from `beginActivityWithOptions:reason:`. Apple's
    // documentation for that method and for `endActivity:` places no
    // thread restriction on either. The only operations this crate ever
    // performs on the wrapped value are (a) `Retained`'s `release` when
    // an `ActivityToken` is dropped, and (b) reading `&self.0` to pass
    // to `endActivity:` in `end` below — both rely solely on
    // Objective-C's atomic ARC retain/release, which is thread-safe by
    // construction, and neither mutates the object through a shared
    // reference. So `Send` (moving the token to another thread, e.g. a
    // different `tokio` worker) and `Sync` (sharing `&ActivityToken`,
    // e.g. through `AppNapGuard`'s `Mutex`) are both sound even though
    // `dyn NSObjectProtocol` does not imply either on its own.
    unsafe impl Send for ActivityToken {}
    unsafe impl Sync for ActivityToken {}

    /// Begins an `NSProcessInfo` activity assertion with
    /// [`ACTIVITY_OPTIONS`] (SPEC.md §9.1).
    pub(super) fn begin() -> ActivityToken {
        let process_info = NSProcessInfo::processInfo();
        let reason = ns_string!("Eluma is keeping webmail services live");
        let activity = process_info.beginActivityWithOptions_reason(ACTIVITY_OPTIONS, reason);
        ActivityToken(activity)
    }

    /// Ends a previously [`begin`]-gun activity assertion.
    pub(super) fn end(token: ActivityToken) {
        let process_info = NSProcessInfo::processInfo();
        // SAFETY: `token.0` was returned by this same process's own
        // earlier `beginActivityWithOptions:reason:` call — the only way
        // to construct an `ActivityToken` — so it is "of the correct
        // type" as `endActivity:`'s safety doc requires.
        unsafe { process_info.endActivity(&token.0) };
    }
}

#[cfg(not(target_os = "macos"))]
mod platform_impl {
    /// No-op on non-macOS platforms: App Nap does not exist there, so
    /// there is nothing to hold. This is a real (empty) type, not a type
    /// alias standing in for a native handle, so it cannot be mistaken
    /// for holding an actual OS resource.
    pub struct ActivityToken;

    pub(super) const ACTIVITY_OPTIONS_NAME: &str = "none (App Nap does not apply on this platform)";

    pub(super) fn begin() -> ActivityToken {
        ActivityToken
    }

    pub(super) fn end(_token: ActivityToken) {}
}

pub use platform_impl::ActivityToken;

/// Acquires a new activity assertion. macOS: begins an `NSProcessInfo`
/// assertion (SPEC.md §9.1). Every other platform: a documented no-op
/// (module doc above).
pub fn begin() -> ActivityToken {
    platform_impl::begin()
}

/// Releases an activity assertion previously returned by [`begin`].
pub fn end(token: ActivityToken) {
    platform_impl::end(token)
}

struct GuardState {
    /// The resident (created) service count [`AppNapGuard::sync`] last
    /// saw, so the next call can compute a before/after transition.
    resident_count: usize,
    /// `Some` while the assertion is held.
    token: Option<ActivityToken>,
}

/// Holds the App Nap assertion token (design.md §2.2.8) for as long as
/// `ServiceManager` has at least one created service, and releases it
/// once the count returns to zero.
///
/// The token lives behind a private `Mutex`, alongside the resident
/// count `sync` last saw, so [`Self::sync`] can compute
/// [`decide_hold_action`] purely from "before" and "after" without the
/// caller (`ServiceManager`) having to track that itself.
pub struct AppNapGuard {
    state: Mutex<GuardState>,
}

impl AppNapGuard {
    /// Starts with no assertion held and a resident count of 0 — correct
    /// for a freshly constructed `ServiceManager`, which has not created
    /// any service webview yet.
    pub fn new() -> Self {
        AppNapGuard {
            state: Mutex::new(GuardState {
                resident_count: 0,
                token: None,
            }),
        }
    }

    /// Updates the resident service count and acquires or releases the
    /// assertion if that crosses the 0/1 boundary (design.md §2.2.8).
    /// Call this every time `ServiceManager`'s `created` set changes —
    /// after a `host.create` that inserts into it, and after a
    /// `host.destroy` that removes from it. Logs one `tracing::info!`
    /// line on each acquire and each release; a poisoned internal mutex
    /// (a prior panic while holding it) is logged and skipped rather
    /// than propagated, since App Nap bookkeeping must never bring down
    /// service creation/removal.
    pub fn sync(&self, new_count: usize) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(err) => {
                tracing::error!("app_nap: guard mutex poisoned, skipping sync: {err}");
                return;
            }
        };
        let previous_count = state.resident_count;
        state.resident_count = new_count;
        match decide_hold_action(previous_count, new_count) {
            HoldAction::Acquire => {
                state.token = Some(begin());
                tracing::info!(
                    resident_count = new_count,
                    activity_options = platform_impl::ACTIVITY_OPTIONS_NAME,
                    "app_nap: acquired activity assertion"
                );
            }
            HoldAction::Release => {
                if let Some(token) = state.token.take() {
                    end(token);
                    tracing::info!(
                        resident_count = new_count,
                        "app_nap: released activity assertion"
                    );
                }
            }
            HoldAction::None => {}
        }
    }
}

impl Default for AppNapGuard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Native APIs are not tested here (module doc's split between the
    // pure `decide_hold_action` and the native `platform_impl` exists
    // precisely so these don't need a macOS runtime to run).

    #[test]
    fn zero_to_one_acquires() {
        assert_eq!(decide_hold_action(0, 1), HoldAction::Acquire);
    }

    #[test]
    fn one_to_two_is_noop() {
        assert_eq!(decide_hold_action(1, 2), HoldAction::None);
    }

    #[test]
    fn two_to_one_is_noop() {
        assert_eq!(decide_hold_action(2, 1), HoldAction::None);
    }

    #[test]
    fn one_to_zero_releases() {
        assert_eq!(decide_hold_action(1, 0), HoldAction::Release);
    }

    #[test]
    fn guard_sync_runs_acquire_and_release_paths_without_panicking() {
        let guard = AppNapGuard::new();
        guard.sync(1); // 0 -> 1: acquire
        guard.sync(2); // 1 -> 2: no-op
        guard.sync(1); // 2 -> 1: no-op
        guard.sync(0); // 1 -> 0: release
    }
}
