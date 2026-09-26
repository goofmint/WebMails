//! Per-service unread status store (design.md §2.2.7): [`ServiceStatus`],
//! [`AttentionReason`], [`StatusChanged`], and the [`StatusStore`] state
//! machine that computes transitions from validated reports and
//! `on_page_load` origin checks, tracking only whether a mutation
//! actually changed a service's status.
//!
//! Nothing here depends on Tauri. [`emit_if_changed`] is the one seam
//! [`crate::services::ServiceManager`] plugs an `emit_to("shell",
//! "status-changed", …)` closure into, so this module never needs an
//! `AppHandle` and stays fully unit-tested without one (see `status.rs`
//! and `store.rs`'s own `#[cfg(test)]` modules).

mod status;
mod store;

pub use status::{is_off_origin, AttentionReason, ServiceStatus, StatusChanged};
pub use store::{emit_if_changed, StatusStore};
