//! Pure diff engine over a validated report (design.md §2.2.9):
//! [`evaluate`] decides what, if anything, a service's latest
//! [`ValidReport`] implies for notification purposes, given the
//! in-memory [`ServiceNotifyState`] baseline for that service and the
//! persisted [`SeenRing`] snapshot.
//!
//! [`evaluate`] never touches Tauri or `StateStore`, and never mutates
//! `seen` — it only reads it to decide which message ids are new. Acting
//! on a [`DiffOutcome`] (inserting newly-seen ids into the persisted
//! ring, sending a notification) is task 4.5's job.

use std::collections::HashSet;

use url::Url;

use crate::agent_bridge::{ValidMessageRef, ValidReport};
use crate::state::SeenRing;

use super::seen;

/// Per-service notification baseline, held in memory only for the
/// lifetime of the running app (design.md §2.2.9): `None` until the
/// first report with a non-null `count` has been seen since launch, then
/// that report's (and every later report's) count.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServiceNotifyState {
    last_count: Option<u32>,
}

impl ServiceNotifyState {
    /// A fresh baseline, as at app launch: no report seen yet, so the
    /// next `Some` report will seed it.
    pub fn new() -> Self {
        Self::default()
    }

    /// The count [`evaluate`] last recorded for this service, or `None`
    /// if no report with a non-null count has been seen since launch.
    ///
    /// No production caller yet outside this module's own tests — a
    /// future diagnostics view (design.md §2.2.12's `get_diagnostics`) is
    /// a plausible one — so this stays `#[allow(dead_code)]` rather than
    /// fabricating a reader that doesn't exist (same pattern as
    /// `agent_bridge::validate::ValidReport`'s still-unread fields).
    #[allow(dead_code)]
    pub fn last_count(&self) -> Option<u32> {
        self.last_count
    }
}

/// A copy of a new message worth notifying about, taken out of a
/// [`ValidReport`]'s messages so the caller does not need to keep the
/// report itself alive (design.md §2.2.9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageRef {
    pub id: String,
    pub from: Option<String>,
    pub subject: Option<String>,
    pub link: Option<Url>,
}

impl From<&ValidMessageRef> for MessageRef {
    fn from(message: &ValidMessageRef) -> Self {
        MessageRef {
            id: message.id.clone(),
            from: message.from.clone(),
            subject: message.subject.clone(),
            link: message.link.clone(),
        }
    }
}

/// What a report implies for notification purposes (design.md §2.2.9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffOutcome {
    /// The first `Some(count)` report for this service since launch.
    /// The baseline is recorded but nothing is notified (cold start).
    Seed,
    /// Nothing worth notifying about: `count = None`, message ids that
    /// are all already `seen`, or a count that decreased or stayed the
    /// same.
    Nothing,
    /// `messages` contained at least one id not already in `seen`,
    /// deduplicated and kept in report order.
    NewMessages(Vec<MessageRef>),
    /// No `messages`, but `count` increased over the last recorded
    /// count, by this many.
    CountIncrease(u32),
}

