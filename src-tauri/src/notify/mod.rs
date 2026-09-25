//! Seen-id ring and diff engine for unread-count notifications
//! (design.md §2.2.9).
//!
//! - `seen` holds pure ring operations (`contains`, `insert`,
//!   `insert_all`) over `state::SeenRing`.
//! - `diff` holds the pure `evaluate` decision function and its
//!   `ServiceNotifyState` in-memory baseline.
//!
//! Neither module touches Tauri, `StateStore`, or `report_unread`:
//! deciding what to do with an `evaluate` result — persisting newly-seen
//! ids into the ring and actually dispatching a notification — is task
//! 4.5's job (`dispatcher.rs`/`sink.rs`, not added by this task).
//!
//! Nothing outside this module's own tests calls into it yet, since that
//! wiring is task 4.5. Rather than fabricate a caller that doesn't
//! exist, `dead_code` and the resulting `unused_imports` on this
//! module's re-exports are allowed for this module only.
#![allow(dead_code, unused_imports)]

mod diff;
mod seen;

pub use diff::{evaluate, DiffOutcome, MessageRef, ServiceNotifyState};
