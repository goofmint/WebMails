//! The per-service status state machine (design.md §2.2.7): a
//! `HashMap<ServiceId, ServiceStatus>` plus mutating methods, each of
//! which returns `Some(StatusChanged)` only when the mutation actually
//! changed that service's status, `None` otherwise (including for a
//! report or page-load naming a service [`StatusStore`] has never
//! registered via [`StatusStore::mark_loading`] — e.g. one already
//! removed).
//!
//! No Tauri dependency: every method is plain, synchronous and pure with
//! respect to its `&mut self`, so the whole state machine is
//! unit-tested below without an `AppHandle`. [`emit_if_changed`] is the
//! one seam a caller (`services::ServiceManager`) plugs an
//! `emit_to("shell", "status-changed", …)` closure into — injected, not
//! baked in here, so this module never has to know what "emit" means.

use std::collections::HashMap;

use url::{Origin, Url};

use crate::config::ServiceId;

use super::status::{is_off_origin, ServiceStatus, StatusChanged};

/// Owns every service's current [`ServiceStatus`] (design.md §2.2.7).
/// A service id is "registered" — eligible to receive a report or
/// page-load transition — from the moment [`Self::mark_loading`] first
/// inserts it until [`Self::remove`] takes it back out.
#[derive(Debug, Default)]
pub struct StatusStore {
    statuses: HashMap<ServiceId, ServiceStatus>,
}

impl StatusStore {
    pub fn new() -> Self {
        Self {
            statuses: HashMap::new(),
        }
    }

    /// The full status map, e.g. for a `get_snapshot` command (design.md
    /// §2.2.7's shell snapshot).
    pub fn statuses(&self) -> &HashMap<ServiceId, ServiceStatus> {
        &self.statuses
    }

    /// Registers `id` (if not already) and sets its status to
    /// [`ServiceStatus::Loading`] — called before a webview creation
    /// attempt (design.md §2.2.7: "created, no report yet").
    pub fn mark_loading(&mut self, id: &ServiceId) -> Option<StatusChanged> {
        self.set(id, ServiceStatus::Loading)
    }

    /// Sets `id`'s status to `NeedsAttention(CreateFailed)` (design.md
    /// §5.1) — called when profile resolution or webview creation fails.
    pub fn mark_create_failed(&mut self, id: &ServiceId) -> Option<StatusChanged> {
        self.set(id, ServiceStatus::create_failed())
    }

    /// Applies a validated report's transition (design.md §2.2.7):
    /// `Some(n)` → `Ok`, `None` → `NeedsAttention(ReportedNone)` —
    /// unconditionally, regardless of the current status, which is how a
    /// report also clears `NeedsAttention(OffOrigin)` ("leaves `OffOrigin`
    /// on the next report", design.md §2.2.7) without any special case.
    ///
    /// Ignored (`None`, no change) if `id` is not registered.
    pub fn record_report(&mut self, id: &ServiceId, count: Option<u32>) -> Option<StatusChanged> {
        if !self.statuses.contains_key(id) {
            return None;
        }
        self.set(id, ServiceStatus::from_report(count))
    }

    /// Applies an `on_page_load` origin check (design.md §2.2.7):
    /// same-origin or an `about:` page leaves the status untouched; a
    /// different origin sets `NeedsAttention(OffOrigin)`.
    ///
    /// Ignored (`None`, no change) if `id` is not registered.
    pub fn record_page_load(
        &mut self,
        id: &ServiceId,
        page_url: &Url,
        service_origin: &Origin,
    ) -> Option<StatusChanged> {
        if !self.statuses.contains_key(id) {
            return None;
        }
        if !is_off_origin(page_url, service_origin) {
            return None;
        }
        self.set(id, ServiceStatus::off_origin())
    }

    /// Unregisters `id`: no further report or page-load reaches it until
    /// (if ever) [`Self::mark_loading`] registers it again. No event is
    /// emitted — the shell already learns of the removal via
    /// `services-changed`.
    pub fn remove(&mut self, id: &ServiceId) {
        self.statuses.remove(id);
    }

    /// Sets `id`'s status to `new_status`, inserting it if not already
    /// present, and returns `Some(StatusChanged)` only if that actually
    /// changed the stored value — the one place change detection happens,
    /// so every public mutator gets it for free.
    fn set(&mut self, id: &ServiceId, new_status: ServiceStatus) -> Option<StatusChanged> {
        if self.statuses.get(id) == Some(&new_status) {
            return None;
        }
        self.statuses.insert(id.clone(), new_status.clone());
        Some(StatusChanged {
            service_id: id.clone(),
            status: new_status,
        })
    }
}

