//! The impure half of liveness monitoring (module doc in `mod.rs` has the
//! full picture): a `tokio::time::interval` task that drives
//! [`super::machine::LivenessMachine`] and applies its [`super::Action`]s
//! through [`ServiceManager`], the shared `unread::StatusStore`, and
//! [`StateStore`].

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex as SyncMutex};
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, Wry};

use crate::config::ServiceId;
use crate::services::ServiceManager;
use crate::state::{StalenessStats, StateStore};
use crate::unread::{emit_if_changed, ServiceStatus, StatusStore};

use super::machine::{Action, LivenessMachine};
use super::TICK;

/// The webview label every shell-only event targets (matches
/// `services::SHELL_LABEL`, which is private to that module — duplicated
/// here rather than exported, since it is a one-line constant and this is
/// the only other place that needs it).
const SHELL_LABEL: &str = "shell";

/// The event name for an `unread::StatusChanged` payload (matches
/// `services::STATUS_CHANGED_EVENT`, duplicated for the same reason as
/// [`SHELL_LABEL`]).
const STATUS_CHANGED_EVENT: &str = "status-changed";

/// A source of the current time in Unix epoch milliseconds, injected so
/// [`LivenessRuntime`] itself never reads the wall clock directly. The
/// pure [`LivenessMachine`] never needs this — every one of its methods
/// takes `now_ms` explicitly — this trait exists only to supply that
/// value once per tick (and once per report) in production.
///
/// Fallible, deliberately: the project forbids substituting a fallback
/// value for one that could not be obtained. [`evaluate_tick`] and
/// [`evaluate_touch`] are what a `now_ms()` error actually leads to —
/// skipping that tick or that report's touch entirely, never a
/// made-up time.
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> Result<u64, ClockError>;
}

/// Why [`Clock::now_ms`] could not produce a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockError(String);

impl fmt::Display for ClockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The production [`Clock`]: the real wall clock, in Unix epoch
/// milliseconds — the same unit [`crate::state::StalenessStats::last_at`]
/// persists (design.md §2.2.2).
#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> Result<u64, ClockError> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            // The system clock is set before the Unix epoch — not
            // something this app can recover from or meaningfully fall
            // back on (project rule: no fallback defaults). The caller
            // ([`evaluate_tick`]/[`evaluate_touch`]) logs this and skips
            // the tick/touch entirely rather than substituting a time.
            .map_err(|err| ClockError(err.to_string()))
    }
}

/// Evaluates one tick against `clock`: on success, runs
/// [`LivenessMachine::tick`] as normal; on a clock error, skips the tick
/// entirely — no actions, and `machine`'s own state is left untouched —
/// logging at error level instead of substituting a fallback time
/// (project rule: no fallback defaults). Tauri-free, so it is
/// unit-tested directly below; [`LivenessRuntime::tick_once`] is its only
/// production caller.
fn evaluate_tick(
    clock: &dyn Clock,
    machine: &mut LivenessMachine,
    statuses: &HashMap<ServiceId, ServiceStatus>,
) -> Vec<Action> {
    match clock.now_ms() {
        Ok(now_ms) => machine.tick(now_ms, statuses),
        Err(err) => {
            tracing::error!("liveness: clock unavailable, skipping this tick: {err}");
            Vec::new()
        }
    }
}

/// Evaluates a report-arrival touch against `clock`: on success, touches
/// `machine` and returns the time used; on a clock error, skips the touch
/// entirely — `machine`'s own state is left untouched, `None` is
/// returned — logging at error level instead of substituting a fallback
/// time. Tauri-free, unit-tested directly below;
/// [`LivenessRuntime::record_report`] is its only production caller.
fn evaluate_touch(clock: &dyn Clock, machine: &mut LivenessMachine, id: &ServiceId) -> Option<u64> {
    match clock.now_ms() {
        Ok(now_ms) => {
            machine.touch(id, now_ms);
            Some(now_ms)
        }
        Err(err) => {
            tracing::error!(service_id = %id, "liveness: clock unavailable, skipping report touch: {err}");
            None
        }
    }
}

/// The [`ServiceId`] an [`Action`] names — every variant has exactly one,
/// under the same field name.
fn action_service_id(action: &Action) -> &ServiceId {
    match action {
        Action::MarkStale { id } | Action::Reload { id } | Action::Recreate { id } => id,
    }
}

