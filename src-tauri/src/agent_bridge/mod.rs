//! Builds the agent injection script, adds runtime capabilities, and
//! implements the `report_unread` command and its validation (design.md
//! §2.2.6, §1.3).
//!
//! The `report_unread` command (Task 2.1) and the runtime capability /
//! injection-script building [`capability`] adds (Task 2.2) both live
//! here. This module does not touch `capabilities/default.json` (the
//! static shell capability) at all — only runtime capabilities added via
//! [`capability::ensure_capability`].
//!
//! Task 1.14 adds one more consumer of a validated report: its
//! `iconCandidates`, when non-empty, are handed to
//! [`crate::services::ServiceManager::record_icon_candidates`] — see
//! [`report_unread`]'s own doc comment.

pub mod capability;
mod dto;
mod validate;

use std::sync::Arc;

use tauri::{Manager, State};

use crate::config::ServiceId;
use crate::liveness::LivenessRuntime;
use crate::notify::Dispatcher;
use crate::services::ServiceManager;

pub use dto::UnreadReportDto;
pub use validate::{ReportError, ValidMessageRef, ValidReport};

// Crate-visible only (not part of this crate's public API): task 4.4's
// `notify::diff` tests build `ValidReport` fixtures through the real
// `validate()` function — there is deliberately no public constructor
// for `ValidReport` itself — which needs `MessageRefDto` to fill in
// `UnreadReportDto::messages`. Only that (`#[cfg(test)]`) code uses
// these outside this module, so a non-test build sees them as unused.
#[allow(unused_imports)]
pub(crate) use dto::MessageRefDto;
#[allow(unused_imports)]
pub(crate) use validate::validate;

/// Validates and (for now) only logs an unread report from a service's
/// injected agent (design.md §2.2.6).
///
/// `webview.label()` must equal `svc-<serviceId>` for the report's own
/// `serviceId`, that service must exist in the live configuration, and
/// the webview's *current* URL (`webview.url()`) must share the
/// service's currently configured origin — [`validate::validate`]
/// checks all of that, plus every other design.md §2.2.6 rule, and
/// rejects on the first violation.
///
/// `webview.url()` itself can fail; when it does, the report is dropped
/// the same way a validation rejection is (logged at debug level, no
/// fallback origin substituted, still `Ok(())`) rather than calling
/// [`validate::validate`] with anything but the real current URL.
///
/// The service's currently configured URL is looked up from
/// [`ServiceManager`]'s live `Config` (`services::ServiceManager::
/// service_url`), keyed off the raw `serviceId` string the agent sent.
/// If that string is not even a well-formed [`ServiceId`], no service
/// can match it, so the lookup is skipped (`None`) and [`validate::validate`]
/// rejects the report as a label mismatch before ever consulting this
/// lookup's result.
///
/// On success, the report's count feeds the `unread` status store
/// (design.md §2.2.7: `Some(n)` → `Ok`, `None` →
/// `NeedsAttention(ReportedNone)`) via [`ServiceManager::record_report`],
/// plus (Task 1.14) a validated report's *icon candidates*, when
/// non-empty, are handed to [`ServiceManager::record_icon_candidates`];
/// then a debug log line naming the service id is emitted and `Ok(())`
/// is returned. On rejection, only the service id and the rejection's
/// `kind()` are logged (never the report's contents), and the report is
/// dropped: the agent is not notified (design.md §5.1), so the command
/// still returns `Ok(())`. The `Result` return type is kept because
/// Tauri requires it for async commands that borrow managed state.
#[tauri::command]
pub async fn report_unread(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    report: UnreadReportDto,
    services: State<'_, Arc<ServiceManager>>,
    dispatcher: State<'_, Arc<Dispatcher>>,
) -> Result<(), ReportError> {
    let label = webview.label().to_string();
    let requested_service_id = report.service_id.clone();

    let caller_url = match webview.url() {
        Ok(url) => url,
        Err(err) => {
            tracing::debug!(
                service_id = %requested_service_id,
                error = %err,
                "failed to read calling webview's URL; dropping report"
            );
            return Ok(());
        }
    };

    let service_url = match ServiceId::new(requested_service_id.clone()) {
        Ok(id) => services.inner().service_url(&id).await,
        Err(_) => None,
    };

    match validate::validate(report, &label, &caller_url, |_id| service_url) {
        Ok(valid_report) => {
            services
                .inner()
                .record_report(valid_report.service_id(), valid_report.count());
            // Liveness (design.md §2.2.8, §2.2.6: "Liveness uses the time
            // Rust receives the report", never the report's own
            // `observedAt`). Looked up via `try_state`, not the `State`
            // extractor: the runtime is legitimately unmanaged when
            // startup failed to open `state.json` (see `lib.rs`'s
            // `setup`), and no webview — hence no report — should exist
            // in that case anyway, but this must not panic if one somehow
            // arrives.
            if let Some(liveness) = app.try_state::<Arc<LivenessRuntime>>() {
                liveness.record_report(valid_report.service_id());
            }
            tracing::debug!(
                service_id = %valid_report.service_id(),
                "accepted unread report"
            );
            dispatch_notification(services.inner(), dispatcher.inner(), &valid_report).await;

            // Task 1.14 (design.md §2.2.10): a validated report's *icon
            // candidates*, when non-empty, are consumed here too.
            let candidates = valid_report.icon_candidates();
            if !candidates.is_empty() {
                let manager = Arc::clone(services.inner());
                let service_id = valid_report.service_id().clone();
                let candidates = candidates.to_vec();
                if let Some(icon_source) = manager
                    .record_icon_candidates(&service_id, candidates)
                    .await
                {
                    ServiceManager::spawn_icon_resolve(&manager, service_id, icon_source);
                }
            }
            Ok(())
        }
        Err(err) => {
            tracing::debug!(
                service_id = %requested_service_id,
                rejection = err.kind(),
                "rejected unread report"
            );
            Ok(())
        }
    }
}

/// Runs `notify::diff::evaluate` and dispatches any resulting
/// notification for `report` (design.md §2.2.9; Task 4.5's wiring).
///
/// A small, standalone call — kept separate from `report_unread`'s own
/// body — so it stays a single, easily-rebased addition to the success
/// arm alongside Task 2.3's (not yet merged) unread status store, rather
/// than the two changes interleaving in the same block.
///
/// Looks up `report.service_id()`'s current display name and
/// notification toggles/threshold through [`ServiceManager::
/// with_notify_state`], which also runs [`Dispatcher::evaluate_and_persist`]
/// under that same lock. If that returns `None` (the service no longer
/// exists, or `services` never started), this logs at debug level and
/// does nothing further — the same "drop and log, no fallback" rule
/// `report_unread` itself already follows. On `Some`, any resulting
/// notification is planned and sent only after `ServiceManager`'s lock
/// has already been released (`Dispatcher::send`'s own contract).
async fn dispatch_notification(
    services: &ServiceManager,
    dispatcher: &Dispatcher,
    report: &ValidReport,
) {
    let id = report.service_id().clone();
    let Some(ctx) = services
        .with_notify_state(&id, |state| {
            dispatcher.evaluate_and_persist(state, &id, report)
        })
        .await
    else {
        tracing::debug!(service_id = %id, "no live service for notification dispatch");
        return;
    };

    match ctx.result {
        Ok(outcome) => dispatcher.send(
            &id,
            &ctx.service_name,
            ctx.global_notifications,
            ctx.service_notifications,
            ctx.batch_threshold,
            outcome,
        ),
        Err(err) => {
            tracing::warn!(
                service_id = %id,
                error = %err,
                "failed to persist notification state for report"
            );
        }
    }
}
