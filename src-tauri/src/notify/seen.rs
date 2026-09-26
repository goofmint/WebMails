//! Pure ring operations over [`SeenRing`] (design.md §2.2.9): membership
//! checks and touch/insert-with-eviction on the plain `VecDeque<String>`
//! [`SeenRing`] wraps. Persisting the result to `state.json` is
//! `state::StateStore`'s job (task 1.4) — this module only defines
//! operations on the in-memory value; nothing here reads or writes a
//! file.

use std::collections::HashSet;

use crate::state::{SeenRing, SEEN_RING_CAPACITY};

/// Whether `id` is already present in `ring`.
pub fn contains(ring: &SeenRing, id: &str) -> bool {
    ring.0.iter().any(|seen| seen == id)
}

/// Inserts `id` into `ring`, dedupe-on-insert (design.md §2.2.9): an id
/// already present is moved to the back (touched) rather than
/// duplicated; a new id is appended at the back. If appending pushes the
/// ring over [`SEEN_RING_CAPACITY`], the oldest entry (the front) is
/// evicted — the ring never holds more than `SEEN_RING_CAPACITY` ids
/// after this returns.
pub fn insert(ring: &mut SeenRing, id: &str) {
    if let Some(pos) = ring.0.iter().position(|seen| seen == id) {
        ring.0.remove(pos);
    }
    ring.0.push_back(id.to_string());
    while ring.0.len() > SEEN_RING_CAPACITY {
        ring.0.pop_front();
    }
}

/// Inserts every id in `ids`, in order, via [`insert`] — with duplicate
/// ids *within this call* processed only once each (design.md §2.2.9:
/// "the same report's repeated ids are handled once"), so a report that
/// lists the same id twice touches it a single time rather than moving
/// it to the back twice.
pub fn insert_all<'a>(ring: &mut SeenRing, ids: impl IntoIterator<Item = &'a str>) {
    let mut seen_this_call = HashSet::new();
    for id in ids {
        if seen_this_call.insert(id) {
            insert(ring, id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring_of(ids: impl IntoIterator<Item = &'static str>) -> SeenRing {
        SeenRing(ids.into_iter().map(str::to_string).collect())
    }

    fn as_vec(ring: &SeenRing) -> Vec<String> {
        ring.0.iter().cloned().collect()
    }

    #[test]
    fn contains_finds_a_present_id() {
        let ring = ring_of(["a", "b"]);
        assert!(contains(&ring, "a"));
    }

    #[test]
    fn contains_is_false_for_an_absent_id() {
        let ring = ring_of(["a", "b"]);
        assert!(!contains(&ring, "z"));
    }

    #[test]
    fn insert_appends_a_new_id_at_the_back() {
        let mut ring = ring_of(["a", "b"]);
        insert(&mut ring, "c");
        assert_eq!(as_vec(&ring), vec!["a", "b", "c"]);
    }

    #[test]
    fn insert_touches_an_existing_id_moving_it_to_the_back() {
        let mut ring = ring_of(["a", "b", "c"]);
        insert(&mut ring, "a");
        assert_eq!(as_vec(&ring), vec!["b", "c", "a"]);
    }

    #[test]
    fn insert_at_capacity_evicts_the_oldest_entry() {
        let ids: Vec<String> = (0..SEEN_RING_CAPACITY).map(|n| n.to_string()).collect();
        let mut ring = SeenRing(ids.into_iter().collect());

        insert(&mut ring, "new");

        assert_eq!(ring.0.len(), SEEN_RING_CAPACITY);
        assert_eq!(ring.0.front().map(String::as_str), Some("1"));
        assert_eq!(ring.0.back().map(String::as_str), Some("new"));
    }

    #[test]
    fn insert_touching_an_existing_id_never_grows_the_ring() {
        let ids: Vec<String> = (0..SEEN_RING_CAPACITY).map(|n| n.to_string()).collect();
        let mut ring = SeenRing(ids.into_iter().collect());

        insert(&mut ring, "0");

        assert_eq!(ring.0.len(), SEEN_RING_CAPACITY);
        assert_eq!(ring.0.back().map(String::as_str), Some("0"));
    }

    #[test]
    fn insert_all_dedupes_repeated_ids_within_one_call() {
        let mut ring = SeenRing::default();
        insert_all(&mut ring, ["a", "b", "a", "c"]);
        assert_eq!(as_vec(&ring), vec!["a", "b", "c"]);
    }

    #[test]
    fn insert_all_over_capacity_keeps_only_the_most_recent_ids_in_order() {
        let ids: Vec<String> = (0..SEEN_RING_CAPACITY + 5).map(|n| n.to_string()).collect();
        let mut ring = SeenRing::default();

        insert_all(&mut ring, ids.iter().map(String::as_str));

        assert_eq!(ring.0.len(), SEEN_RING_CAPACITY);
        let expected: Vec<String> = (5..SEEN_RING_CAPACITY + 5).map(|n| n.to_string()).collect();
        assert_eq!(as_vec(&ring), expected);
    }

    #[test]
    fn seen_insert_round_trips_through_state_store() {
        use tempfile::tempdir;

        use crate::config::ServiceId;
        use crate::state::{load, StateStore};

        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("state.json");
        let id = ServiceId::new("svc-1").expect("valid id");

        {
            let store = StateStore::open(path.clone()).expect("open");
            store
                .update(|state| {
                    let ring = state.seen.entry(id.clone()).or_default();
                    insert(ring, "m1");
                    insert(ring, "m2");
                    insert(ring, "m1"); // touch: moves to the back, no duplicate
                })
                .expect("update");
            store.flush().expect("flush");
            // Store dropped here; a fresh `load` below must see what was
            // flushed, not anything left only in the dropped store's
            // memory.
        }

        let reloaded = load(&path).expect("load");
        let ring = reloaded.seen.get(&id).expect("ring present after reload");
        assert_eq!(as_vec(ring), vec!["m2", "m1"]);
    }

    #[test]
    fn seen_rings_are_kept_separate_per_service_through_a_round_trip() {
        use tempfile::tempdir;

        use crate::config::ServiceId;
        use crate::state::{load, StateStore};

        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("state.json");
        let first = ServiceId::new("svc-1").expect("valid id");
        let second = ServiceId::new("svc-2").expect("valid id");

        {
            let store = StateStore::open(path.clone()).expect("open");
            store
                .update(|state| {
                    insert(state.seen.entry(first.clone()).or_default(), "a");
                    insert(state.seen.entry(second.clone()).or_default(), "b");
                })
                .expect("update");
            store.flush().expect("flush");
        }

        let reloaded = load(&path).expect("load");
        assert_eq!(
            as_vec(reloaded.seen.get(&first).expect("first ring")),
            vec!["a"]
        );
        assert_eq!(
            as_vec(reloaded.seen.get(&second).expect("second ring")),
            vec!["b"]
        );
    }
}