/// An [`Action`] paired with the generation ([`LivenessMachine::
/// generation`]) its service had at the exact moment `tick` decided it —
/// read under the same `machine` lock `tick` itself just ran under, so it
/// reflects that decision precisely, with nothing able to race it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Tagged {
    action: Action,
    generation: u64,
}

/// Tags every action in `actions` with its service's *current* generation
/// (read from `machine`, still under the same lock `tick` produced
/// `actions` with — see [`Tagged`]). An action naming a service `machine`
/// no longer tracks is dropped outright rather than tagged with a made-up
/// generation: `tick`'s own postcondition (every action's service is
/// tracked immediately after it returns) rules this out in practice, so
/// it is logged as the anomaly it would be, not silently worked around.
/// Tauri-free, unit-tested directly below.
fn tag_actions(machine: &LivenessMachine, actions: Vec<Action>) -> Vec<Tagged> {
    actions
        .into_iter()
        .filter_map(|action| {
            let id = action_service_id(&action);
            match machine.generation(id) {
                Some(generation) => Some(Tagged { action, generation }),
                None => {
                    tracing::error!(
                        service_id = %id,
                        "liveness: action decided for a service no longer tracked, discarding"
                    );
                    None
                }
            }
        })
        .collect()
}

/// Whether `tagged` is still safe to apply: `current_generation` — what
/// [`LivenessMachine::generation`] reports for its service *right now* —
/// still equals the generation it was tagged with. `None` (no longer
/// tracked at all — removed, or never was) is also a mismatch. Pure,
/// unit-tested directly below; [`LivenessRuntime::apply_tagged_action`]
/// is its only production caller, called right before applying, so a
/// concurrent `touch` (design.md §2.2.6: a report arriving resets
/// everything) that lands between `tick` deciding this action and the
/// runtime actually applying it is exactly what this catches — the
/// service already recovered, so reloading/recreating it now would be
/// wrong.
fn is_current(tagged: &Tagged, current_generation: Option<u64>) -> bool {
    current_generation == Some(tagged.generation)
}

/// Whether a freshly received report's `now_ms` should replace `existing`
/// (`state.staleness[id].last_at`): only if there was no previous value,
/// or the new one is not older (design.md §2.2.2) — reports can arrive
/// out of order (e.g. two concurrent `report_unread` calls), and letting
/// an earlier one overwrite a later one would make a service look staler
/// than it actually is. Pure, unit-tested directly below;
/// [`LivenessRuntime::record_report`] is its only production caller.
fn is_newer_last_at(now_ms: u64, existing: Option<u64>) -> bool {
    match existing {
        None => true,
        Some(existing) => now_ms >= existing,
    }
}

/// Runs the 30s liveness tick loop (design.md §2.2.8, SPEC.md §9.4) and
/// exposes [`Self::record_report`] for `agent_bridge::report_unread` to
/// call right after a report validates.
pub struct LivenessRuntime {
    machine: SyncMutex<LivenessMachine>,
    clock: Arc<dyn Clock>,
    service_manager: Arc<ServiceManager>,
    state: Arc<StateStore>,
    status_store: Arc<SyncMutex<StatusStore>>,
    app_handle: AppHandle<Wry>,
}

impl LivenessRuntime {
    /// Builds a runtime around `service_manager`'s own `unread` status
    /// store (via [`ServiceManager::status_store`]) and `state`, using the
    /// real [`SystemClock`]. `lib.rs`'s `setup` hook is the only
    /// production caller; tests build a [`LivenessMachine`] directly
    /// instead (see `machine`'s own `#[cfg(test)]` module).
    pub fn new(
        service_manager: Arc<ServiceManager>,
        state: Arc<StateStore>,
        app_handle: AppHandle<Wry>,
    ) -> Arc<Self> {
        let status_store = service_manager.status_store();
        Arc::new(LivenessRuntime {
            machine: SyncMutex::new(LivenessMachine::new()),
            clock: Arc::new(SystemClock),
            service_manager,
            state,
            status_store,
            app_handle,
        })
    }

    /// Spawns the tick loop on `tauri::async_runtime` and returns
    /// immediately; the caller never blocks on it (matches
    /// `ServiceManager::start`'s own contract).
    pub fn spawn(self: Arc<Self>) {
        tauri::async_runtime::spawn(async move {
            self.run().await;
        });
    }

