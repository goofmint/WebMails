//! The `State` model persisted to `state.json` (design.md §2.2.2, SPEC.md
//! §13).

use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

/// Maximum number of seen message ids retained per service, in [`SeenRing`]
/// (design.md §2.2.2; eviction logic itself belongs to task 4.4's
/// `notify::seen`).
pub const SEEN_RING_CAPACITY: usize = 500;

pub use crate::config::{ProfileName, ServiceId};
pub use crate::profile::ProfileKey;

/// Persisted application state (`state.json`).
///
/// Every field is required in the file: the app always writes all three, so
/// a `state.json` missing one of them is treated as corrupt
/// (`AppError::State`), never silently filled in with a default (no
/// `#[serde(default)]` — see project rules). A *missing file* is a
/// different, expected case: `state::load` returns [`State::empty`] rather
/// than erroring when the file does not exist yet (design.md §2.2.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub profiles: BTreeMap<ProfileKey, Uuid>,
    pub seen: BTreeMap<ServiceId, SeenRing>,
    pub staleness: BTreeMap<ServiceId, StalenessStats>,
}

impl State {
    /// The state for a fresh install: no profiles, no seen ids, no
    /// staleness history yet. Used when no `state.json` exists (first
    /// launch); deliberately not `Default`, so that call sites choose this
    /// explicitly rather than by inference.
    pub fn empty() -> Self {
        State {
            profiles: BTreeMap::new(),
            seen: BTreeMap::new(),
            staleness: BTreeMap::new(),
        }
    }
}

/// Ring buffer of seen message ids for one service, capped at
/// [`SEEN_RING_CAPACITY`] (design.md §2.2.2).
///
/// This task defines only the storage type; insertion and eviction belong to
/// task 4.4's `notify::seen` module.
///
/// Serialization is transparent (just the inner array), but deserialization
/// rejects a `state.json` whose ring already holds more than
/// [`SEEN_RING_CAPACITY`] entries: that is corrupt data, not something to
/// silently truncate (no fallback defaults — see project rules), so it
/// surfaces as a serde error and `state::load` reports it as
/// `AppError::State`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct SeenRing(pub VecDeque<String>);

impl<'de> Deserialize<'de> for SeenRing {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let inner = VecDeque::<String>::deserialize(deserializer)?;
        if inner.len() > SEEN_RING_CAPACITY {
            return Err(serde::de::Error::custom(format!(
                "seen ring has {} entries, exceeding capacity of {SEEN_RING_CAPACITY}",
                inner.len()
            )));
        }
        Ok(SeenRing(inner))
    }
}

/// Liveness/staleness bookkeeping for one service (design.md §9.4, task
/// 3.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StalenessStats {
    /// How many times this service has been marked `Stale` (design.md
    /// §2.2.8: incremented on every `MarkStale` action).
    pub count: u32,
    /// Unix epoch milliseconds of the moment `count` was last
    /// incremented — i.e. the last time this service was marked `Stale`
    /// (design.md §2.2.8), **not** the last time a report was received.
    /// `None` until the first stale episode. Task 3.3's `get_diagnostics`
    /// reports this as a service's "last stale time"; it computes "last
    /// report age" from a separate, unpersisted source (the liveness
    /// machine's own last-report timing), never from this field.
    pub last_at: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_state_has_no_entries() {
        let state = State::empty();
        assert!(state.profiles.is_empty());
        assert!(state.seen.is_empty());
        assert!(state.staleness.is_empty());
    }

    #[test]
    fn seen_ring_round_trips_through_json() {
        let mut ring = SeenRing::default();
        ring.0.push_back("a".to_string());
        ring.0.push_back("b".to_string());

        let json = serde_json::to_string(&ring).expect("serialize");
        assert_eq!(json, r#"["a","b"]"#);

        let back: SeenRing = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, ring);
    }

    #[test]
    fn seen_ring_at_capacity_deserializes_ok() {
        let ids: Vec<String> = (0..SEEN_RING_CAPACITY).map(|i| i.to_string()).collect();
        let json = serde_json::to_string(&ids).expect("serialize");

        let ring: SeenRing = serde_json::from_str(&json).expect("deserialize at capacity");
        assert_eq!(ring.0.len(), SEEN_RING_CAPACITY);
    }

    #[test]
    fn seen_ring_over_capacity_is_deserialization_error() {
        let ids: Vec<String> = (0..=SEEN_RING_CAPACITY).map(|i| i.to_string()).collect();
        let json = serde_json::to_string(&ids).expect("serialize");

        let err = serde_json::from_str::<SeenRing>(&json).expect_err("should fail");
        assert!(err.to_string().contains("exceeding capacity"));
    }

    #[test]
    fn staleness_stats_last_at_accepts_explicit_null_or_a_value() {
        // `last_at` is `Option<u64>`, not a fallback default: `None` is a
        // meaningful value ("no report yet"), and serde's derive treats
        // `Option<T>` fields as implicitly present-or-absent regardless of
        // `#[serde(default)]`. The app always writes the key explicitly, so
        // this only affects how permissive `load`/`StateStore::open` are on
        // a hand-edited file, not the no-fallback rule for `State`'s own
        // top-level fields (profiles/seen/staleness — see store.rs tests).
        let explicit_null = serde_json::from_str::<StalenessStats>(r#"{"count":1,"last_at":null}"#)
            .expect("deserialize");
        assert_eq!(
            explicit_null,
            StalenessStats {
                count: 1,
                last_at: None
            }
        );

        let with_value = serde_json::from_str::<StalenessStats>(r#"{"count":1,"last_at":42}"#)
            .expect("deserialize");
        assert_eq!(
            with_value,
            StalenessStats {
                count: 1,
                last_at: Some(42)
            }
        );
    }

    #[test]
    fn staleness_stats_requires_count_key() {
        // Unlike `last_at`, `count: u32` has no serde special-casing: a
        // missing `count` key is a genuine parse error.
        let missing = serde_json::from_str::<StalenessStats>(r#"{"last_at":null}"#);
        assert!(missing.is_err());
    }
}
