//! SP5 baseline candidate: `tauri-plugin-notification` 2.4.0.
//!
//! Included for comparison only — design.md §2.2.9 and SPEC.md §10 already
//! establish (plugins-workspace#2150) that this plugin delivers **no click
//! event** on desktop. This module exists so the owner can see the same
//! "identifying payload" notification appear from all three candidates
//! side by side; it never calls `common::activate_main_window` because
//! there is nothing to react to a click with.

use std::thread;
use std::time::Duration;

use tauri::App;
use tauri_plugin_notification::NotificationExt;

use super::common::{spike_log, SpikePayload};

/// `tauri_plugin_notification::init()` must be registered on the
/// `tauri::Builder` itself (before `.build()`/`.run()`), which happens in
/// `eluma_lib::run` behind the same `sp5-baseline` feature — this `setup`
/// only schedules the delayed send, mirroring the other two candidates.
pub fn setup(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    spike_log(
        &handle,
        "[baseline] tauri-plugin-notification 2.4.0 registered (display-only, no click event, plugins-workspace#2150)",
    );

    thread::Builder::new()
        .name("sp5-baseline".to_string())
        .spawn(move || {
            thread::sleep(Duration::from_secs(5));

            let payload = SpikePayload::new("baseline");
            let payload_json = payload.encode();
            spike_log(
                &handle,
                &format!("[baseline] sending notification payload={payload_json}"),
            );

            let result = handle
                .notification()
                .builder()
                .title(&payload.service)
                .body(format!(
                    "{} — new message {} ({payload_json})",
                    payload.service, payload.message_id
                ))
                .show();

            match result {
                Ok(()) => spike_log(&handle, "[baseline] show() ok (no click event to wait for)"),
                Err(err) => spike_log(&handle, &format!("[baseline] show() failed: {err}")),
            }
        })?;

    Ok(())
}