/// Calls `emit` with `changed`'s payload only when a mutation actually
/// changed a status. The injectable seam mentioned in the module doc:
/// production passes a closure that calls `app_handle.emit_to("shell",
/// "status-changed", …)`; tests pass a closure that records what it was
/// called with, so the whole state machine above is exercised without
/// any Tauri type in scope.
pub fn emit_if_changed(changed: Option<StatusChanged>, emit: impl FnOnce(StatusChanged)) {
    if let Some(changed) = changed {
        emit(changed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unread::status::AttentionReason;

    fn id(value: &str) -> ServiceId {
        ServiceId::new(value).expect("valid id")
    }

    fn origin(url: &str) -> Origin {
        Url::parse(url).expect("valid url").origin()
    }

    const GMAIL_ORIGIN: &str = "https://mail.google.com";

    #[test]
    fn mark_loading_registers_and_emits() {
        let mut store = StatusStore::new();
        let changed = store.mark_loading(&id("gmail")).expect("should change");
        assert_eq!(changed.service_id, id("gmail"));
        assert_eq!(changed.status, ServiceStatus::Loading);
        assert_eq!(
            store.statuses().get(&id("gmail")),
            Some(&ServiceStatus::Loading)
        );
    }

    #[test]
    fn mark_loading_again_while_already_loading_does_not_emit() {
        let mut store = StatusStore::new();
        store.mark_loading(&id("gmail"));
        assert_eq!(store.mark_loading(&id("gmail")), None);
    }

    #[test]
    fn mark_create_failed_transitions_and_emits() {
        let mut store = StatusStore::new();
        store.mark_loading(&id("gmail"));
        let changed = store
            .mark_create_failed(&id("gmail"))
            .expect("should change");
        assert_eq!(
            changed.status,
            ServiceStatus::NeedsAttention {
                reason: AttentionReason::CreateFailed
            }
        );
    }

    #[test]
    fn mark_create_failed_twice_does_not_emit_the_second_time() {
        let mut store = StatusStore::new();
        store.mark_loading(&id("gmail"));
        store.mark_create_failed(&id("gmail"));
        assert_eq!(store.mark_create_failed(&id("gmail")), None);
    }

    #[test]
    fn record_report_some_transitions_to_ok_and_emits() {
        let mut store = StatusStore::new();
        store.mark_loading(&id("gmail"));
        let changed = store
            .record_report(&id("gmail"), Some(7))
            .expect("should change");
        assert_eq!(changed.status, ServiceStatus::Ok { count: 7 });
    }

    #[test]
    fn record_report_none_transitions_to_needs_attention_reported_none() {
        let mut store = StatusStore::new();
        store.mark_loading(&id("gmail"));
        let changed = store
            .record_report(&id("gmail"), None)
            .expect("should change");
        assert_eq!(
            changed.status,
            ServiceStatus::NeedsAttention {
                reason: AttentionReason::ReportedNone
            }
        );
    }

    #[test]
    fn record_report_same_count_twice_does_not_emit_the_second_time() {
        let mut store = StatusStore::new();
        store.mark_loading(&id("gmail"));
        store.record_report(&id("gmail"), Some(3));
        assert_eq!(store.record_report(&id("gmail"), Some(3)), None);
    }

    #[test]
    fn record_report_changing_count_emits_again() {
        let mut store = StatusStore::new();
        store.mark_loading(&id("gmail"));
        store.record_report(&id("gmail"), Some(3));
        let changed = store
            .record_report(&id("gmail"), Some(4))
            .expect("should change");
        assert_eq!(changed.status, ServiceStatus::Ok { count: 4 });
    }

    #[test]
    fn record_report_ignores_an_unregistered_service() {
        let mut store = StatusStore::new();
        assert_eq!(store.record_report(&id("never-created"), Some(1)), None);
        assert!(store.statuses().is_empty());
    }

    #[test]
    fn record_page_load_off_origin_transitions_and_emits() {
        let mut store = StatusStore::new();
        store.mark_loading(&id("gmail"));
        let page = Url::parse("https://evil.example.com").expect("valid url");
        let changed = store
            .record_page_load(&id("gmail"), &page, &origin(GMAIL_ORIGIN))
            .expect("should change");
        assert_eq!(
            changed.status,
            ServiceStatus::NeedsAttention {
                reason: AttentionReason::OffOrigin
            }
        );
    }

    #[test]
    fn record_page_load_same_origin_does_not_change_or_emit() {
        let mut store = StatusStore::new();
        store.mark_loading(&id("gmail"));
        store.record_report(&id("gmail"), Some(2));
        let page = Url::parse("https://mail.google.com/mail/u/0/").expect("valid url");
        assert_eq!(
            store.record_page_load(&id("gmail"), &page, &origin(GMAIL_ORIGIN)),
            None
        );
        assert_eq!(
            store.statuses().get(&id("gmail")),
            Some(&ServiceStatus::Ok { count: 2 })
        );
    }

    #[test]
    fn record_page_load_about_page_does_not_change_or_emit() {
        let mut store = StatusStore::new();
        store.mark_loading(&id("gmail"));
        let page = Url::parse("about:blank").expect("valid url");
        assert_eq!(
            store.record_page_load(&id("gmail"), &page, &origin(GMAIL_ORIGIN)),
            None
        );
        assert_eq!(
            store.statuses().get(&id("gmail")),
            Some(&ServiceStatus::Loading)
        );
    }

    #[test]
    fn record_page_load_ignores_an_unregistered_service() {
        let mut store = StatusStore::new();
        let page = Url::parse("https://evil.example.com").expect("valid url");
        assert_eq!(
            store.record_page_load(&id("never-created"), &page, &origin(GMAIL_ORIGIN)),
            None
        );
        assert!(store.statuses().is_empty());
    }

    /// design.md §2.2.7: "The status leaves `OffOrigin` on the next
    /// report received from the service origin" — a subsequent *report*
    /// clears it (via the unconditional `Some`/`None` transition above),
    /// but a page-load back to the same origin does not, since only a
    /// report is documented to clear it.
    #[test]
    fn off_origin_is_cleared_by_the_next_report_not_by_a_same_origin_page_load() {
        let mut store = StatusStore::new();
        store.mark_loading(&id("gmail"));
        let evil = Url::parse("https://evil.example.com").expect("valid url");
        store.record_page_load(&id("gmail"), &evil, &origin(GMAIL_ORIGIN));
        assert_eq!(
            store.statuses().get(&id("gmail")),
            Some(&ServiceStatus::NeedsAttention {
                reason: AttentionReason::OffOrigin
            })
        );

        // A same-origin page load alone does not clear OffOrigin.
        let same_origin_page = Url::parse("https://mail.google.com/mail/u/0/").expect("valid url");
        store.record_page_load(&id("gmail"), &same_origin_page, &origin(GMAIL_ORIGIN));
        assert_eq!(
            store.statuses().get(&id("gmail")),
            Some(&ServiceStatus::NeedsAttention {
                reason: AttentionReason::OffOrigin
            })
        );

        // The next report clears it.
        let changed = store
            .record_report(&id("gmail"), Some(5))
            .expect("should change");
        assert_eq!(changed.status, ServiceStatus::Ok { count: 5 });
    }

    #[test]
    fn remove_unregisters_a_service() {
        let mut store = StatusStore::new();
        store.mark_loading(&id("gmail"));
        store.remove(&id("gmail"));
        assert!(store.statuses().is_empty());

        // Reports and page loads for the now-removed id are ignored.
        assert_eq!(store.record_report(&id("gmail"), Some(1)), None);
        let page = Url::parse("https://evil.example.com").expect("valid url");
        assert_eq!(
            store.record_page_load(&id("gmail"), &page, &origin(GMAIL_ORIGIN)),
            None
        );
        assert!(store.statuses().is_empty());
    }

    #[test]
    fn remove_of_an_unregistered_service_is_a_no_op() {
        let mut store = StatusStore::new();
        store.remove(&id("never-existed"));
        assert!(store.statuses().is_empty());
    }

    #[test]
    fn a_service_can_be_recreated_after_removal() {
        let mut store = StatusStore::new();
        store.mark_loading(&id("gmail"));
        store.record_report(&id("gmail"), Some(1));
        store.remove(&id("gmail"));

        let changed = store.mark_loading(&id("gmail")).expect("should change");
        assert_eq!(changed.status, ServiceStatus::Loading);
    }

    #[test]
    fn emit_if_changed_calls_emit_only_when_some() {
        let mut calls = Vec::new();
        emit_if_changed(None, |changed| calls.push(changed));
        assert!(calls.is_empty());

        let changed = StatusChanged {
            service_id: id("gmail"),
            status: ServiceStatus::Loading,
        };
        emit_if_changed(Some(changed.clone()), |c| calls.push(c));
        assert_eq!(calls, vec![changed]);
    }
}
