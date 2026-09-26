//! The pure liveness state machine (design.md §2.2.8, SPEC.md §9.4): given
//! the current time and every service's [`ServiceStatus`], decides which
//! services have gone silent and what to do about it, without performing
//! any of those actions itself.
//!
//! No Tauri, no I/O, no wall-clock reads: every method takes `now_ms`
//! explicitly (an injected "clock" in the form the task allows — see
//! [`super::Clock`] for the thin wrapper the runtime uses to supply it in
//! production), which is what makes [`LivenessMachine::tick`] and
//! [`LivenessMachine::touch`] deterministic and unit-testable below
//! without any real waiting.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::config::ServiceId;
use crate::unread::{AttentionReason, ServiceStatus};

/// The heartbeat cadence (design.md §2.2.8): every agent re-posts its
/// latest report every 30s, and the runtime's tokio interval ticks this
/// often.
pub const TICK: Duration = Duration::from_secs(30);

/// Extra slack added on top of the missed-reports threshold before a
/// service is declared stale (design.md §2.2.8's "+ 5s grace").
pub const GRACE: Duration = Duration::from_secs(5);

/// How many consecutive missed reports (i.e. elapsed ticks with no report)
/// mark a service stale (design.md §2.2.8, SPEC.md §9.4: "two consecutive
/// missed reports").
pub const MISSED_REPORTS_THRESHOLD: u32 = 2;

/// How many ticks a reloaded service is given to report again before it is
/// destroyed and recreated (design.md §2.2.8: "Still `Stale` two ticks
/// after the reload").
pub const TICKS_AFTER_RELOAD: u32 = 2;

const TICK_MS: u64 = TICK.as_millis() as u64;
const GRACE_MS: u64 = GRACE.as_millis() as u64;

/// The staleness threshold in milliseconds: `now - last_report >
/// MISSED_REPORTS_THRESHOLD * TICK + GRACE` (design.md §2.2.8), i.e. 65s
/// with the constants above.
const STALE_THRESHOLD_MS: u64 = MISSED_REPORTS_THRESHOLD as u64 * TICK_MS + GRACE_MS;

/// An action [`LivenessMachine::tick`] wants applied. The machine never
/// applies these itself — [`super::runtime::LivenessRuntime`] is the only
/// place that turns one into a `StatusStore`/`ServiceManager`/`StateStore`
/// call, so this module stays pure and Tauri-free.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// The service has missed too many reports: move its status to
    /// `Stale`, record a staleness episode, and reload its webview.
    MarkStale { id: ServiceId },
    /// Reload the service's webview (always paired with the `MarkStale`
    /// that triggered it, in the same tick).
    Reload { id: ServiceId },
    /// The service is still stale two ticks after the reload: destroy and
    /// recreate its webview (design.md §2.2.8).
    Recreate { id: ServiceId },
}

/// A tracked service's liveness phase.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Phase {
    /// Waiting for a report; goes stale if `now - last_report_ms` exceeds
    /// [`STALE_THRESHOLD_MS`] (subject to the exemptions in
    /// [`LivenessMachine::tick`]).
    Monitoring,
    /// Already reloaded once; counts ticks until [`TICKS_AFTER_RELOAD`] is
    /// reached, at which point a `Recreate` action is emitted and the
    /// service returns to `Monitoring` with a fresh baseline (design.md
    /// §2.2.8: "recreate never removes the old profile's on-disk data",
    /// i.e. the page just reloads from its session, so a fresh window
    /// starts here too). If `status` becomes exempt while counting down
    /// (e.g. the reload landed the page off-origin), the countdown itself
    /// is abandoned: exempt means "not this machine's problem to fix",
    /// which the `Reloaded` phase's whole reason for counting — deciding
    /// whether to recreate — no longer applies to.
    Reloaded { ticks_since_reload: u32 },
}

