//! Builds the agent injection script, adds runtime capabilities, and
//! implements the `report_unread` command and its validation (design.md
//! §2.2.6, §1.3).
//!
//! Only the `report_unread` command (Task 2.1) lives here so far. The
//! injection script and per-service runtime capability (design.md
//! §2.2.6's `CapabilityBuilder` example) are Task 2.2's job — this
//! module does not touch `capabilities/default.json` (the static shell
//! capability) at all.

mod dto;
mod validate;

use std::sync::Arc;

use tauri::State;

use crate::config::ServiceId;
use crate::services::ServiceManager;

pub use dto::UnreadReportDto;
pub use validate::ReportError;

/// Validates and (for now) only logs an unread report from a service's
/// injected agent (design.md §2.2.6).
///
/// `webview.label()` must equal `svc-<serviceId>` for the report's own
/// `serviceId`, and that service must exist in the live configuration —
/// [`validate::validate`] checks both, plus every other design.md
/// §2.2.6 rule, and rejects on the first violation.
///
/// The service's currently configured URL is looked up from
/// [`ServiceManager`]'s live `Config` (`services::ServiceManager::
/// service_url`), keyed off the raw `serviceId` string the agent sent.
/// If that string is not even a well-formed [`ServiceId`], no service
/// can match it, so the lookup is skipped (`None`) and [`validate::validate`]
/// rejects the report as a label mismatch before ever consulting this
/// lookup's result.
///
/// On success there is no consumer yet — Task 2.3 adds the `unread`
/// status store that a validated report will feed — so this only emits
/// a debug log line naming the service id and returns `Ok(())`. On
/// rejection, only the service id and the rejection's `kind()` are
/// logged (never the report's contents), and the [`ReportError`] itself
/// is returned to the caller.
#[tauri::command]
pub async fn report_unread(
    webview: tauri::Webview,
    report: UnreadReportDto,
    services: State<'_, Arc<ServiceManager>>,
) -> Result<(), ReportError> {
    let label = webview.label().to_string();
    let requested_service_id = report.service_id.clone();

    let service_url = match ServiceId::new(requested_service_id.clone()) {
        Ok(id) => services.inner().service_url(&id).await,
        Err(_) => None,
    };

    match validate::validate(report, &label, |_id| service_url) {
        Ok(valid_report) => {
            // Task 2.3's `unread` status store will consume `valid_report`
            // here once it exists; until then, accepting it is a no-op
            // beyond this log line.
            tracing::debug!(
                service_id = %valid_report.service_id(),
                "accepted unread report (no consumer yet — Task 2.3)"
            );
            Ok(())
        }
        Err(err) => {
            tracing::debug!(
                service_id = %requested_service_id,
                rejection = err.kind(),
                "rejected unread report"
            );
            Err(err)
        }
    }
}
