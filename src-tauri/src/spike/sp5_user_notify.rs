//! SP5 candidate: `user-notify` 0.4.2 (the crate named in
//! plugins-workspace#2150, per design.md §2.2.9 / SPEC.md §10).
//!
//! Read directly from the crate's source
//! (github.com/Simon-Laux/user-notify, tag matching 0.4.2):
//!
//! - macOS (`src/platform_impl/mac_os/manager.rs`,
//!   `src/platform_impl/mac_os/delegate.rs`): a `UNUserNotificationCenterDelegate`
//!   receives `didReceiveNotificationResponse` on the **main thread** (the
//!   delegate type is `#[thread_kind = MainThreadOnly]`), which sends the
//!   response over a `tokio::sync::mpsc` channel to a dedicated
//!   `listener_loop` background thread; that thread is what actually calls
//!   the `handler_callback` this module registers. So the callback this
//!   harness receives runs on a crate-owned background thread, not the
//!   delegate's main-thread callback itself, and NOT Tauri's main thread.
//! - Windows (`src/platform_impl/windows.rs`): `toast.Activated(&handler)`
//!   registers a WinRT `TypedEventHandler` that calls `handler_callback`
//!   directly from whatever thread WinRT invokes the COM event on — this
//!   crate does no marshalling of its own on Windows.
//! - Either way, `show`/`unminimize`/`set_focus` are called through
//!   `common::activate_main_window`, which explicitly hands off to Tauri's
//!   main thread via `AppHandle::run_on_main_thread` rather than assuming
//!   the callback thread is safe to call window methods from directly.
//!
//! Platform prerequisites confirmed from source (`get_notification_manager`
//! in `src/lib.rs`):
//! - macOS: falls back to a log-only mock manager if
//!   `NSBundle::mainBundle().bundleIdentifier()` is `None` — i.e. an
//!   unbundled `cargo run`/dev binary. Needs a real, signed `.app` bundle
//!   with a bundle id (see SPIKE-SP5.md).
//! - Windows: falls back to the same mock if
//!   `ToastNotificationManager::CreateToastNotifierWithId(app_id)` fails,
//!   which happens when `app_id` isn't a real AUMID registered via an
//!   installed Start Menu shortcut (see SPIKE-SP5.md).

use std::collections::HashMap;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tauri::{App, AppHandle};
use user_notify::{get_notification_manager, NotificationBuilder, NotificationManager};

use super::common::{activate_main_window, spike_log, SpikePayload};

/// Matches `tauri.conf.json`'s `identifier`. On Windows this must be a real
/// AUMID registered by an installer-created shortcut for `send_notification`
/// to actually deliver (see module docs above and SPIKE-SP5.md); on macOS
/// this value is unused by `user-notify` (it reads the bundle id from
/// `NSBundle.mainBundle` instead).
const APP_ID: &str = "com.goofmint.eluma";

const PAYLOAD_KEY: &str = "payload";

pub fn setup(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    let manager: Arc<dyn NotificationManager> = get_notification_manager(APP_ID.to_string(), None);

    let handle_for_register = handle.clone();
    let register_result = manager.register(
        Box::new(move |response| {
            let payload_json = response
                .user_info
                .get(PAYLOAD_KEY)
                .cloned()
                .unwrap_or_else(|| "<missing payload in user_info>".to_string());
            activate_main_window(&handle_for_register, "user-notify", &payload_json);
        }),
        Vec::new(),
    );
    if let Err(err) = register_result {
        spike_log(&handle, &format!("[user-notify] register() failed: {err}"));
    }

    let handle_for_send = handle.clone();
    thread::Builder::new()
        .name("sp5-user-notify".to_string())
        .spawn(move || send_after_delay(manager, handle_for_send))?;

    Ok(())
}

/// Runs on a plain OS thread (not the Tokio runtime) so the delay is a
/// simple blocking `sleep`; `send_notification` itself is async, so this
/// thread blocks on it via `tauri::async_runtime::block_on` rather than
/// pulling in a direct `tokio` dependency just for `time::sleep`.
fn send_after_delay(manager: Arc<dyn NotificationManager>, handle: AppHandle) {
    thread::sleep(Duration::from_secs(5));

    let payload = SpikePayload::new("user-notify");
    let payload_json = payload.encode();
    spike_log(
        &handle,
        &format!("[user-notify] sending notification payload={payload_json}"),
    );

    let mut user_info = HashMap::new();
    user_info.insert(PAYLOAD_KEY.to_string(), payload_json.clone());

    let builder = NotificationBuilder::new()
        .title(&payload.service)
        .body(&format!(
            "{} — new message {} ({payload_json})",
            payload.service, payload.message_id
        ))
        .set_user_info(user_info);

    let send_result =
        tauri::async_runtime::block_on(async move { manager.send_notification(builder).await });

    match send_result {
        Ok(_notification_handle) => spike_log(&handle, "[user-notify] send_notification ok"),
        Err(err) => spike_log(
            &handle,
            &format!("[user-notify] send_notification failed: {err}"),
        ),
    }
}
