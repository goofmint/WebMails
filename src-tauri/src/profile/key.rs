//! Profile key derivation and stable-UUID resolution (design.md §2.2.3,
//! §10; SPEC.md §5).
//!
//! A service's `profile` name (`"default"`, `"isolated"`, or an arbitrary
//! named profile) is not itself the identifier a webview data store uses.
//! [`resolve`] first derives a canonical [`ProfileKey`] from `(name,
//! service)` — folding the two reserved names into their own key shapes so
//! that, for example, every service naming `"isolated"` still gets its own
//! private key even though they all share the same literal profile name —
//! and then looks up (or computes and records) a stable UUID for that key
//! in `state.profiles`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use crate::config::{ProfileName, ServiceId};
use crate::state::State;

/// The project's fixed namespace UUID for deriving profile data-store ids
/// with UUIDv5 (design.md §10, SPEC.md §5.2).
///
/// Generated once with `uuidgen` (a random UUIDv4) and hard-coded here —
/// **never** the nil UUID and never the SP2 spike harness's namespace
/// (design.md §10 flags tauri#12843, where one reported crash cause was an
/// invalid identifier such as `[0u8; 16]`).
///
/// **Do not change this value.** Every profile UUID this module produces is
/// `Uuid::new_v5(&PROFILE_NAMESPACE, key.to_string().as_bytes())`; changing
/// the namespace changes every derived UUID, which orphans every existing
/// on-disk webview data store (`~/Library/WebKit/WebsiteDataStore/<uuid>/`
/// on macOS, `<data_dir>/webview/<uuid>/` on Windows) — users would lose
/// every signed-in session the next time the app starts.
const PROFILE_NAMESPACE: Uuid = Uuid::from_bytes([
    0x21, 0x5e, 0x6f, 0x6b, 0x39, 0x7e, 0x46, 0x25, 0xbe, 0x56, 0x4f, 0x02, 0x42, 0x05, 0x28, 0x46,
]);

/// The canonical `state.profiles` key derived from a service's profile name
/// and, when relevant, its service id (design.md §2.2.3).
///
/// String form (used for [`Display`](fmt::Display), [`FromStr`], and
/// `state.json` map-key (de)serialization):
/// - `"default"` — the profile shared by every service naming `"default"`.
/// - `"isolated:<service_id>"` — a private profile for exactly one
///   service.
/// - `"named:<name>"` — a named profile, shareable between every service
///   that names it. `<name>` is never the literal `default` or `isolated`
///   (those are always the two variants above, never a `Named`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProfileKey {
    Default,
    Isolated(ServiceId),
    Named(ProfileName),
}

impl fmt::Display for ProfileKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProfileKey::Default => f.write_str("default"),
            ProfileKey::Isolated(id) => write!(f, "isolated:{id}"),
            ProfileKey::Named(name) => write!(f, "named:{name}"),
        }
    }
}

impl FromStr for ProfileKey {
    type Err = String;

    /// Parses a `state.json` `profiles` map key back into a [`ProfileKey`],
    /// the inverse of `Display`. Rejects `"named:default"` and
    /// `"named:isolated"`: those names are always represented by the
    /// [`ProfileKey::Default`] / [`ProfileKey::Isolated`] variants, so a
    /// `named:` form of either is not a value this module ever produces,
    /// and a hand-edited `state.json` containing one is corrupt data, not
    /// something to silently reinterpret.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value == "default" {
            return Ok(ProfileKey::Default);
        }
        if let Some(id) = value.strip_prefix("isolated:") {
            let id = ServiceId::new(id)
                .map_err(|reason| format!("invalid profile key `{value}`: {reason}"))?;
            return Ok(ProfileKey::Isolated(id));
        }
        if let Some(name) = value.strip_prefix("named:") {
            if name == "default" || name == "isolated" {
                return Err(format!(
                    "invalid profile key `{value}`: `named:{name}` is reserved for \
                     `ProfileKey::{}`, never a named profile",
                    if name == "default" {
                        "Default"
                    } else {
                        "Isolated"
                    }
                ));
            }
            let name = ProfileName::new(name)
                .map_err(|reason| format!("invalid profile key `{value}`: {reason}"))?;
            return Ok(ProfileKey::Named(name));
        }
        Err(format!(
            "invalid profile key `{value}`: expected `default`, `isolated:<id>`, or \
             `named:<name>`"
        ))
    }
}

impl Serialize for ProfileKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

/// Deserialization (e.g. as a `state.json` map key) runs the same
/// validation as [`ProfileKey::from_str`].
impl<'de> Deserialize<'de> for ProfileKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

