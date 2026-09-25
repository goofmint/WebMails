//! Platform-specific integrations that do not belong in `host` (design.md
//! §2.2.11): today, macOS's App Nap assertion. A future task adds
//! Windows' `PermissionRequested` handler here (`webview2.rs`).

pub mod app_nap;
