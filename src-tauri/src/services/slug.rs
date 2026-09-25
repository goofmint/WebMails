//! Generates a valid, unique [`ServiceId`] from a service's display name
//! (design.md §2.2.5: "slugify the name to `[a-z0-9-]`, then append `-2`,
//! `-3`, … on collision").
//!
//! Pure: no Tauri, no I/O, no access to the live [`crate::config::Config`].
//! The set of already-used ids is supplied by the caller (typically
//! `Config.services`), never read from anywhere in this module.

use crate::config::ServiceId;

/// The maximum length of a generated id, including any `-N` collision
/// suffix — matches [`ServiceId`]'s own limit
/// (`config::model::ID_PATTERN_DESCRIPTION`, `[a-z0-9-]{1,48}`).
const MAX_LEN: usize = 48;

/// Base slug used when `name` has no ASCII alphanumeric characters at all
/// (e.g. it is empty, punctuation-only, or entirely non-ASCII). design.md
/// §2.2.5 does not name a fallback for this case; this module documents
/// its own choice as part of the algorithm: fall back to this fixed base,
/// and let ordinary collision handling number it (`service-2`,
/// `service-3`, …) if it is already taken.
const FALLBACK_BASE: &str = "service";

/// Turns `name` into a valid `[a-z0-9-]{1,48}` [`ServiceId`], unique
/// against `existing`.
///
/// Algorithm:
/// 1. ASCII-lowercase `name` (a non-ASCII character is left as-is here,
///    then dropped in step 2, since it always falls outside `[a-z0-9]`).
/// 2. Replace every run of characters outside `[a-z0-9]` with a single
///    hyphen.
/// 3. Trim leading/trailing hyphens.
/// 4. If nothing is left, use [`FALLBACK_BASE`] (`"service"`).
/// 5. Truncate to [`MAX_LEN`] characters, trimming a trailing hyphen the
///    truncation may have exposed.
/// 6. If the result collides with an id already in `existing`, try `-2`,
///    `-3`, … — truncating the base further so the suffixed id still fits
///    within [`MAX_LEN`] — until one does not collide.
///
/// The result always passes [`ServiceId::new`]; this function never
/// returns an error.
pub fn slugify(name: &str, existing: &[ServiceId]) -> ServiceId {
    let base = base_slug(name);

    if let Some(id) = try_candidate(base.clone(), existing) {
        return id;
    }

    let mut suffix_n: u32 = 2;
    loop {
        let suffix = format!("-{suffix_n}");
        let max_base_len = MAX_LEN.saturating_sub(suffix.chars().count());
        let truncated_base = truncate_base(&base, max_base_len);
        let candidate = format!("{truncated_base}{suffix}");
        if let Some(id) = try_candidate(candidate, existing) {
            return id;
        }
        suffix_n += 1;
    }
}

/// Accepts `candidate` as the result only if it does not collide with
/// `existing` and passes [`ServiceId::new`]. `base_slug`/`truncate_base`
/// only ever produce strings already satisfying `[a-z0-9-]{1,48}`, so the
/// validation re-check here never actually rejects anything in practice —
/// but going through it (instead of assuming success) means this module
/// never needs `unwrap`/`expect`/`unreachable!` to turn a `Result` it is
/// "sure" about into a plain `ServiceId`; an unexpectedly invalid
/// candidate is just treated as another collision and the caller moves on
/// to the next suffix.
fn try_candidate(candidate: String, existing: &[ServiceId]) -> Option<ServiceId> {
    if collides(&candidate, existing) {
        return None;
    }
    ServiceId::new(candidate).ok()
}

fn collides(candidate: &str, existing: &[ServiceId]) -> bool {
    existing.iter().any(|id| id.as_str() == candidate)
}

/// Lowercases, collapses runs of non-`[a-z0-9]` characters to single
/// hyphens, trims edge hyphens, falls back to [`FALLBACK_BASE`] if that
/// leaves nothing, then truncates to [`MAX_LEN`].
fn base_slug(name: &str) -> String {
    let lowered = name.to_ascii_lowercase();

    let mut slug = String::with_capacity(lowered.len());
    let mut last_was_hyphen = true; // suppresses a leading hyphen
    for c in lowered.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            slug.push(c);
            last_was_hyphen = false;
        } else if !last_was_hyphen {
            slug.push('-');
            last_was_hyphen = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }

    if slug.is_empty() {
        slug = FALLBACK_BASE.to_string();
    }

    truncate_base(&slug, MAX_LEN)
}