/// Decides `report`'s [`DiffOutcome`] against `prev` and `seen`
/// (design.md §2.2.9), in this order:
///
/// 1. `report.count() == None` → [`DiffOutcome::Nothing`]; `prev` is
///    left untouched.
/// 2. `prev` has no recorded count yet (the first `Some` report since
///    launch) → `prev`'s count is recorded and this returns
///    [`DiffOutcome::Seed`].
/// 3. Otherwise `prev`'s count is updated to `report`'s count, and:
///    - `report.messages()` is non-empty → the ids not already in
///      `seen` (deduplicated, report order) become
///      [`DiffOutcome::NewMessages`], or [`DiffOutcome::Nothing`] if
///      every id was already seen.
///    - `report.messages()` is empty and the count increased →
///      [`DiffOutcome::CountIncrease`] with the (always non-negative)
///      difference.
///    - Otherwise (count decreased or stayed the same) →
///      [`DiffOutcome::Nothing`].
///
/// `seen` is read-only here: `evaluate` never inserts into it. Deciding
/// which ids to persist into the ring — every id on `Seed`, or the new
/// ids on `NewMessages` — is left to the caller (task 4.5), which is
/// also the only thing wired up to `report_unread` and `StateStore`.
pub fn evaluate(
    prev: &mut ServiceNotifyState,
    report: &ValidReport,
    seen: &SeenRing,
) -> DiffOutcome {
    let Some(count) = report.count() else {
        return DiffOutcome::Nothing;
    };

    let Some(last_count) = prev.last_count else {
        prev.last_count = Some(count);
        return DiffOutcome::Seed;
    };

    prev.last_count = Some(count);

    let messages = report.messages();
    if !messages.is_empty() {
        let new_refs = new_message_refs(messages, seen);
        return if new_refs.is_empty() {
            DiffOutcome::Nothing
        } else {
            DiffOutcome::NewMessages(new_refs)
        };
    }

    if count > last_count {
        DiffOutcome::CountIncrease(count - last_count)
    } else {
        DiffOutcome::Nothing
    }
}