/// Derives `(name, service)`'s canonical [`ProfileKey`] (design.md §2.2.3),
/// without touching `state`.
fn derive_key(name: &ProfileName, service: &ServiceId) -> ProfileKey {
    match name.as_str() {
        "default" => ProfileKey::Default,
        "isolated" => ProfileKey::Isolated(service.clone()),
        _ => ProfileKey::Named(name.clone()),
    }
}

/// Resolves `(name, service)` to its webview data-store UUID.
///
/// Derives the canonical key (design.md §2.2.3) and returns the existing
/// `state.profiles` entry for it, if there is one. Otherwise computes a
/// UUIDv5 over the key's string form (using [`PROFILE_NAMESPACE`]), records
/// it in `state.profiles`, and returns it. An existing entry always wins:
/// once a profile has a UUID, this function never recomputes or overwrites
/// it, so a profile keeps the same on-disk data store for the life of the
/// install.
///
/// This function only mutates the in-memory `state` it is given — it does
/// not save anything to disk itself. The intended call site is inside
/// [`crate::state::StateStore::update`], so a freshly minted UUID is
/// captured by that update's dirty flag and durably saved within the
/// store's debounce window, e.g.:
///
/// ```ignore
/// let uuid = store.update(|state| profile::resolve(&profile_name, &service_id, state))?;
/// ```
pub fn resolve(name: &ProfileName, service: &ServiceId, state: &mut State) -> Uuid {
    let key = derive_key(name, service);
    if let Some(uuid) = state.profiles.get(&key) {
        return *uuid;
    }
    let uuid = Uuid::new_v5(&PROFILE_NAMESPACE, key.to_string().as_bytes());
    state.profiles.insert(key, uuid);
    uuid
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn profile_name(value: &str) -> ProfileName {
        ProfileName::new(value).expect("valid profile name")
    }

    fn service_id(value: &str) -> ServiceId {
        ServiceId::new(value).expect("valid service id")
    }

    // --- ProfileKey: Display / FromStr round trip -----------------------

    #[test]
    fn default_key_round_trips() {
        let key = ProfileKey::Default;
        assert_eq!(key.to_string(), "default");
        assert_eq!("default".parse::<ProfileKey>().unwrap(), key);
    }

    #[test]
    fn isolated_key_round_trips() {
        let key = ProfileKey::Isolated(service_id("svc-1"));
        assert_eq!(key.to_string(), "isolated:svc-1");
        assert_eq!("isolated:svc-1".parse::<ProfileKey>().unwrap(), key);
    }

    #[test]
    fn named_key_round_trips() {
        let key = ProfileKey::Named(profile_name("work"));
        assert_eq!(key.to_string(), "named:work");
        assert_eq!("named:work".parse::<ProfileKey>().unwrap(), key);
    }

    #[test]
    fn named_default_is_rejected() {
        assert!("named:default".parse::<ProfileKey>().is_err());
    }

    #[test]
    fn named_isolated_is_rejected() {
        assert!("named:isolated".parse::<ProfileKey>().is_err());
    }

    #[test]
    fn isolated_key_rejects_invalid_service_id() {
        assert!("isolated:Not Valid".parse::<ProfileKey>().is_err());
    }

    #[test]
    fn named_key_rejects_invalid_profile_name() {
        assert!("named:Not Valid".parse::<ProfileKey>().is_err());
    }

    #[test]
    fn unrecognized_prefix_is_rejected() {
        assert!("bogus:foo".parse::<ProfileKey>().is_err());
        assert!("".parse::<ProfileKey>().is_err());
    }

    #[test]
    fn profile_key_serializes_as_its_display_string() {
        let key = ProfileKey::Isolated(service_id("svc-1"));
        let json = serde_json::to_string(&key).expect("serialize");
        assert_eq!(json, "\"isolated:svc-1\"");
    }

    #[test]
    fn profile_key_deserializes_from_its_display_string() {
        let key: ProfileKey = serde_json::from_str("\"named:work\"").expect("deserialize");
        assert_eq!(key, ProfileKey::Named(profile_name("work")));
    }

    #[test]
    fn profile_key_deserialize_rejects_invalid_string() {
        let err = serde_json::from_str::<ProfileKey>("\"named:default\"");
        assert!(err.is_err());
    }

    // --- resolve(): derivation, sharing/isolation ------------------------

    #[test]
    fn default_profile_name_derives_the_default_key() {
        let mut state = State::empty();
        resolve(&profile_name("default"), &service_id("svc-1"), &mut state);
        assert!(state.profiles.contains_key(&ProfileKey::Default));
    }

    #[test]
    fn isolated_profile_name_derives_a_per_service_key() {
        let mut state = State::empty();
        resolve(&profile_name("isolated"), &service_id("svc-1"), &mut state);
        assert!(state
            .profiles
            .contains_key(&ProfileKey::Isolated(service_id("svc-1"))));
    }

    #[test]
    fn named_profile_name_derives_a_named_key() {
        let mut state = State::empty();
        resolve(&profile_name("work"), &service_id("svc-1"), &mut state);
        assert!(state
            .profiles
            .contains_key(&ProfileKey::Named(profile_name("work"))));
    }

    #[test]
    fn two_services_naming_default_share_one_uuid() {
        let mut state = State::empty();
        let a = resolve(&profile_name("default"), &service_id("svc-a"), &mut state);
        let b = resolve(&profile_name("default"), &service_id("svc-b"), &mut state);
        assert_eq!(a, b);
        assert_eq!(state.profiles.len(), 1);
    }

    #[test]
    fn two_services_naming_isolated_get_different_uuids() {
        let mut state = State::empty();
        let a = resolve(&profile_name("isolated"), &service_id("svc-a"), &mut state);
        let b = resolve(&profile_name("isolated"), &service_id("svc-b"), &mut state);
        assert_ne!(a, b);
        assert_eq!(state.profiles.len(), 2);
    }

    #[test]
    fn two_services_naming_the_same_named_profile_share_one_uuid() {
        let mut state = State::empty();
        let a = resolve(&profile_name("work"), &service_id("svc-a"), &mut state);
        let b = resolve(&profile_name("work"), &service_id("svc-b"), &mut state);
        assert_eq!(a, b);
        assert_eq!(state.profiles.len(), 1);
    }

    // --- resolve(): determinism, UUID shape ------------------------------

    #[test]
    fn resolve_is_deterministic_across_independent_states() {
        let mut state1 = State::empty();
        let mut state2 = State::empty();
        let a = resolve(&profile_name("work"), &service_id("svc-1"), &mut state1);
        let b = resolve(&profile_name("work"), &service_id("svc-1"), &mut state2);
        assert_eq!(a, b);
    }

    #[test]
    fn resolve_produces_a_v5_non_nil_uuid() {
        let mut state = State::empty();
        let uuid = resolve(&profile_name("isolated"), &service_id("svc-1"), &mut state);
        assert_eq!(uuid.get_version_num(), 5);
        assert_ne!(uuid, Uuid::nil());
    }

    #[test]
    fn resolve_matches_a_known_fixed_value() {
        // Pinned so an accidental change to `PROFILE_NAMESPACE` or the key
        // format is caught immediately, rather than silently orphaning
        // every existing on-disk profile store (see its doc comment).
        let mut state = State::empty();
        let uuid = resolve(&profile_name("default"), &service_id("svc-1"), &mut state);
        assert_eq!(
            uuid,
            Uuid::new_v5(&PROFILE_NAMESPACE, b"default"),
            "PROFILE_NAMESPACE or the key format changed"
        );
    }

    // --- resolve(): existing value wins -----------------------------------

    #[test]
    fn resolve_returns_the_existing_recorded_uuid_unchanged() {
        let mut state = State::empty();
        let pinned = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        state.profiles.insert(ProfileKey::Default, pinned);

        let uuid = resolve(&profile_name("default"), &service_id("svc-1"), &mut state);
        assert_eq!(uuid, pinned);
        assert_eq!(state.profiles.len(), 1);
    }

    // --- State JSON round trip with ProfileKey keys -----------------------

    #[test]
    fn state_with_profile_keys_round_trips_through_json() {
        let mut profiles = BTreeMap::new();
        profiles.insert(
            ProfileKey::Default,
            Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
        );
        profiles.insert(
            ProfileKey::Isolated(service_id("svc-1")),
            Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
        );
        profiles.insert(
            ProfileKey::Named(profile_name("work")),
            Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap(),
        );
        let state = State {
            profiles,
            seen: BTreeMap::new(),
            staleness: BTreeMap::new(),
        };

        let json = serde_json::to_string(&state).expect("serialize");
        assert!(json.contains("\"default\""));
        assert!(json.contains("\"isolated:svc-1\""));
        assert!(json.contains("\"named:work\""));

        let back: State = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, state);
    }

    #[test]
    fn state_with_an_invalid_profile_key_fails_to_deserialize() {
        let json = r#"{"profiles":{"named:isolated":"11111111-1111-1111-1111-111111111111"},"seen":{},"staleness":{}}"#;
        let err = serde_json::from_str::<State>(json);
        assert!(err.is_err());
    }
}