    async fn run(&self) {
        let mut interval = tokio::time::interval(TICK);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // `interval`'s first tick fires immediately; discard it so the
        // first real evaluation happens a full `TICK` after startup, not
        // the instant this task is spawned (design.md §2.2.8: a 30s
        // ticker, not an on-launch check).
        interval.tick().await;
        loop {
            interval.tick().await;
            self.tick_once().await;
        }
    }

    async fn tick_once(&self) {
        let snapshot = self.service_manager.snapshot().await;

        let tagged_actions = {
            let mut machine = self.lock_machine();
            let actions = evaluate_tick(self.clock.as_ref(), &mut machine, &snapshot.statuses);
            tag_actions(&machine, actions)
        };

        for tagged in tagged_actions {
            self.apply_tagged_action(tagged).await;
        }
    }

    /// Re-checks `tagged`'s generation immediately before applying it
    /// (module-level [`is_current`]'s own doc has the full picture):
    /// discards it, instead of applying it, if a report has reset the
    /// service since `tick` decided this action.
    async fn apply_tagged_action(&self, tagged: Tagged) {
        let id = action_service_id(&tagged.action).clone();
        let current_generation = self.lock_machine().generation(&id);
        if !is_current(&tagged, current_generation) {
            tracing::warn!(
                service_id = %id,
                "liveness: discarding a stale action (the service recovered or was removed since it was decided)"
            );
            return;
        }
        self.apply_action(tagged.action).await;
    }

    async fn apply_action(&self, action: Action) {
        match action {
            Action::MarkStale { id } => self.apply_mark_stale(&id),
            Action::Reload { id } => {
                if let Err(err) = self.service_manager.reload_service(&id).await {
                    tracing::warn!(service_id = %id, "liveness: reload failed: {err}");
                }
            }
            Action::Recreate { id } => {
                tracing::warn!(service_id = %id, "liveness: still stale after reload, recreating");
                if let Err(err) = self.service_manager.recreate_service(&id).await {
                    tracing::warn!(service_id = %id, "liveness: recreate failed: {err}");
                }
            }
        }
    }

    fn apply_mark_stale(&self, id: &ServiceId) {
        tracing::warn!(service_id = %id, "liveness: service missed too many reports, marking stale");

        let changed = {
            let mut store = self.lock_status_store();
            store.mark_stale(id)
        };
        emit_if_changed(changed, |changed| {
            if let Err(err) = self
                .app_handle
                .emit_to(SHELL_LABEL, STATUS_CHANGED_EVENT, changed)
            {
                tracing::warn!("liveness: failed to emit {STATUS_CHANGED_EVENT}: {err}");
            }
        });

        if let Err(err) = self.state.update(|state| {
            let stats = state.staleness.entry(id.clone()).or_insert(StalenessStats {
                count: 0,
                last_at: None,
            });
            stats.count += 1;
        }) {
            tracing::error!(service_id = %id, "liveness: failed to record staleness count: {err}");
        }
    }

    /// Records that `id` is alive as of now: resets the liveness state
    /// machine's tracking for `id` and updates `state.staleness[id].
    /// last_at` (design.md §2.2.2, §2.2.6). Called by
    /// `agent_bridge::report_unread` immediately after a report validates
    /// — the design's "liveness uses the time Rust receives the report",
    /// using this runtime's own [`Clock`] rather than the report's
    /// `observedAt`.
    pub fn record_report(&self, id: &ServiceId) {
        let now_ms = {
            let mut machine = self.lock_machine();
            evaluate_touch(self.clock.as_ref(), &mut machine, id)
        };
        let Some(now_ms) = now_ms else {
            // The clock errored: `evaluate_touch` already logged it, and
            // already left `machine` untouched. Skipping the `state`
            // update too is the same "no fallback time" rule applied to
            // `last_at` (project rule: no fallback defaults).
            return;
        };

        if let Err(err) = self.state.update(|state| {
            let stats = state.staleness.entry(id.clone()).or_insert(StalenessStats {
                count: 0,
                last_at: None,
            });
            if is_newer_last_at(now_ms, stats.last_at) {
                stats.last_at = Some(now_ms);
            }
        }) {
            tracing::error!(service_id = %id, "liveness: failed to record last report time: {err}");
        }
    }

