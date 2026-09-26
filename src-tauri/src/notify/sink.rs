//! [`NotificationSink`]: the boundary between `notify::dispatcher`'s pure
//! planning and an actual OS notification (design.md §2.2.9).
//!
//! [`TauriNotificationSink`] is the baseline, production implementation,
//! over `tauri-plugin-notification`. It is display-only — clicking a
//! notification does nothing yet; a click-capable sink (focusing the
//! window, `host.activate`, navigating to `link`) is Task 4.6's M4 spike
//! ("only if SP5 found a click-capable crate"), not part of this task.
//!
//! `TauriNotificationSink` calls the plugin's Rust-side builder API
//! (`AppHandle::notification()`, via [`NotificationExt`]) directly —
//! never through Tauri's IPC/ACL layer, since nothing here is invoked
//! from a webview. That means no service (or any other) webview needs a
//! runtime capability grant for this to work: the plugin only needs
//! `.plugin(tauri_plugin_notification::init())` registered on the
//! `tauri::Builder` in `lib.rs`. The design's "the shell must NOT gain
//! notification permissions unless required" constraint therefore holds
//! by construction — there is nothing to add to `capabilities/shell.json`
//! for this — not because it was deliberately left out.

use tauri::{AppHandle, Wry};
use tauri_plugin_notification::NotificationExt;
use url::Url;

use crate::config::ServiceId;
use crate::error::AppError;

/// One notification `notify::dispatcher` has decided to show (design.md
/// §2.2.9), addressed to no particular window — just enough for a sink to
/// render it.
///
/// `link` has no reader yet: no click-capable sink exists until Task 4.6,
/// an M4 spike outside this task's scope, so it stays
/// `#[allow(dead_code)]` here rather than fabricating a reader that
/// doesn't exist (the same pattern `agent_bridge::validate::ValidReport`
/// already uses for fields awaiting a later task).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutgoingNotification {
    pub service: ServiceId,
    pub title: String,
    pub body: String,
    #[allow(dead_code)]
    pub link: Option<Url>,
}

/// Shows a notification, or reports why it could not be shown.
///
/// `Send + Sync` so `notify::dispatcher::Dispatcher` can hold one behind
/// an `Arc<dyn NotificationSink>` and share it across the async
/// `report_unread` command (design.md §2.2.9).
pub trait NotificationSink: Send + Sync {
    fn show(&self, notification: OutgoingNotification) -> Result<(), AppError>;
}

/// The baseline sink: `tauri-plugin-notification`, display-only
/// (design.md §2.2.9). `service` and `link` are not read by this
/// implementation — `title`/`body` are the whole notification a
/// display-only sink can show.
pub struct TauriNotificationSink {
    app_handle: AppHandle<Wry>,
}

impl TauriNotificationSink {
    pub fn new(app_handle: AppHandle<Wry>) -> Self {
        Self { app_handle }
    }
}

impl NotificationSink for TauriNotificationSink {
    fn show(&self, notification: OutgoingNotification) -> Result<(), AppError> {
        self.app_handle
            .notification()
            .builder()
            .title(notification.title)
            .body(notification.body)
            .show()
            .map_err(|err| AppError::Notification(err.to_string()))
    }
}
