//! Profile key derivation, stable-UUID resolution, and the per-OS webview
//! data store / directory backend (design.md §2.2.3, §2.2.4, §10; SPEC.md
//! §5).
//!
//! [`resolve`] turns a service's `profile` name into a stable UUID,
//! recording newly minted ones in `state.profiles` (keyed by
//! [`ProfileKey`]). [`ProfileBackend`] then applies that UUID to a webview
//! being built and, later, removes its on-disk data — macOS and Windows
//! each get their own [`PlatformProfileBackend`], selected at compile time.

mod backend;
mod key;

#[cfg(any(target_os = "macos", windows))]
pub use backend::PlatformProfileBackend;
pub use backend::{remove_webview_data_dir, webview_data_dir, ProfileBackend};
pub use key::{resolve, ProfileKey};