    /// Recovers from a poisoned lock the same way `ServiceManager::
    /// with_status_store` does: every mutation the machine or the status
    /// store can run is a plain, infallible transition, so there is never
    /// anything to roll back.
    fn lock_machine(&self) -> std::sync::MutexGuard<'_, LivenessMachine> {
        match self.machine.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn lock_status_store(&self) -> std::sync::MutexGuard<'_, StatusStore> {
        match self.status_store.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test double whose clock never works — used to prove
    /// [`evaluate_tick`] and [`evaluate_touch`] skip entirely rather than
    /// substituting a fallback time.
    struct FailingClock;

    impl Clock for FailingClock {
        fn now_ms(&self) -> Result<u64, ClockError> {
            Err(ClockError("clock unavailable in test".to_string()))
        }
    }

    struct FixedClock(u64);

    impl Clock for FixedClock {
        fn now_ms(&self) -> Result<u64, ClockError> {
            Ok(self.0)
        }
    }

    fn id(value: &str) -> ServiceId {
        ServiceId::new(value).expect("valid id")
    }

    fn statuses(pairs: &[(&str, ServiceStatus)]) -> HashMap<ServiceId, ServiceStatus> {
        pairs.iter().map(|(k, v)| (id(k), v.clone())).collect()
    }

    // 2 missed reports * 30s + 5s grace + 1ms (design.md §2.2.8) — kept
    // literal here since `machine`'s own threshold constant is private to
    // that module; `machine::tests` asserts the same value against its
    // own constant.
    const PAST_THRESHOLD_MS: u64 = 65_001;

    #[test]
    fn evaluate_tick_skips_entirely_when_the_clock_errors() {
        let mut machine = LivenessMachine::new();
        let s = statuses(&[("gmail", ServiceStatus::Loading)]);

        // Establish a baseline with a working clock.
        let actions = evaluate_tick(&FixedClock(1_000), &mut machine, &s);
        assert!(actions.is_empty());

        // Long past the staleness threshold, but the clock now errors:
        // no actions must be produced.
        let actions = evaluate_tick(&FailingClock, &mut machine, &s);
        assert!(actions.is_empty());

        // The machine's own state was never touched by the failed tick
        // above: a working clock, still measured from the *original*
        // baseline (1_000), finds the service stale exactly where it
        // should — proving the failed tick changed nothing.
        let actions = evaluate_tick(&FixedClock(1_000 + PAST_THRESHOLD_MS), &mut machine, &s);
        assert_eq!(
            actions,
            vec![
                Action::MarkStale { id: id("gmail") },
                Action::Reload { id: id("gmail") },
            ]
        );
    }

