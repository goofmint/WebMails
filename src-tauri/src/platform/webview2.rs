//! Windows only: denies the WebView2 `NOTIFICATIONS` permission for a
//! service webview (design.md §2.2.11) — a second, WebView2-level line of
//! defense alongside the agent's own `Notification` stub
//! (`agent/core/notification-stub.ts`, Task 4.3), in case anything in a
//! service page ever reaches the real browser API instead of the stub.
//!
//! [`deny_notification_permission`] is called once per service webview,
//! right after the host creates it
//! (`host::multiwebview`/`host::child_windows`, both `cfg(windows)` only).

use tauri::webview::PlatformWebview;
use tauri::{Webview, Wry};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_PERMISSION_KIND, COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS,
    COREWEBVIEW2_PERMISSION_KIND_UNKNOWN_PERMISSION, COREWEBVIEW2_PERMISSION_STATE_DENY,
};
use webview2_com::PermissionRequestedEventHandler;

/// The one decision the `PermissionRequested` handler makes (design.md
/// §2.2.11): deny `NOTIFICATIONS`, leave every other kind untouched. A
/// plain function so it can be unit-tested without any COM interface —
/// the handler below (which does need one) only calls it.
fn should_deny(kind: COREWEBVIEW2_PERMISSION_KIND) -> bool {
    kind == COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS
}

/// Registers a `PermissionRequested` handler on `webview`'s underlying
/// `ICoreWebView2` (reached via `Webview::with_webview`'s
/// `PlatformWebview::controller().CoreWebView2()`, per design.md §2.2.11)
/// that sets `COREWEBVIEW2_PERMISSION_STATE_DENY` for
/// `COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS` and leaves every other
/// permission kind untouched (its default state).
///
/// `Webview::with_webview` queues its closure onto the platform's own
/// event loop and can return before the closure has run, so failures
/// *inside* the closure (reading `ICoreWebView2` off the controller,
/// registering the handler, or reading a request's `PermissionKind`) are
/// only logged (`tracing::error!`) rather than returned — by the time they
/// would happen there is no caller left to hand a `Result` back to. The
/// `Err` this function itself returns is only `with_webview`'s own
/// dispatch failure (e.g. the webview already closed).
pub fn deny_notification_permission(webview: &Webview<Wry>) -> tauri::Result<()> {
    webview.with_webview(|platform_webview: PlatformWebview| {
        let controller = platform_webview.controller();
        let core_webview2 = match unsafe { controller.CoreWebView2() } {
            Ok(core_webview2) => core_webview2,
            Err(err) => {
                tracing::error!("webview2: could not get ICoreWebView2 from controller: {err}");
                return;
            }
        };

        let handler = PermissionRequestedEventHandler::create(Box::new(move |_sender, args| {
            let Some(args) = args else {
                return Ok(());
            };

            let mut kind = COREWEBVIEW2_PERMISSION_KIND_UNKNOWN_PERMISSION;
            unsafe { args.PermissionKind(&mut kind) }?;

            if should_deny(kind) {
                unsafe { args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY) }?;
            }
            // Every other permission kind is left alone: no `SetState`
            // call, so WebView2 falls back to its own default handling
            // (design.md §2.2.11: "leaves every other permission at the
            // default").

            Ok(())
        }));

        let mut token: i64 = 0;
        if let Err(err) = unsafe { core_webview2.add_PermissionRequested(&handler, &mut token) } {
            tracing::error!("webview2: add_PermissionRequested failed: {err}");
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PERMISSION_KIND_CAMERA, COREWEBVIEW2_PERMISSION_KIND_GEOLOCATION,
        COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
    };

    #[test]
    fn denies_notifications() {
        assert!(should_deny(COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS));
    }

    #[test]
    fn leaves_every_other_kind_untouched() {
        assert!(!should_deny(
            COREWEBVIEW2_PERMISSION_KIND_UNKNOWN_PERMISSION
        ));
        assert!(!should_deny(COREWEBVIEW2_PERMISSION_KIND_CAMERA));
        assert!(!should_deny(COREWEBVIEW2_PERMISSION_KIND_MICROPHONE));
        assert!(!should_deny(COREWEBVIEW2_PERMISSION_KIND_GEOLOCATION));
    }
}
