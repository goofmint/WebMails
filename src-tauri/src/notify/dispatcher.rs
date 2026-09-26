//! Turns a validated report's `notify::diff::evaluate` outcome into
//! notifications to send, and coordinates persisting newly-seen ids
//! alongside deciding what (if anything) to show (design.md §2.2.9).
//!
//! [`plan_notifications`] is the pure half: a [`DiffOutcome`] plus the
//! service's display name and the batching threshold becomes a
//! `Vec<OutgoingNotification>` — no I/O, no [`StateStore`], no sink.
//! [`Dispatcher`] is the stateful half: it holds each service's in-memory
//! [`ServiceNotifyState`] baseline (design.md §2.2.9: "held in memory
//! only for the lifetime of the running app") and the
//! `Arc<dyn NotificationSink>` notifications are actually sent through.
//!
//! [`Dispatcher::evaluate_and_persist`] and [`Dispatcher::send`] are
//! deliberately two separate calls, not one: `services::ServiceManager`
//! (this task's wiring, in `agent_bridge::report_unread`) runs the first
//! while its own lock is held — serializing report handling with every
//! other edit — and the second only after that lock has been released,
//! so a sink call (potentially slow — the OS's own notification center)
//! never blocks unrelated `ServiceManager` operations.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::agent_bridge::ValidReport;
use crate::config::ServiceId;
use crate::error::AppResult;
use crate::state::{SeenRing, StateStore};

use super::diff::{evaluate, DiffOutcome, ServiceNotifyState};
use super::seen;
use super::sink::{NotificationSink, OutgoingNotification};
use super::text;

/// Builds the notifications `outcome` implies, given `service_id`,
/// `service_name` and the batching threshold (design.md §2.2.9) — pure,
/// no I/O.
///
/// The caller ([`Dispatcher::send`]) is responsible for checking the
/// global and per-service notification toggles *before* calling this:
/// this function always plans whatever `outcome` implies, with no
/// knowledge of toggles at all.
///
/// - `Seed` and `Nothing` never produce a notification.
/// - `NewMessages` with more than `batch_threshold` new messages becomes
///   one batched, count-only notification; otherwise one rich
///   notification per new message, in report order.
/// - `CountIncrease` becomes one count-only notification.
pub fn plan_notifications(
    service_id: &ServiceId,
    service_name: &str,
    outcome: DiffOutcome,
    batch_threshold: u32,
) -> Vec<OutgoingNotification> {
    match outcome {
        DiffOutcome::Seed | DiffOutcome::Nothing => Vec::new(),
        DiffOutcome::NewMessages(new_refs) => {
            let count = new_refs.len() as u32;
            if count > batch_threshold {
                vec![count_notification(service_id, service_name, count)]
            } else {
                new_refs
                    .into_iter()
                    .map(|message| OutgoingNotification {
                        service: service_id.clone(),
                        title: text::title(service_name),
                        body: text::rich_body(message.from.as_deref(), message.subject.as_deref()),
                        link: message.link,
                    })
                    .collect()
            }
        }
        DiffOutcome::CountIncrease(delta) => {
            vec![count_notification(service_id, service_name, delta)]
        }
    }
}

/// A count-only notification (design.md §2.2.9: `"{name} — {n} new
/// messages"`, shared between batching and `CountIncrease` — the name
/// carried by the title, the count by the body; see `text`'s module
/// doc).
fn count_notification(service_id: &ServiceId, service_name: &str, n: u32) -> OutgoingNotification {
    OutgoingNotification {
        service: service_id.clone(),
        title: text::title(service_name),
        body: text::count_body(n),
        link: None,
    }
}

/// Persists whatever `outcome` says needs remembering into `ring`: every
/// message id in `report` on `Seed` (design.md §2.2.9 — a cold-start
/// baseline must not retroactively notify about messages already present
/// at launch), or just the ids `NewMessages` already narrowed down to.
/// `Nothing` and `CountIncrease` never carry an id to persist.
fn persist_seen_ids(ring: &mut SeenRing, outcome: &DiffOutcome, report: &ValidReport) {
    match outcome {
        DiffOutcome::Seed => {
            seen::insert_all(ring, report.messages().iter().map(|m| m.id.as_str()));
        }
        DiffOutcome::NewMessages(new_refs) => {
            seen::insert_all(ring, new_refs.iter().map(|m| m.id.as_str()));
        }
        DiffOutcome::Nothing | DiffOutcome::CountIncrease(_) => {}
    }
}

/// Holds every service's in-memory notification baseline and the sink
/// notifications are sent through (design.md §2.2.9).
pub struct Dispatcher {
    sink: Arc<dyn NotificationSink>,
    baselines: Mutex<BTreeMap<ServiceId, ServiceNotifyState>>,
}