/// One tracked service's liveness bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ServiceLiveness {
    /// The time (ms) [`LivenessMachine`] last considered this service
    /// alive: a report, its first sighting, or its most recent recreation
    /// (design.md §2.2.6: "Liveness uses the time Rust receives the
    /// report").
    last_report_ms: u64,
    phase: Phase,
    /// Bumped only by [`LivenessMachine::touch`] (an external reset — a
    /// report arriving, design.md §2.2.6) — never by `tick`'s own
    /// internal phase transitions. The runtime tags each action `tick`
    /// returns with this value at the moment it was decided, and
    /// re-checks it (via [`LivenessMachine::generation`]) right before
    /// applying that action: if a report reset the service in between,
    /// the generation no longer matches and the runtime discards the
    /// now-stale action rather than reloading/recreating a service that
    /// has already recovered.
    generation: u64,
}

impl ServiceLiveness {
    fn fresh(now_ms: u64) -> Self {
        ServiceLiveness {
            last_report_ms: now_ms,
            phase: Phase::Monitoring,
            generation: 0,
        }
    }
}

/// Whether `status` is exempt from staleness evaluation (design.md
/// §2.2.8): `NeedsAttention(OffOrigin)`, because the agent is not
/// permitted to report there, and `NeedsAttention(CreateFailed)`, because
/// there is no webview to reload — the task's own instruction, matching
/// how `OffOrigin` is already exempted. Nothing invents a retry for
/// either case. Checked in both [`Phase::Monitoring`] (skip the
/// staleness check) and [`Phase::Reloaded`] (abandon the recreate
/// countdown).
fn is_exempt(status: &ServiceStatus) -> bool {
    matches!(
        status,
        ServiceStatus::NeedsAttention {
            reason: AttentionReason::OffOrigin | AttentionReason::CreateFailed
        }
    )
}

/// The pure per-service liveness state machine (module doc has the full
/// picture). Owns no clock, no `Tauri` handle, no I/O: every mutation
/// takes `now_ms` explicitly and every decision comes back as a plain
/// [`Action`] list for the caller to apply.
#[derive(Debug, Default)]
pub struct LivenessMachine {
    services: BTreeMap<ServiceId, ServiceLiveness>,
}

impl LivenessMachine {
    pub fn new() -> Self {
        LivenessMachine {
            services: BTreeMap::new(),
        }
    }

    /// Records that `id` is alive as of `now_ms`, resetting it to
    /// [`Phase::Monitoring`] with a fresh baseline: called on every
    /// validated report (design.md §2.2.6) and, by
    /// [`super::runtime::LivenessRuntime`], right after a service is
    /// (re)created, so a fresh webview starts its own staleness window
    /// rather than inheriting a stale one. Registers `id` if it was not
    /// already tracked.
    pub fn touch(&mut self, id: &ServiceId, now_ms: u64) {
        // A report never moves the baseline backwards (e.g. a clock step
        // back, or two reports processed out of order).
        let (generation, report_ms) = match self.services.get(id) {
            Some(existing) => (
                existing.generation.wrapping_add(1),
                now_ms.max(existing.last_report_ms),
            ),
            None => (0, now_ms),
        };
        self.services.insert(
            id.clone(),
            ServiceLiveness {
                generation,
                ..ServiceLiveness::fresh(report_ms)
            },
        );
    }

    /// Stops tracking `id` (no further `tick` evaluates it until it is
    /// `touch`ed or reappears in a `tick`'s `statuses`). Also invalidates
    /// any action already tagged with `id`'s generation: [`Self::
    /// generation`] returns `None` for an untracked id, which never
    /// equals a previously tagged `Some(_)`.
    pub fn remove(&mut self, id: &ServiceId) {
        self.services.remove(id);
    }

    /// `id`'s current generation counter (`None` if untracked) — see
    /// [`ServiceLiveness::generation`]'s doc for what the runtime uses
    /// this for.
    pub fn generation(&self, id: &ServiceId) -> Option<u64> {
        self.services.get(id).map(|liveness| liveness.generation)
    }