/// Extracts `messages`' ids not already in `seen`, in report order, with
/// duplicate ids within `messages` collapsed to their first occurrence
/// (design.md §2.2.9).
fn new_message_refs(messages: &[ValidMessageRef], seen_ring: &SeenRing) -> Vec<MessageRef> {
    let mut seen_this_report = HashSet::new();
    let mut result = Vec::new();
    for message in messages {
        if !seen_this_report.insert(message.id.as_str()) {
            continue;
        }
        if seen::contains(seen_ring, &message.id) {
            continue;
        }
        result.push(MessageRef::from(message));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::agent_bridge::{validate, MessageRefDto, UnreadReportDto};
    use crate::config::ServiceId;
    use crate::host;

    const SERVICE_ID: &str = "gmail-personal";
    const SERVICE_ORIGIN: &str = "https://mail.google.com";

    fn make_report(count: Option<i64>, messages: Vec<MessageRefDto>) -> ValidReport {
        let dto = UnreadReportDto {
            service_id: SERVICE_ID.to_string(),
            count,
            messages,
            recipe_id: "gmail".to_string(),
            observed_at: 1_700_000_000_000,
            icon_candidates: Vec::new(),
        };
        let label = host::service_label(&ServiceId::new(SERVICE_ID).expect("valid id"));
        let caller_url = Url::parse(SERVICE_ORIGIN).expect("valid url");
        let lookup = |_id: &ServiceId| Some(Url::parse(SERVICE_ORIGIN).expect("valid url"));
        validate(dto, &label, &caller_url, lookup).expect("report should validate")
    }

    fn message(id: &str) -> MessageRefDto {
        MessageRefDto {
            id: id.to_string(),
            from: None,
            subject: None,
            link: None,
        }
    }

    fn empty_ring() -> SeenRing {
        SeenRing::default()
    }

    fn ring_of(ids: impl IntoIterator<Item = &'static str>) -> SeenRing {
        SeenRing(ids.into_iter().map(str::to_string).collect())
    }

    #[test]
    fn first_some_report_since_launch_seeds_and_records_the_count() {
        let mut prev = ServiceNotifyState::new();
        let report = make_report(Some(5), Vec::new());

        let outcome = evaluate(&mut prev, &report, &empty_ring());

        assert_eq!(outcome, DiffOutcome::Seed);
        assert_eq!(prev.last_count(), Some(5));
    }

    #[test]
    fn a_null_count_before_seeding_returns_nothing_and_stays_unseeded() {
        let mut prev = ServiceNotifyState::new();
        let report = make_report(None, Vec::new());

        let outcome = evaluate(&mut prev, &report, &empty_ring());

        assert_eq!(outcome, DiffOutcome::Nothing);
        assert_eq!(prev.last_count(), None);

        // The next `Some` report is still treated as the first one.
        let seeded = make_report(Some(1), Vec::new());
        assert_eq!(
            evaluate(&mut prev, &seeded, &empty_ring()),
            DiffOutcome::Seed
        );
    }

    #[test]
    fn a_null_count_after_seeding_returns_nothing_and_does_not_change_the_baseline() {
        let mut prev = ServiceNotifyState::new();
        evaluate(&mut prev, &make_report(Some(5), Vec::new()), &empty_ring());
        assert_eq!(prev.last_count(), Some(5));

        let outcome = evaluate(&mut prev, &make_report(None, Vec::new()), &empty_ring());

        assert_eq!(outcome, DiffOutcome::Nothing);
        assert_eq!(prev.last_count(), Some(5));
    }

    #[test]
    fn messages_with_ids_not_in_seen_are_reported_as_new() {
        let mut prev = ServiceNotifyState::new();
        evaluate(&mut prev, &make_report(Some(1), Vec::new()), &empty_ring());

        let seen = ring_of(["a"]);
        let outcome = evaluate(
            &mut prev,
            &make_report(Some(3), vec![message("a"), message("b")]),
            &seen,
        );

        match outcome {
            DiffOutcome::NewMessages(refs) => {
                assert_eq!(refs.len(), 1);
                assert_eq!(refs[0].id, "b");
            }
            other => panic!("expected NewMessages, got {other:?}"),
        }
        assert_eq!(prev.last_count(), Some(3));
    }

    #[test]
    fn messages_all_already_seen_returns_nothing_even_if_count_increased() {
        let mut prev = ServiceNotifyState::new();
        evaluate(&mut prev, &make_report(Some(1), Vec::new()), &empty_ring());

        let seen = ring_of(["a", "b"]);
        let outcome = evaluate(
            &mut prev,
            &make_report(Some(5), vec![message("a"), message("b")]),
            &seen,
        );

        assert_eq!(outcome, DiffOutcome::Nothing);
        assert_eq!(prev.last_count(), Some(5));
    }

    #[test]
    fn duplicate_ids_within_one_report_are_collapsed_to_one_new_message() {
        let mut prev = ServiceNotifyState::new();
        evaluate(&mut prev, &make_report(Some(1), Vec::new()), &empty_ring());

        let outcome = evaluate(
            &mut prev,
            &make_report(Some(2), vec![message("a"), message("a")]),
            &empty_ring(),
        );

        match outcome {
            DiffOutcome::NewMessages(refs) => assert_eq!(refs.len(), 1),
            other => panic!("expected NewMessages, got {other:?}"),
        }
    }

    #[test]
    fn no_messages_and_a_higher_count_is_a_count_increase() {
        let mut prev = ServiceNotifyState::new();
        evaluate(&mut prev, &make_report(Some(3), Vec::new()), &empty_ring());

        let outcome = evaluate(&mut prev, &make_report(Some(7), Vec::new()), &empty_ring());

        assert_eq!(outcome, DiffOutcome::CountIncrease(4));
        assert_eq!(prev.last_count(), Some(7));
    }

    #[test]
    fn no_messages_and_a_lower_count_is_nothing_but_still_updates_the_baseline() {
        let mut prev = ServiceNotifyState::new();
        evaluate(&mut prev, &make_report(Some(5), Vec::new()), &empty_ring());

        let outcome = evaluate(&mut prev, &make_report(Some(2), Vec::new()), &empty_ring());
        assert_eq!(outcome, DiffOutcome::Nothing);
        assert_eq!(prev.last_count(), Some(2));

        // The lower count became the new baseline, so a rise back above
        // it is still reported as an increase from there.
        let outcome = evaluate(&mut prev, &make_report(Some(4), Vec::new()), &empty_ring());
        assert_eq!(outcome, DiffOutcome::CountIncrease(2));
    }

    #[test]
    fn no_messages_and_an_equal_count_is_nothing() {
        let mut prev = ServiceNotifyState::new();
        evaluate(&mut prev, &make_report(Some(5), Vec::new()), &empty_ring());

        let outcome = evaluate(&mut prev, &make_report(Some(5), Vec::new()), &empty_ring());

        assert_eq!(outcome, DiffOutcome::Nothing);
        assert_eq!(prev.last_count(), Some(5));
    }
}