    #[test]
    fn evaluate_touch_skips_and_leaves_the_machine_unchanged_when_the_clock_errors() {
        let mut machine = LivenessMachine::new();
        machine.touch(&id("gmail"), 1_000);

        let result = evaluate_touch(&FailingClock, &mut machine, &id("gmail"));
        assert_eq!(result, None);

        // The original baseline (1_000) from the direct `touch` above is
        // still in effect — the failed `evaluate_touch` call never moved
        // it — so a tick measured from that same baseline still goes
        // stale right on schedule.
        let s = statuses(&[("gmail", ServiceStatus::Loading)]);
        let actions = evaluate_tick(&FixedClock(1_000 + PAST_THRESHOLD_MS), &mut machine, &s);
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn evaluate_tick_runs_normally_when_the_clock_succeeds() {
        let mut machine = LivenessMachine::new();
        let s = statuses(&[("gmail", ServiceStatus::Loading)]);
        assert!(evaluate_tick(&FixedClock(1_000), &mut machine, &s).is_empty());
        let actions = evaluate_tick(&FixedClock(1_000 + PAST_THRESHOLD_MS), &mut machine, &s);
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn evaluate_touch_returns_the_clock_time_when_it_succeeds() {
        let mut machine = LivenessMachine::new();
        assert_eq!(
            evaluate_touch(&FixedClock(42), &mut machine, &id("gmail")),
            Some(42)
        );
    }

    // --- generation tagging / discard (finding 2) ------------------------

    #[test]
    fn action_service_id_extracts_the_id_from_every_variant() {
        assert_eq!(
            action_service_id(&Action::MarkStale { id: id("gmail") }),
            &id("gmail")
        );
        assert_eq!(
            action_service_id(&Action::Reload { id: id("gmail") }),
            &id("gmail")
        );
        assert_eq!(
            action_service_id(&Action::Recreate { id: id("gmail") }),
            &id("gmail")
        );
    }

    #[test]
    fn tag_actions_tags_each_action_with_its_services_current_generation() {
        let mut machine = LivenessMachine::new();
        machine.touch(&id("gmail"), 1_000);
        let generation = machine.generation(&id("gmail")).expect("tracked");

        let tagged = tag_actions(
            &machine,
            vec![
                Action::MarkStale { id: id("gmail") },
                Action::Reload { id: id("gmail") },
            ],
        );
        assert_eq!(
            tagged,
            vec![
                Tagged {
                    action: Action::MarkStale { id: id("gmail") },
                    generation,
                },
                Tagged {
                    action: Action::Reload { id: id("gmail") },
                    generation,
                },
            ]
        );
    }

    #[test]
    fn tag_actions_drops_an_action_for_a_service_no_longer_tracked() {
        // `machine` never registered "gmail" at all — standing in for the
        // (should-never-happen) case `tag_actions`'s own doc describes.
        let machine = LivenessMachine::new();
        let tagged = tag_actions(&machine, vec![Action::Reload { id: id("gmail") }]);
        assert!(tagged.is_empty());
    }

    #[test]
    fn is_current_true_when_the_generation_still_matches() {
        let tagged = Tagged {
            action: Action::Reload { id: id("gmail") },
            generation: 3,
        };
        assert!(is_current(&tagged, Some(3)));
    }

    #[test]
    fn is_current_false_when_a_touch_bumped_the_generation() {
        // A report arrived (bumping the generation) between `tick`
        // deciding this action and the runtime re-checking it — the exact
        // race `tag_actions`/`is_current` exist to catch.
        let tagged = Tagged {
            action: Action::Reload { id: id("gmail") },
            generation: 3,
        };
        assert!(!is_current(&tagged, Some(4)));
    }

    #[test]
    fn is_current_false_when_the_service_is_no_longer_tracked() {
        let tagged = Tagged {
            action: Action::Recreate { id: id("gmail") },
            generation: 0,
        };
        assert!(!is_current(&tagged, None));
    }

    #[test]
    fn stale_recovery_action_is_discarded_end_to_end_in_the_pure_layer() {
        // The scenario the runtime loop guards against, reproduced with
        // only the pure pieces: `tick` decides a `Recreate` for a service
        // that is still stale; before it would be applied, a report
        // arrives (`touch`, e.g. via `record_report`) and resets it. The
        // tagged action must then read as no longer current.
        let mut machine = LivenessMachine::new();
        let s = statuses(&[("gmail", ServiceStatus::Loading)]);
        machine.tick(1_000, &s);

        let t1 = 1_000 + PAST_THRESHOLD_MS;
        machine.tick(t1, &s); // MarkStale + Reload

        let stale = statuses(&[("gmail", ServiceStatus::Stale)]);
        machine.tick(t1 + 30_000, &stale); // one tick after the reload
        let actions = machine.tick(t1 + 60_000, &stale); // Recreate
        let tagged = tag_actions(&machine, actions);
        assert_eq!(tagged.len(), 1);

        // The service recovers right after `tick` decided to recreate it,
        // but before the runtime gets to apply that decision.
        machine.touch(&id("gmail"), t1 + 60_001);

        let current_generation = machine.generation(&id("gmail"));
        assert!(!is_current(&tagged[0], current_generation));
    }

    // --- monotonic `last_at` (finding 3) ----------------------------------

    #[test]
    fn is_newer_last_at_true_when_nothing_was_stored_yet() {
        assert!(is_newer_last_at(1_000, None));
    }

    #[test]
    fn is_newer_last_at_true_for_an_equal_or_later_time() {
        assert!(is_newer_last_at(1_000, Some(1_000)));
        assert!(is_newer_last_at(1_001, Some(1_000)));
    }

    #[test]
    fn is_newer_last_at_false_for_an_earlier_time() {
        assert!(!is_newer_last_at(999, Some(1_000)));
    }
}