    /// Evaluates every service named in `statuses` at `now_ms` and returns
    /// the actions the caller should apply, in service-id order
    /// (deterministic, for tests and for stable logging).
    ///
    /// Also reconciles the tracked set against `statuses`' keys: a
    /// service seen for the first time is registered with `now_ms` as its
    /// baseline (design.md's own sanctioned alternative to a precise
    /// creation hook — see this module's doc), and a previously tracked
    /// service absent from `statuses` (removed, e.g. `StatusStore::remove`
    /// after a successful `host.destroy`) is dropped.
    pub fn tick(
        &mut self,
        now_ms: u64,
        statuses: &std::collections::HashMap<ServiceId, ServiceStatus>,
    ) -> Vec<Action> {
        self.services.retain(|id, _| statuses.contains_key(id));
        for id in statuses.keys() {
            self.services
                .entry(id.clone())
                .or_insert_with(|| ServiceLiveness::fresh(now_ms));
        }

        let mut actions = Vec::new();
        for (id, liveness) in self.services.iter_mut() {
            // `statuses.contains_key(id)` for every tracked id, by the
            // sync above.
            let Some(status) = statuses.get(id) else {
                continue;
            };
            match &mut liveness.phase {
                Phase::Monitoring => {
                    if is_exempt(status) || matches!(status, ServiceStatus::Stale) {
                        continue;
                    }
                    let elapsed = now_ms.saturating_sub(liveness.last_report_ms);
                    if elapsed > STALE_THRESHOLD_MS {
                        actions.push(Action::MarkStale { id: id.clone() });
                        actions.push(Action::Reload { id: id.clone() });
                        liveness.phase = Phase::Reloaded {
                            ticks_since_reload: 0,
                        };
                    }
                }
                Phase::Reloaded { ticks_since_reload } => {
                    if is_exempt(status) {
                        // The reload landed the service somewhere this
                        // machine cannot help with (design.md §2.2.8's
                        // exemptions) — abandon the recreate countdown
                        // rather than escalating a case that isn't a
                        // liveness problem, and start a fresh window from
                        // here in case it later becomes eligible again.
                        liveness.last_report_ms = now_ms;
                        liveness.phase = Phase::Monitoring;
                        continue;
                    }
                    *ticks_since_reload += 1;
                    if *ticks_since_reload >= TICKS_AFTER_RELOAD {
                        actions.push(Action::Recreate { id: id.clone() });
                        liveness.last_report_ms = now_ms;
                        liveness.phase = Phase::Monitoring;
                    }
                }
            }
        }
        actions
    }

