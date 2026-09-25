//! SP2 identifier derivation (design §2.2.3 `profile`). Kept separate
//! from `sp2.rs` because it is pure and easy to unit test on its own.

use uuid::Uuid;

/// Fixed project namespace UUID for this spike, per design §2.2.3 ("UUID =
/// UUIDv5 over the key, using a fixed project namespace UUID (a constant
/// in code)"). Generated once as an arbitrary UUIDv4 and hard-coded here;
/// it must never change once data stores exist under it, or every
/// derived UUID (and the data stores keyed by them) would change too
/// (design §10: "never use a zero or placeholder value", tauri#12843).
///
/// This is a namespace for the *spike* only. The real implementation
/// needs its own project-wide constant, chosen the same way (see
/// `docs/spikes/SP2.md` "Configuration").
pub const SPIKE_NAMESPACE: Uuid = Uuid::from_bytes([
    0x6f, 0x1a, 0x3c, 0x2e, 0x9b, 0x77, 0x4d, 0x0a, 0x8e, 0x51, 0x2c, 0x66, 0x0b, 0x4f, 0x9a, 0x13,
]);

/// Derives a stable UUIDv5 for a profile key (e.g. `"isolated:a"`),
/// mirroring design §2.2.3's `profile::resolve`. Never uses `new_v4` —
/// the whole point of this spike is a reproducible identifier across
/// restarts.
pub fn derive(key: &str) -> Uuid {
    Uuid::new_v5(&SPIKE_NAMESPACE, key.as_bytes())
}

/// The Windows `data_directory` for a derived UUID, under
/// `{app_data_dir}/webview/<uuid>` (design §2.2.3). Only called from
/// `sp2.rs`'s `#[cfg(windows)]` branch, so non-Windows builds would
/// otherwise flag it as dead code.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn windows_data_directory(app_data_dir: &std::path::Path, uuid: Uuid) -> std::path::PathBuf {
    app_data_dir.join("webview").join(uuid.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_is_stable_and_v5() {
        let a1 = derive("isolated:a");
        let a2 = derive("isolated:a");
        assert_eq!(a1, a2, "the same key must always derive the same UUID");
        assert_eq!(a1.get_version_num(), 5);
    }

    #[test]
    fn different_keys_derive_different_uuids() {
        assert_ne!(derive("isolated:a"), derive("isolated:b"));
    }

    #[test]
    fn windows_path_uses_webview_subdir_and_uuid_string() {
        let uuid = derive("isolated:a");
        let path = windows_data_directory(std::path::Path::new("/data"), uuid);
        assert_eq!(
            path,
            std::path::PathBuf::from(format!("/data/webview/{uuid}"))
        );
    }
}
