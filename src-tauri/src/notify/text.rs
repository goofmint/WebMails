//! Every user-visible notification string, in one place (design.md
//! §2.2.9: "All user-visible strings are English literals in one module,
//! `notify/text.rs`; the spec defines no i18n").
//!
//! [`notify::dispatcher::plan_notifications`](super::dispatcher::plan_notifications)
//! is the only caller: this module just formats strings, with no
//! knowledge of `DiffOutcome`, toggles or thresholds.

/// The body used for a rich notification whose message has neither
/// `from` nor `subject` (design.md §2.2.9's dispatcher rules).
const NEW_MESSAGE_FALLBACK: &str = "New message";

/// A notification's title: the service's display name. The same title is
/// used whether the notification ends up rich, batched or count-only, so
/// the user can always tell at a glance which service it is about.
pub fn title(service_name: &str) -> String {
    service_name.to_string()
}

/// A rich notification's body for one new message (design.md §2.2.9):
/// `"{from} — {subject}"` when both are present, just the one that is
/// present when the other is missing, or `"New message"` when both are
/// missing.
pub fn rich_body(from: Option<&str>, subject: Option<&str>) -> String {
    match (from, subject) {
        (Some(from), Some(subject)) => format!("{from} — {subject}"),
        (Some(from), None) => from.to_string(),
        (None, Some(subject)) => subject.to_string(),
        (None, None) => NEW_MESSAGE_FALLBACK.to_string(),
    }
}

/// A batched or count-increase notification's body (design.md §2.2.9):
/// `"{n} new messages"`, shared between `NewMessages` batching and
/// `CountIncrease`.
pub fn count_body(n: u32) -> String {
    format!("{n} new messages")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_is_the_service_name_unchanged() {
        assert_eq!(title("Gmail — Personal"), "Gmail — Personal");
    }

    #[test]
    fn rich_body_joins_from_and_subject_with_an_em_dash() {
        assert_eq!(rich_body(Some("Alice"), Some("Lunch?")), "Alice — Lunch?");
    }

    #[test]
    fn rich_body_uses_only_from_when_subject_is_missing() {
        assert_eq!(rich_body(Some("Alice"), None), "Alice");
    }

    #[test]
    fn rich_body_uses_only_subject_when_from_is_missing() {
        assert_eq!(rich_body(None, Some("Lunch?")), "Lunch?");
    }

    #[test]
    fn rich_body_falls_back_when_both_are_missing() {
        assert_eq!(rich_body(None, None), "New message");
    }

    #[test]
    fn count_body_formats_the_count_and_suffix() {
        assert_eq!(count_body(1), "1 new messages");
        assert_eq!(count_body(42), "42 new messages");
    }
}
