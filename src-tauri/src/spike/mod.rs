//! Throwaway spike harness for tasks 0.3–0.6 (GitHub issues #3–#6). Never
//! merged to `main` — see `SPIKE.md` at the worktree root.
//!
//! The mode is selected once, at process start, via the `ELUMA_SPIKE`
//! environment variable. There is no fallback default (project rule): an
//! unset or unrecognised value is a hard error, not a silently-chosen mode.
//!
//! `Window::add_child` / `WebviewBuilder` require the `unstable` Cargo
//! feature (design.md §10: "Keep every call to them inside
//! `host/multiwebview.rs`" — here, inside this `spike` module tree, since
//! this branch has no production host code).

pub mod common;
pub mod data_store;
pub mod sp1;
pub mod sp2;
pub mod sp3;
pub mod sp4;

/// Which spike this run is exercising. Each `sp1`..`sp4` module names its
/// own mode string directly where it needs one (e.g. in
/// `common::shell_init_script`'s `mode` argument), so this enum only
/// drives the dispatch in `setup` below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpikeMode {
    Sp1,
    Sp2,
    Sp3,
    Sp4,
}

/// Reads and validates `ELUMA_SPIKE`. No fallback: an unset or
/// unrecognised value panics with a clear message instead of silently
/// picking a mode.
pub fn read_mode() -> SpikeMode {
    let raw = std::env::var("ELUMA_SPIKE")
        .expect("ELUMA_SPIKE must be set to one of: sp1, sp2, sp3, sp4");
    match raw.as_str() {
        "sp1" => SpikeMode::Sp1,
        "sp2" => SpikeMode::Sp2,
        "sp3" => SpikeMode::Sp3,
        "sp4" => SpikeMode::Sp4,
        other => {
            panic!("ELUMA_SPIKE has unrecognised value '{other}'; expected sp1, sp2, sp3, or sp4")
        }
    }
}

/// Dispatches to the mode-specific setup, chosen by `ELUMA_SPIKE`. Each
/// mode builds its own `main` `Window` (via `WindowBuilder`, never through
/// `tauri.conf.json` — see `src-tauri/tauri.conf.json`'s empty
/// `app.windows`) and whatever children the mode needs.
///
/// Returns `Box<dyn std::error::Error>`, matching
/// `tauri::Builder::setup`'s own closure signature exactly (confirmed in
/// `tauri-2.11.6/src/app.rs`), so `lib.rs` can call this directly with `?`
/// instead of converting error types at the boundary.
pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    match read_mode() {
        SpikeMode::Sp1 => sp1::setup(app),
        SpikeMode::Sp2 => sp2::setup(app),
        SpikeMode::Sp3 => sp3::setup(app),
        SpikeMode::Sp4 => sp4::setup(app),
    }
}
