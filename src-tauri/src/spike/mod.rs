//! SP5 spike harness — notification click crate comparison.
//!
//! docs/spikes/SP5.md, tasks.md Task 4.1. This whole module tree only
//! compiles when at least one `sp5-*` Cargo feature is enabled (see
//! `src-tauri/Cargo.toml`), and is never called from production code paths.
//! It is throwaway: built on `spike/sp5-notifications` only, not merged.

pub mod common;

#[cfg(feature = "sp5-baseline")]
pub mod sp5_baseline;
#[cfg(feature = "sp5-notify-rust")]
pub mod sp5_notify_rust;
#[cfg(feature = "sp5-user-notify")]
pub mod sp5_user_notify;

use tauri::App;

/// Wires up whichever SP5 candidate(s) were compiled in. Called once from
/// `eluma_lib::run`'s `.setup()` closure.
pub fn setup(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "sp5-baseline")]
    sp5_baseline::setup(app)?;
    #[cfg(feature = "sp5-notify-rust")]
    sp5_notify_rust::setup(app)?;
    #[cfg(feature = "sp5-user-notify")]
    sp5_user_notify::setup(app)?;

    // Silence "unused variable" when this function is compiled (i.e. at
    // least one sp5-* feature is on) but, hypothetically, none of the arms
    // above end up compiled — cannot happen given the module's own #[cfg]
    // gate in lib.rs, but keeps this function's signature stable either way.
    let _ = app;

    Ok(())
}
