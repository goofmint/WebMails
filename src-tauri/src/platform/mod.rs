//! Platform-specific integrations that do not belong in `host` (design.md
//! §2.2.11):
//!
//! - **`app_nap`** (macOS): the App Nap assertion held while services exist.
//! - **`webview2`** (Windows only): denies the WebView2 `NOTIFICATIONS`
//!   permission on every service webview (Task 4.3).

pub mod app_nap;
#[cfg(windows)]
pub mod webview2;
