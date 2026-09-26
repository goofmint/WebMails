//! Notification pipeline for unread-count reports (design.md §2.2.9):
//! deciding what changed (`diff`), remembering which message ids have
//! already been notified about (`seen`), turning that decision into
//! notification text (`text`) and an `OutgoingNotification` (`sink`), and
//! coordinating persistence plus sending (`dispatcher`).
//!
//! `diff` and `seen` are pure and know nothing about Tauri, `StateStore`,
//! or `report_unread` — see their own module docs. `dispatcher::Dispatcher`
//! is the seam that ties them to a live `StateStore` and a
//! `NotificationSink`; `agent_bridge::report_unread` (Task 4.5's wiring)
//! is its only production caller, via `services::ServiceManager::
//! with_notify_state`.

mod diff;
mod dispatcher;
mod seen;
mod sink;
mod text;

pub use dispatcher::Dispatcher;
pub use sink::TauriNotificationSink;
