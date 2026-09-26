//! Liveness monitoring (design.md §2.2.8, SPEC.md §9.4; Task 3.2): the
//! "safety net" for a throttled or suspended service webview. A service's
//! `unread` badge is only as trustworthy as its last report is recent —
//! this module is what notices a report has stopped arriving and does
//! something about it.
//!
//! - [`machine`] is the pure state machine: given the current time and
//!   every service's [`crate::unread::ServiceStatus`], it decides which
//!   services have gone stale and what to do, without touching Tauri or
//!   performing any I/O. It is exhaustively unit-tested with an injected
//!   `now_ms` — no real waiting.
//! - [`runtime`] is the impure wiring: a `tokio::time::interval` task that
//!   calls [`machine::LivenessMachine::tick`] every 30s and applies the
//!   actions it returns through [`crate::services::ServiceManager`] (
//!   reload/recreate) and the shared `unread::StatusStore` (`MarkStale` →
//!   `Stale`), plus [`crate::state::StateStore`] for the staleness
//!   counters `state.json` persists (design.md §2.2.2's `staleness`
//!   field). [`runtime::LivenessRuntime::record_report`] is the one other
//!   entry point: `agent_bridge::report_unread` calls it immediately after
//!   a report validates, using the same Rust-side clock the tick loop
//!   uses (design.md §2.2.6: "Liveness uses the time Rust receives the
//!   report" — never the report's own `observedAt`).

mod machine;
mod runtime;

pub use machine::{
    Action, LivenessMachine, GRACE, MISSED_REPORTS_THRESHOLD, TICK, TICKS_AFTER_RELOAD,
};
pub use runtime::{Clock, LivenessRuntime, SystemClock};