    #[cfg(test)]
    fn is_tracked(&self, id: &ServiceId) -> bool {
        self.services.contains_key(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn id(value: &str) -> ServiceId {
        ServiceId::new(value).expect("valid id")
    }

    fn statuses(pairs: &[(&str, ServiceStatus)]) -> HashMap<ServiceId, ServiceStatus> {
        pairs.iter().map(|(k, v)| (id(k), v.clone())).collect()
    }

    const BASE: u64 = 1_000_000;

    #[test]
    fn threshold_constant_is_sixty_five_seconds() {
        assert_eq!(STALE_THRESHOLD_MS, 65_000);
    }

    #[test]
    fn a_service_first_seen_this_tick_is_not_marked_stale() {
        let mut machine = LivenessMachine::new();
        let s = statuses(&[("gmail", ServiceStatus::Loading)]);
        let actions = machine.tick(BASE, &s);
        assert!(actions.is_empty());
        assert!(machine.is_tracked(&id("gmail")));
    }

    #[test]
    fn exactly_at_the_grace_boundary_is_not_yet_stale() {
        let mut machine = LivenessMachine::new();
        let s = statuses(&[("gmail", ServiceStatus::Loading)]);
        machine.tick(BASE, &s); // registers at BASE

        // now - last_report == 65_000ms exactly: the rule is "> 65s", so
        // this must NOT be stale yet.
        let actions = machine.tick(BASE + STALE_THRESHOLD_MS, &s);
        assert!(actions.is_empty());
    }

    #[test]
    fn one_millisecond_past_the_grace_boundary_is_stale() {
        let mut machine = LivenessMachine::new();
        let s = statuses(&[("gmail", ServiceStatus::Loading)]);
        machine.tick(BASE, &s);

        let actions = machine.tick(BASE + STALE_THRESHOLD_MS + 1, &s);
        assert_eq!(
            actions,
            vec![
                Action::MarkStale { id: id("gmail") },
                Action::Reload { id: id("gmail") },
            ]
        );
    }

    #[test]
    fn ok_status_also_goes_stale_past_the_threshold() {
        let mut machine = LivenessMachine::new();
        let s = statuses(&[("gmail", ServiceStatus::Ok { count: 3 })]);
        machine.tick(BASE, &s);
        let actions = machine.tick(BASE + STALE_THRESHOLD_MS + 1, &s);
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn needs_attention_reported_none_also_goes_stale_past_the_threshold() {
        let mut machine = LivenessMachine::new();
        let s = statuses(&[(
            "gmail",
            ServiceStatus::NeedsAttention {
                reason: AttentionReason::ReportedNone,
            },
        )]);
        machine.tick(BASE, &s);
        let actions = machine.tick(BASE + STALE_THRESHOLD_MS + 1, &s);
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn off_origin_is_exempt_forever() {
        let mut machine = LivenessMachine::new();
        let s = statuses(&[(
            "gmail",
            ServiceStatus::NeedsAttention {
                reason: AttentionReason::OffOrigin,
            },
        )]);
        machine.tick(BASE, &s);
        let actions = machine.tick(BASE + STALE_THRESHOLD_MS * 100, &s);
        assert!(actions.is_empty());
    }

    #[test]
    fn create_failed_is_exempt_forever() {
        let mut machine = LivenessMachine::new();
        let s = statuses(&[(
            "gmail",
            ServiceStatus::NeedsAttention {
                reason: AttentionReason::CreateFailed,
            },
        )]);
        machine.tick(BASE, &s);
        let actions = machine.tick(BASE + STALE_THRESHOLD_MS * 100, &s);
        assert!(actions.is_empty());
    }

    #[test]
    fn already_stale_status_does_not_emit_mark_stale_again() {
        // Once the store's status is `Stale`, a service still in
        // `Monitoring` (e.g. re-synced after a restart) must not be
        // re-marked; in practice this phase/status combination only
        // arises defensively, but the guard must hold regardless.
        let mut machine = LivenessMachine::new();
        let s = statuses(&[("gmail", ServiceStatus::Stale)]);
        machine.tick(BASE, &s);
        let actions = machine.tick(BASE + STALE_THRESHOLD_MS * 100, &s);
        assert!(actions.is_empty());
    }

    #[test]
    fn reload_then_recreate_after_two_more_ticks() {
        let mut machine = LivenessMachine::new();
        let s = statuses(&[("gmail", ServiceStatus::Loading)]);
        machine.tick(BASE, &s);

        // Crosses the threshold: MarkStale + Reload.
        let t1 = BASE + STALE_THRESHOLD_MS + 1;
        let actions = machine.tick(t1, &s);
        assert_eq!(
            actions,
            vec![
                Action::MarkStale { id: id("gmail") },
                Action::Reload { id: id("gmail") },
            ]
        );

        // Now Stale in the store; one tick after the reload: nothing yet.
        let stale = statuses(&[("gmail", ServiceStatus::Stale)]);
        let t2 = t1 + TICK_MS;
        let actions = machine.tick(t2, &stale);
        assert!(actions.is_empty());

        // Two ticks after the reload: Recreate.
        let t3 = t2 + TICK_MS;
        let actions = machine.tick(t3, &stale);
        assert_eq!(actions, vec![Action::Recreate { id: id("gmail") }]);

        // No further Recreate on the next tick, still Stale in the store:
        // the machine has moved back to `Monitoring` with a fresh
        // baseline, so nothing fires until the threshold elapses again.
        let t4 = t3 + TICK_MS;
        let actions = machine.tick(t4, &stale);
        assert!(actions.is_empty());
    }

    #[test]
    fn off_origin_during_reloaded_abandons_the_countdown_and_never_recreates() {
        let mut machine = LivenessMachine::new();
        let s = statuses(&[("gmail", ServiceStatus::Loading)]);
        machine.tick(BASE, &s);

        // Crosses the threshold: MarkStale + Reload, phase -> Reloaded{0}.
        let t1 = BASE + STALE_THRESHOLD_MS + 1;
        assert_eq!(machine.tick(t1, &s).len(), 2);

        // The reload lands the page off-origin before the agent can
        // report again (design.md §2.2.8's exemption applies here too).
        let off_origin = statuses(&[(
            "gmail",
            ServiceStatus::NeedsAttention {
                reason: AttentionReason::OffOrigin,
            },
        )]);
        let t2 = t1 + TICK_MS;
        assert!(machine.tick(t2, &off_origin).is_empty());

        // Even after two more ticks — the point at which a Recreate would
        // otherwise have fired — nothing happens: the countdown was
        // abandoned, not merely paused.
        let t3 = t2 + TICK_MS;
        assert!(machine.tick(t3, &off_origin).is_empty());
        let t4 = t3 + TICK_MS;
        assert!(machine.tick(t4, &off_origin).is_empty());
    }

    #[test]
    fn create_failed_during_reloaded_abandons_the_countdown_and_never_recreates() {
        let mut machine = LivenessMachine::new();
        let s = statuses(&[("gmail", ServiceStatus::Loading)]);
        machine.tick(BASE, &s);

        let t1 = BASE + STALE_THRESHOLD_MS + 1;
        assert_eq!(machine.tick(t1, &s).len(), 2);

        let create_failed = statuses(&[(
            "gmail",
            ServiceStatus::NeedsAttention {
                reason: AttentionReason::CreateFailed,
            },
        )]);
        let t2 = t1 + TICK_MS;
        assert!(machine.tick(t2, &create_failed).is_empty());
        let t3 = t2 + TICK_MS;
        assert!(machine.tick(t3, &create_failed).is_empty());
    }

    #[test]
    fn a_report_arriving_mid_episode_resets_everything() {
        let mut machine = LivenessMachine::new();
        let s = statuses(&[("gmail", ServiceStatus::Loading)]);
        machine.tick(BASE, &s);

        let t1 = BASE + STALE_THRESHOLD_MS + 1;
        let actions = machine.tick(t1, &s);
        assert_eq!(actions.len(), 2); // MarkStale + Reload

        // A report arrives one tick later (mid `Reloaded` countdown).
        let report_time = t1 + TICK_MS;
        machine.touch(&id("gmail"), report_time);

        // Even long after, nothing more fires until the threshold elapses
        // again from `report_time`.
        let ok = statuses(&[("gmail", ServiceStatus::Ok { count: 1 })]);
        let just_under = report_time + STALE_THRESHOLD_MS;
        assert!(machine.tick(just_under, &ok).is_empty());

        let just_over = report_time + STALE_THRESHOLD_MS + 1;
        let actions = machine.tick(just_over, &ok);
        assert_eq!(actions.len(), 2); // a fresh episode, not a leftover one
    }

    #[test]
    fn staleness_counter_increments_once_per_stale_episode() {
        // The machine itself does not hold a counter (that lives in
        // `state.json`, updated by the runtime once per `MarkStale`
        // action) — this test asserts the machine's contribution to that
        // invariant: exactly one `MarkStale` per episode, never repeated
        // while still `Reloaded`, and exactly one more if a second
        // episode happens after recovery.
        let mut machine = LivenessMachine::new();
        let s = statuses(&[("gmail", ServiceStatus::Loading)]);
        machine.tick(BASE, &s);

        let t1 = BASE + STALE_THRESHOLD_MS + 1;
        let mark_stale_count = |actions: &[Action]| {
            actions
                .iter()
                .filter(|a| matches!(a, Action::MarkStale { .. }))
                .count()
        };
        assert_eq!(mark_stale_count(&machine.tick(t1, &s)), 1);

        let stale = statuses(&[("gmail", ServiceStatus::Stale)]);
        assert_eq!(mark_stale_count(&machine.tick(t1 + TICK_MS, &stale)), 0);
        assert_eq!(mark_stale_count(&machine.tick(t1 + 2 * TICK_MS, &stale)), 0);

        // Second episode, after recovery via `touch`.
        let recovered_at = t1 + 2 * TICK_MS;
        machine.touch(&id("gmail"), recovered_at);
        let ok = statuses(&[("gmail", ServiceStatus::Ok { count: 1 })]);
        let t2 = recovered_at + STALE_THRESHOLD_MS + 1;
        assert_eq!(mark_stale_count(&machine.tick(t2, &ok)), 1);
    }

    #[test]
    fn remove_stops_tracking_a_service() {
        let mut machine = LivenessMachine::new();
        let s = statuses(&[("gmail", ServiceStatus::Loading)]);
        machine.tick(BASE, &s);
        assert!(machine.is_tracked(&id("gmail")));

        machine.remove(&id("gmail"));
        assert!(!machine.is_tracked(&id("gmail")));
    }

    #[test]
    fn a_service_no_longer_in_statuses_is_dropped_by_tick() {
        let mut machine = LivenessMachine::new();
        let s = statuses(&[("gmail", ServiceStatus::Loading)]);
        machine.tick(BASE, &s);
        assert!(machine.is_tracked(&id("gmail")));

        let empty = HashMap::new();
        machine.tick(BASE + 1, &empty);
        assert!(!machine.is_tracked(&id("gmail")));
    }

    #[test]
    fn touch_registers_a_previously_unknown_service() {
        let mut machine = LivenessMachine::new();
        machine.touch(&id("gmail"), BASE);
        assert!(machine.is_tracked(&id("gmail")));
    }

    #[test]
    fn generation_is_none_for_an_untracked_service() {
        let machine = LivenessMachine::new();
        assert_eq!(machine.generation(&id("gmail")), None);
    }

    #[test]
    fn touch_bumps_the_generation_each_time() {
        let mut machine = LivenessMachine::new();
        machine.touch(&id("gmail"), BASE);
        let g0 = machine.generation(&id("gmail")).expect("tracked");

        machine.touch(&id("gmail"), BASE + 1);
        let g1 = machine.generation(&id("gmail")).expect("tracked");
        assert_ne!(g0, g1);

        machine.touch(&id("gmail"), BASE + 2);
        let g2 = machine.generation(&id("gmail")).expect("tracked");
        assert_ne!(g1, g2);
    }

    #[test]
    fn tick_does_not_bump_the_generation_for_its_own_transitions() {
        // Only an external reset (`touch`) changes the generation —
        // `tick`'s own internal transitions (crossing the threshold,
        // reloading, recreating) must not, or every in-flight action the
        // very tick that decided them produced would already look stale
        // by the time the runtime re-checks it.
        let mut machine = LivenessMachine::new();
        let s = statuses(&[("gmail", ServiceStatus::Loading)]);
        machine.tick(BASE, &s);
        let g0 = machine.generation(&id("gmail")).expect("tracked");

        let t1 = BASE + STALE_THRESHOLD_MS + 1;
        machine.tick(t1, &s); // MarkStale + Reload
        assert_eq!(machine.generation(&id("gmail")), Some(g0));

        let stale = statuses(&[("gmail", ServiceStatus::Stale)]);
        machine.tick(t1 + TICK_MS, &stale);
        machine.tick(t1 + 2 * TICK_MS, &stale); // Recreate
        assert_eq!(machine.generation(&id("gmail")), Some(g0));
    }

    #[test]
    fn remove_makes_the_generation_none_again() {
        let mut machine = LivenessMachine::new();
        machine.touch(&id("gmail"), BASE);
        assert!(machine.generation(&id("gmail")).is_some());

        machine.remove(&id("gmail"));
        assert_eq!(machine.generation(&id("gmail")), None);
    }

    #[test]
    fn multiple_services_are_evaluated_independently() {
        let mut machine = LivenessMachine::new();
        let s = statuses(&[
            ("gmail", ServiceStatus::Loading),
            ("icloud", ServiceStatus::Loading),
        ]);
        machine.tick(BASE, &s);

        // Only `icloud` gets a fresh report.
        machine.touch(&id("icloud"), BASE + TICK_MS);

        let t1 = BASE + STALE_THRESHOLD_MS + 1;
        let actions = machine.tick(t1, &s);
        assert_eq!(
            actions,
            vec![
                Action::MarkStale { id: id("gmail") },
                Action::Reload { id: id("gmail") },
            ]
        );
    }

    #[test]
    fn touch_never_moves_the_last_report_time_backwards() {
        let mut machine = LivenessMachine::new();
        let id = ServiceId::new("svc-1").expect("valid id");
        machine.touch(&id, 100_000);
        machine.touch(&id, 50_000);
        // Still measured from 100_000: at 100_000 + 65_000 it is not yet stale.
        let statuses = std::collections::HashMap::from([(
            id.clone(),
            crate::unread::ServiceStatus::Ok { count: 1 },
        )]);
        assert!(machine.tick(165_000, &statuses).is_empty());
    }
}
