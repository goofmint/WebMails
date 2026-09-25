//! SP5 candidate: `notify-rust` 4.18.0.
//!
//! On macOS this crate is built with the `preview-macos-un` feature (see
//! `src-tauri/Cargo.toml`), which switches it from the deprecated
//! `NSUserNotificationCenter` backend to `UNUserNotificationCenter` — the
//! same underlying API family `user-notify` uses, and the one recommended
//! for macOS 14+ (the default `NSUserNotificationCenter` backend's own doc
//! comment in notify-rust's source says it "still works" on 14+ but is
//! deprecated). On Windows, `notify-rust` always uses the
//! `tauri-winrt-notification` crate (`ToastNotification`/`ToastNotifier`),
//! regardless of feature flags.
//!
//! Both platforms expose the same blocking API:
//! `Notification::show()` returns a `NotificationHandle`, and
//! `handle.wait_for_response(handler)` **blocks the calling thread** until
//! the user interacts with the notification (or, on macOS, until the
//! process's run loop delivers a delegate callback the handle is waiting
//! on). There is no async/callback-registration API — the crate's model is
//! "spawn a thread, block it on this notification's response" rather than
//! "register a global click handler", which is the opposite of
//! `user-notify`'s model (see `sp5_user_notify.rs`). That's the thing to
//! record under "how the callback thread hands work to the main thread":
//! for this candidate, the thread that receives the click *is* the thread
//! that called `wait_for_response`, i.e. a thread this harness spawned and
//! named itself, not a callback thread owned by the crate.

use std::thread;
use std::time::Duration;

use notify_rust::{Notification, NotificationResponse};
use tauri::{App, AppHandle};

use super::common::{activate_main_window, spike_log, SpikePayload};

pub fn setup(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    thread::Builder::new()
        .name("sp5-notify-rust".to_string())
        .spawn(move || run(handle))?;
    Ok(())
}

fn run(handle: AppHandle) {
    thread::sleep(Duration::from_secs(5));

    let payload = SpikePayload::new("notify-rust");
    let payload_json = payload.encode();
    spike_log(
        &handle,
        &format!("[notify-rust] sending notification payload={payload_json}"),
    );

    let show_result = Notification::new()
        .appname("Eluma")
        .summary(&payload.service)
        .body(&format!(
            "{} — new message {} ({payload_json})",
            payload.service, payload.message_id
        ))
        .show();

    let notification_handle = match show_result {
        Ok(notification_handle) => notification_handle,
        Err(err) => {
            spike_log(&handle, &format!("[notify-rust] show() failed: {err}"));
            return;
        }
    };

    spike_log(
        &handle,
        "[notify-rust] shown; blocking this worker thread in wait_for_response() until the user acts on it",
    );

    let wait_result =
        notification_handle.wait_for_response(move |response: &NotificationResponse| {
            spike_log(
                &handle,
                &format!("[notify-rust] wait_for_response resolved: {response:?}"),
            );
            match response {
                NotificationResponse::Default | NotificationResponse::Action(_) => {
                    activate_main_window(&handle, "notify-rust", &payload_json);
                }
                NotificationResponse::Reply(_) | NotificationResponse::Closed(_) => {
                    // Not a click: a text reply (macOS preview-macos-un only) or
                    // the notification was dismissed/expired without activation.
                }
            }
        });

    if let Err(err) = wait_result {
        eprintln!("[sp5] [notify-rust] wait_for_response failed: {err}");
    }
}