impl Dispatcher {
    pub fn new(sink: Arc<dyn NotificationSink>) -> Self {
        Self {
            sink,
            baselines: Mutex::new(BTreeMap::new()),
        }
    }

    /// Runs [`evaluate`] for `id` against `report`, using (and updating)
    /// this dispatcher's in-memory baseline for `id`, and persists
    /// whatever ids that implies into `state`'s seen ring for `id`
    /// (design.md §2.2.9) — in one [`StateStore::update`] call, so the
    /// evaluation and the persistence it implies land in the same
    /// update, regardless of the notification toggles.
    ///
    /// Returns the resulting [`DiffOutcome`] so the caller can plan
    /// notifications from it once any lock it is holding (typically
    /// `ServiceManager::inner`) has been released — see this module's
    /// doc comment.
    pub fn evaluate_and_persist(
        &self,
        state: &StateStore,
        id: &ServiceId,
        report: &ValidReport,
    ) -> AppResult<DiffOutcome> {
        let mut baselines = match self.baselines.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let prev = baselines
            .entry(id.clone())
            .or_insert_with(ServiceNotifyState::new);
        state.update(|s| {
            let ring = s.seen.entry(id.clone()).or_default();
            let outcome = evaluate(prev, report, ring);
            persist_seen_ids(ring, &outcome, report);
            outcome
        })
    }

    /// Plans and sends whatever notifications `outcome` implies, only
    /// when both `settings_notifications` and `service_notifications`
    /// are true (design.md §2.2.9: "Notifications are sent only when
    /// `settings.notifications && service.notifications`").
    ///
    /// A sink failure is logged at warn level and does not stop the
    /// remaining notifications from being attempted (design.md §5.1:
    /// "Notification sink failure ... Unread state is unaffected") — that
    /// state already changed in [`Self::evaluate_and_persist`], before
    /// this is ever called, so a sink failure here cannot roll it back
    /// even if it wanted to.
    pub fn send(
        &self,
        id: &ServiceId,
        service_name: &str,
        settings_notifications: bool,
        service_notifications: bool,
        batch_threshold: u32,
        outcome: DiffOutcome,
    ) {
        if !settings_notifications || !service_notifications {
            return;
        }
        for notification in plan_notifications(id, service_name, outcome, batch_threshold) {
            if let Err(err) = self.sink.show(notification) {
                tracing::warn!(
                    service = %id,
                    error = %err,
                    "notification sink failed to show notification"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::tempdir;
    use url::Url;

    use crate::agent_bridge::{validate, MessageRefDto, UnreadReportDto};
    use crate::error::AppError;
    use crate::host;
    use crate::notify::diff::MessageRef;

    const SERVICE_ID: &str = "gmail-personal";
    const SERVICE_ORIGIN: &str = "https://mail.google.com";

    fn service_id() -> ServiceId {
        ServiceId::new(SERVICE_ID).expect("valid id")
    }

    fn make_report(count: Option<i64>, messages: Vec<MessageRefDto>) -> ValidReport {
        let dto = UnreadReportDto {
            service_id: SERVICE_ID.to_string(),
            count,
            messages,
            recipe_id: "gmail".to_string(),
            observed_at: 1_700_000_000_000,
            icon_candidates: Vec::new(),
        };
        let label = host::service_label(&service_id());
        let caller_url = Url::parse(SERVICE_ORIGIN).expect("valid url");
        let lookup = |_id: &ServiceId| Some(Url::parse(SERVICE_ORIGIN).expect("valid url"));
        validate(dto, &label, &caller_url, lookup).expect("should validate")
    }

    fn message(id: &str, from: Option<&str>, subject: Option<&str>) -> MessageRefDto {
        MessageRefDto {
            id: id.to_string(),
            from: from.map(str::to_string),
            subject: subject.map(str::to_string),
            link: None,
        }
    }

    /// Records every notification `show` is called with; optionally fails
    /// every call while still recording that it was attempted, so a test
    /// can assert the dispatcher keeps trying the rest after a failure.
    struct FakeSink {
        shown: Mutex<Vec<OutgoingNotification>>,
        attempts: Mutex<u32>,
        fail: bool,
    }

    impl FakeSink {
        fn new() -> Self {
            Self {
                shown: Mutex::new(Vec::new()),
                attempts: Mutex::new(0),
                fail: false,
            }
        }

        fn failing() -> Self {
            Self {
                shown: Mutex::new(Vec::new()),
                attempts: Mutex::new(0),
                fail: true,
            }
        }

        fn shown(&self) -> Vec<OutgoingNotification> {
            self.shown.lock().expect("lock").clone()
        }

        fn attempts(&self) -> u32 {
            *self.attempts.lock().expect("lock")
        }
    }

    impl NotificationSink for FakeSink {
        fn show(&self, notification: OutgoingNotification) -> Result<(), AppError> {
            *self.attempts.lock().expect("lock") += 1;
            if self.fail {
                return Err(AppError::Notification("fake sink failure".to_string()));
            }
            self.shown.lock().expect("lock").push(notification);
            Ok(())
        }
    }

    fn open_store() -> (tempfile::TempDir, StateStore) {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("state.json");
        let store = StateStore::open(path).expect("open");
        (dir, store)
    }

    // --- plan_notifications / text formatting --------------------------

    #[test]
    fn seed_and_nothing_plan_no_notifications() {
        let id = service_id();
        assert!(plan_notifications(&id, "Gmail", DiffOutcome::Seed, 5).is_empty());
        assert!(plan_notifications(&id, "Gmail", DiffOutcome::Nothing, 5).is_empty());
    }

    #[test]
    fn new_messages_under_threshold_produce_one_rich_notification_each() {
        let id = service_id();
        let refs = vec![
            MessageRef {
                id: "m1".to_string(),
                from: Some("Alice".to_string()),
                subject: Some("Hi".to_string()),
                link: None,
            },
            MessageRef {
                id: "m2".to_string(),
                from: None,
                subject: None,
                link: None,
            },
        ];
        let notifications = plan_notifications(&id, "Gmail", DiffOutcome::NewMessages(refs), 5);
        assert_eq!(notifications.len(), 2);
        assert_eq!(notifications[0].title, "Gmail");
        assert_eq!(notifications[0].body, "Alice — Hi");
        assert_eq!(notifications[1].body, "New message");
    }

    #[test]
    fn count_increase_produces_one_count_notification() {
        let id = service_id();
        let notifications = plan_notifications(&id, "Gmail", DiffOutcome::CountIncrease(4), 5);
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].title, "Gmail");
        assert_eq!(notifications[0].body, "4 new messages");
    }

    #[test]
    fn batching_threshold_boundary_equal_count_is_not_batched() {
        let id = service_id();
        let refs: Vec<_> = (0..3)
            .map(|i| MessageRef {
                id: format!("m{i}"),
                from: None,
                subject: None,
                link: None,
            })
            .collect();
        // Exactly at the threshold: still one notification per message.
        let notifications = plan_notifications(&id, "Gmail", DiffOutcome::NewMessages(refs), 3);
        assert_eq!(notifications.len(), 3);
    }

    #[test]
    fn batching_threshold_boundary_over_count_is_batched() {
        let id = service_id();
        let refs: Vec<_> = (0..4)
            .map(|i| MessageRef {
                id: format!("m{i}"),
                from: None,
                subject: None,
                link: None,
            })
            .collect();
        // One over the threshold: batched into a single count notification.
        let notifications = plan_notifications(&id, "Gmail", DiffOutcome::NewMessages(refs), 3);
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].body, "4 new messages");
    }