/// Truncates `slug` to at most `max_len` characters, trimming a trailing
/// hyphen the cut may have exposed. `max_len` of `0` yields an empty
/// string (only reachable in `slugify`'s suffix loop, when the base has
/// no room left at all — the suffix alone, e.g. `"-2"`, is still a valid
/// id under `[a-z0-9-]{1,48}`).
fn truncate_base(slug: &str, max_len: usize) -> String {
    let truncated: String = slug.chars().take(max_len).collect();
    truncated.trim_end_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: &str) -> ServiceId {
        ServiceId::new(value).expect("valid id")
    }

    #[test]
    fn normal_name_lowercases_and_hyphenates() {
        assert_eq!(slugify("Gmail Personal", &[]).as_str(), "gmail-personal");
    }

    #[test]
    fn all_uppercase_is_lowercased() {
        assert_eq!(slugify("ICLOUD", &[]).as_str(), "icloud");
    }

    #[test]
    fn spaces_and_symbols_collapse_to_single_hyphens() {
        assert_eq!(slugify("Hello,   World!!!", &[]).as_str(), "hello-world");
    }

    #[test]
    fn leading_and_trailing_symbols_are_trimmed() {
        assert_eq!(slugify("  -- Gmail -- ", &[]).as_str(), "gmail");
    }

    #[test]
    fn digits_are_kept() {
        assert_eq!(slugify("Gmail 2", &[]).as_str(), "gmail-2");
    }

    #[test]
    fn non_ascii_only_falls_back_to_service() {
        assert_eq!(slugify("日本語", &[]).as_str(), "service");
    }

    #[test]
    fn empty_name_falls_back_to_service() {
        assert_eq!(slugify("", &[]).as_str(), "service");
    }

    #[test]
    fn punctuation_only_falls_back_to_service() {
        assert_eq!(slugify("!!!___...", &[]).as_str(), "service");
    }

    #[test]
    fn collision_appends_dash_2() {
        let existing = [id("gmail")];
        assert_eq!(slugify("Gmail", &existing).as_str(), "gmail-2");
    }

    #[test]
    fn multiple_collisions_increment_the_suffix() {
        let existing = [id("gmail"), id("gmail-2"), id("gmail-3")];
        assert_eq!(slugify("Gmail", &existing).as_str(), "gmail-4");
    }

    #[test]
    fn fallback_base_collision_is_also_numbered() {
        let existing = [id("service")];
        assert_eq!(slugify("!!!", &existing).as_str(), "service-2");
    }

    #[test]
    fn exactly_max_length_is_unchanged() {
        let name = "a".repeat(MAX_LEN);
        let slug = slugify(&name, &[]);
        assert_eq!(slug.as_str(), name);
        assert_eq!(slug.as_str().chars().count(), MAX_LEN);
    }

    #[test]
    fn over_max_length_is_truncated() {
        let name = "a".repeat(MAX_LEN + 10);
        let slug = slugify(&name, &[]);
        assert_eq!(slug.as_str().chars().count(), MAX_LEN);
        assert_eq!(slug.as_str(), "a".repeat(MAX_LEN));
    }

    #[test]
    fn truncation_trims_a_trailing_hyphen_exposed_by_the_cut() {
        // 47 'a's followed by a hyphen then more text: cutting at 48 chars
        // lands exactly on the hyphen, which must not survive.
        let name = format!("{}-rest-of-the-name", "a".repeat(47));
        let slug = slugify(&name, &[]);
        assert_eq!(slug.as_str(), "a".repeat(47));
    }

    #[test]
    fn suffixed_id_at_max_length_truncates_the_base_to_make_room() {
        let long_name = "a".repeat(MAX_LEN);
        let existing = [id(&long_name)];
        let slug = slugify(&long_name, &existing);
        assert_eq!(slug.as_str(), format!("{}-2", "a".repeat(MAX_LEN - 2)));
        assert_eq!(slug.as_str().chars().count(), MAX_LEN);
    }

    #[test]
    fn suffixed_id_stays_within_max_length_for_a_double_digit_suffix() {
        let long_name = "a".repeat(MAX_LEN);
        let mut existing = vec![id(&long_name)];
        for n in 2..=10 {
            let suffix = format!("-{n}");
            let base_len = MAX_LEN - suffix.chars().count();
            existing.push(id(&format!("{}{suffix}", "a".repeat(base_len))));
        }
        let slug = slugify(&long_name, &existing);
        assert_eq!(slug.as_str(), format!("{}-11", "a".repeat(MAX_LEN - 3)));
        assert_eq!(slug.as_str().chars().count(), MAX_LEN);
    }

    #[test]
    fn every_generated_id_passes_service_id_validation() {
        let cases: &[(&str, &[ServiceId])] = &[
            ("Gmail Personal", &[]),
            ("日本語", &[]),
            ("", &[]),
            ("Gmail", &[]),
        ];
        for (name, existing) in cases {
            let slug = slugify(name, existing);
            assert!(ServiceId::new(slug.as_str()).is_ok());
        }
    }
}
