//! Shared helpers for the SP5 notification-click spike (docs/spikes/SP5.md,
//! tasks.md Task 4.1).
//!
//! Throwaway: this module and everything under `src/spike/` exists only on
//! `spike/sp5-notifications` to let the owner compare candidate crates. It
//! is never wired into the production app and is not expected to be merged.

use std::fs::OpenOptions;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// Identifying data carried by the spike notification, standing in for
/// design.md §2.2.9's `OutgoingNotification` (a service id plus a message
/// reference) without pulling in the real dispatcher/seen-ring machinery
/// that Task 4.4/4.5 will implement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpikePayload {
    pub candidate: String,
    pub service: String,
    pub message_id: String,
    pub sent_at_ms: u128,
}

impl SpikePayload {
    pub fn new(candidate: &str) -> Self {
        let sent_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_millis();
        Self {
            candidate: candidate.to_string(),
            service: "spike-gmail".to_string(),
            message_id: format!("msg-{sent_at_ms:x}"),
            sent_at_ms,
        }
    }

    pub fn encode(&self) -> String {
        serde_json::to_string(self).expect("SpikePayload always serializes")
    }
}

/// Writes one line to stderr and appends it to
/// `{app_data_dir}/logs/spike-sp5.log`, so a click that happened while the
/// window was minimized (and no terminal/DevTools was visible) can still be
/// read back afterwards. Mirrors `spike_log` in the SP4 harness
/// (docs/spikes/SP4.md).
pub fn spike_log(app: &AppHandle, line: &str) {
    eprintln!("[sp5] {line}");

    let dir = match app.path().app_data_dir() {
        Ok(dir) => dir,
        Err(err) => {
            eprintln!("[sp5] could not resolve app_data_dir to write log file: {err}");
            return;
        }
    };
    let logs_dir = dir.join("logs");
    if let Err(err) = std::fs::create_dir_all(&logs_dir) {
        eprintln!("[sp5] could not create log dir {logs_dir:?}: {err}");
        return;
    }
    let log_path = logs_dir.join("spike-sp5.log");
    match OpenOptions::new().create(true).append(true).open(&log_path) {
        Ok(mut file) => {
            if let Err(err) = writeln!(file, "{line}") {
                eprintln!("[sp5] could not write to {log_path:?}: {err}");
            }
        }
        Err(err) => eprintln!("[sp5] could not open {log_path:?}: {err}"),
    }
}

/// Called from wherever a candidate crate's click callback lands (a
/// background thread for every candidate tried here — see docs/spikes/SP5.md
/// "callback thread" notes). Logs which OS thread the click arrived on, then
/// explicitly hands the window-activation calls to the main thread via
/// `AppHandle::run_on_main_thread`, and on that thread calls `show`,
/// `unminimize`, `set_focus` on the main window (design.md §2.2.9 sink.rs
/// activation sequence).
///
/// `#[allow(dead_code)]`: unused when only the display-only `sp5-baseline`
/// feature is compiled in (it has no click event to react to).
#[allow(dead_code)]
pub fn activate_main_window(app: &AppHandle, candidate: &str, payload_json: &str) {
    let thread_name = std::thread::current()
        .name()
        .unwrap_or("<unnamed>")
        .to_string();
    let thread_id = format!("{:?}", std::thread::current().id());
    spike_log(
        app,
        &format!(
            "[{candidate}] click received on callback thread name={thread_name} id={thread_id}; payload={payload_json}"
        ),
    );

    let app_for_main = app.clone();
    let candidate_for_main = candidate.to_string();
    let dispatch_result = app.run_on_main_thread(move || {
        let main_thread_id = format!("{:?}", std::thread::current().id());
        spike_log(
            &app_for_main,
            &format!(
                "[{candidate_for_main}] activation running on main thread id={main_thread_id}"
            ),
        );

        let Some(window) = app_for_main.get_webview_window("main") else {
            spike_log(
                &app_for_main,
                &format!("[{candidate_for_main}] main window \"main\" not found"),
            );
            return;
        };

        if let Err(err) = window.show() {
            spike_log(
                &app_for_main,
                &format!("[{candidate_for_main}] show() failed: {err}"),
            );
        }
        if let Err(err) = window.unminimize() {
            spike_log(
                &app_for_main,
                &format!("[{candidate_for_main}] unminimize() failed: {err}"),
            );
        }
        if let Err(err) = window.set_focus() {
            spike_log(
                &app_for_main,
                &format!("[{candidate_for_main}] set_focus() failed: {err}"),
            );
        }
    });

    if let Err(err) = dispatch_result {
        spike_log(
            app,
            &format!("[{candidate}] run_on_main_thread dispatch failed: {err}"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spike_payload_encodes_as_json_object_with_expected_keys() {
        let payload = SpikePayload::new("test-candidate");
        let json = payload.encode();
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        assert_eq!(value["candidate"], "test-candidate");
        assert_eq!(value["service"], "spike-gmail");
        assert!(value["message_id"].as_str().unwrap().starts_with("msg-"));
        assert!(value["sent_at_ms"].as_u64().unwrap() > 0);
    }
}
