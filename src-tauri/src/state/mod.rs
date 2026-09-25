//! Application state (`state.json`): profile UUIDs, seen-message ids and
//! staleness counters (design.md §2.2.2, SPEC.md §13).

mod model;
mod store;

pub use model::{
    ProfileKey, ProfileName, SeenRing, ServiceId, StalenessStats, State, SEEN_RING_CAPACITY,
};
pub use store::{load, save_atomic, StateStore, SAVE_DEBOUNCE};