    // --- Dispatcher: toggles ---------------------------------------------

    #[test]
    fn both_toggles_off_sends_nothing() {
        let sink = Arc::new(FakeSink::new());
        let dispatcher = Dispatcher::new(sink.clone());
        let id = service_id();

        dispatcher.send(&id, "Gmail", false, false, 5, DiffOutcome::CountIncrease(3));
        assert!(sink.shown().is_empty());
    }

    #[test]
    fn global_toggle_off_sends_nothing_even_if_service_toggle_is_on() {
        let sink = Arc::new(FakeSink::new());
        let dispatcher = Dispatcher::new(sink.clone());
        let id = service_id();

        dispatcher.send(&id, "Gmail", false, true, 5, DiffOutcome::CountIncrease(3));
        assert!(sink.shown().is_empty());
    }

    #[test]
    fn service_toggle_off_sends_nothing_even_if_global_toggle_is_on() {
        let sink = Arc::new(FakeSink::new());
        let dispatcher = Dispatcher::new(sink.clone());
        let id = service_id();

        dispatcher.send(&id, "Gmail", true, false, 5, DiffOutcome::CountIncrease(3));
        assert!(sink.shown().is_empty());
    }

    #[test]
    fn both_toggles_on_sends_the_planned_notification() {
        let sink = Arc::new(FakeSink::new());
        let dispatcher = Dispatcher::new(sink.clone());
        let id = service_id();

        dispatcher.send(&id, "Gmail", true, true, 5, DiffOutcome::CountIncrease(3));
        let shown = sink.shown();
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].body, "3 new messages");
    }

    // --- Dispatcher: evaluate_and_persist / seeding ----------------------

    #[test]
    fn seed_records_ids_but_sends_nothing() {
        let (_dir, state) = open_store();
        let sink = Arc::new(FakeSink::new());
        let dispatcher = Dispatcher::new(sink.clone());
        let id = service_id();

        let report = make_report(
            Some(2),
            vec![message("m1", None, None), message("m2", None, None)],
        );
        let outcome = dispatcher
            .evaluate_and_persist(&state, &id, &report)
            .expect("evaluate");
        assert_eq!(outcome, DiffOutcome::Seed);

        dispatcher.send(&id, "Gmail", true, true, 5, outcome);
        assert!(sink.shown().is_empty());

        state.flush().expect("flush");
        let ring = state.read(|s| s.seen.get(&id).cloned()).expect("read");
        let ring = ring.expect("ring recorded for seeded service");
        assert!(seen::contains(&ring, "m1"));
        assert!(seen::contains(&ring, "m2"));
    }

    #[test]
    fn new_messages_send_rich_notifications_and_persist_only_new_ids() {
        let (_dir, state) = open_store();
        let sink = Arc::new(FakeSink::new());
        let dispatcher = Dispatcher::new(sink.clone());
        let id = service_id();

        // Seed first (cold start), with "m1" already present.
        let seed_report = make_report(Some(1), vec![message("m1", None, None)]);
        dispatcher
            .evaluate_and_persist(&state, &id, &seed_report)
            .expect("seed");

        // A later report brings a new message alongside the already-seen one.
        let report = make_report(
            Some(2),
            vec![
                message("m1", None, None),
                message("m2", Some("Bob"), Some("Re: hi")),
            ],
        );
        let outcome = dispatcher
            .evaluate_and_persist(&state, &id, &report)
            .expect("evaluate");
        match &outcome {
            DiffOutcome::NewMessages(refs) => {
                assert_eq!(refs.len(), 1);
                assert_eq!(refs[0].id, "m2");
            }
            other => panic!("expected NewMessages, got {other:?}"),
        }

        dispatcher.send(&id, "Gmail", true, true, 5, outcome);
        let shown = sink.shown();
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].title, "Gmail");
        assert_eq!(shown[0].body, "Bob — Re: hi");

        state.flush().expect("flush");
        let ring = state
            .read(|s| s.seen.get(&id).cloned())
            .expect("read")
            .expect("ring present");
        assert!(seen::contains(&ring, "m1"));
        assert!(seen::contains(&ring, "m2"));
    }

    #[test]
    fn count_increase_sends_count_text() {
        let (_dir, state) = open_store();
        let sink = Arc::new(FakeSink::new());
        let dispatcher = Dispatcher::new(sink.clone());
        let id = service_id();

        dispatcher
            .evaluate_and_persist(&state, &id, &make_report(Some(2), Vec::new()))
            .expect("seed");
        let outcome = dispatcher
            .evaluate_and_persist(&state, &id, &make_report(Some(9), Vec::new()))
            .expect("evaluate");
        assert_eq!(outcome, DiffOutcome::CountIncrease(7));

        dispatcher.send(&id, "Gmail", true, true, 5, outcome);
        let shown = sink.shown();
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].body, "7 new messages");
    }

    #[test]
    fn toggling_off_then_on_still_reflects_the_continuous_baseline() {
        let (_dir, state) = open_store();
        let sink = Arc::new(FakeSink::new());
        let dispatcher = Dispatcher::new(sink.clone());
        let id = service_id();

        // Seed while notifications are off — the baseline still advances.
        let seed_outcome = dispatcher
            .evaluate_and_persist(&state, &id, &make_report(Some(3), Vec::new()))
            .expect("seed");
        dispatcher.send(&id, "Gmail", false, true, 5, seed_outcome);
        assert!(sink.shown().is_empty());

        // Re-enabled: the next report is judged against the seeded
        // baseline, not treated as a fresh Seed.
        let outcome = dispatcher
            .evaluate_and_persist(&state, &id, &make_report(Some(6), Vec::new()))
            .expect("evaluate");
        assert_eq!(outcome, DiffOutcome::CountIncrease(3));
        dispatcher.send(&id, "Gmail", true, true, 5, outcome);
        assert_eq!(sink.shown().len(), 1);
    }

    // --- Dispatcher: sink failure -----------------------------------------

    #[test]
    fn sink_failure_is_logged_and_does_not_stop_remaining_notifications() {
        let sink = Arc::new(FakeSink::failing());
        let dispatcher = Dispatcher::new(sink.clone());
        let id = service_id();

        let refs: Vec<_> = (0..3)
            .map(|i| MessageRef {
                id: format!("m{i}"),
                from: None,
                subject: None,
                link: None,
            })
            .collect();

        dispatcher.send(&id, "Gmail", true, true, 10, DiffOutcome::NewMessages(refs));

        // Every planned notification was attempted despite each one
        // failing — the loop does not stop after the first failure.
        assert_eq!(sink.attempts(), 3);
        assert!(sink.shown().is_empty());
    }
}
