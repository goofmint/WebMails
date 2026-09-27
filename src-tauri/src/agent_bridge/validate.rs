//! Pure validation of an [`UnreadReportDto`] into a [`ValidReport`]
//! (design.md §2.2.6). Nothing here touches Tauri: [`validate`] takes the
//! DTO, the calling webview's label and a service-URL lookup closure, so
//! it is fully unit-testable without a running app. [`command::report_unread`]
//! (in `super::command`) is the only caller in production.
//!
//! [`ValidReport`] and [`ValidMessageRef`] can only be constructed by
//! [`validate`] succeeding: every field is private, and there is no
//! public constructor. This is the type-level guarantee that a
//! `report_unread` consumer (Task 2.3) will only ever see already-valid
//! data.
//!
//! Checks run in the order design.md §2.2.6 lists them, and the first
//! violation is returned — later checks never run once one fails.

use url::Url;

use crate::config::ServiceId;
use crate::host;
use crate::net_guard::is_disallowed_host;

use super::dto::UnreadReportDto;

/// `count` must be `null` or an integer no greater than this (design.md
/// §2.2.6).
const MAX_COUNT: i64 = 1_000_000;

/// At most this many `messages` per report (design.md §2.2.6).
const MAX_MESSAGES: usize = 100;

/// Every string field (message `id`/`from`/`subject`/`link`) is at most
/// this many Unicode scalar values (design.md §2.2.6).
const MAX_STRING_LEN: usize = 512;

/// `recipeId` is at most this many characters (design.md §2.2.6).
const MAX_RECIPE_ID_LEN: usize = 64;

/// At most this many `iconCandidates` per report (design.md §2.2.6).
const MAX_ICON_CANDIDATES: usize = 8;

/// Why a report was rejected (design.md §2.2.6's rule list). Serializes
/// as `{ kind, message }`, matching [`crate::error::AppError`]'s shape;
/// no variant carries any part of the untrusted report itself, so the
/// serialized value never reflects attacker-controlled content back to
/// the caller.
#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone, Copy)]
pub enum ReportError {
    /// The calling webview's label is not `svc-<serviceId>` for the
    /// report's own `serviceId` — including when `serviceId` itself is
    /// not a well-formed [`ServiceId`], which can then never match any
    /// real label.
    #[error("calling webview label does not match the report's serviceId")]
    LabelMismatch,
    /// `serviceId` is well-formed and the label matched, but no such
    /// service exists in the live configuration.
    #[error("service does not exist")]
    UnknownService,
    /// The calling webview's current URL (`tauri::Webview::url()`) has a
    /// different origin than the service's currently configured URL.
    /// This is distinct from a stale runtime-capability grant (a service
    /// edited to a new URL keeps its old origin's grant — see
    /// `agent_bridge::capability`'s module doc): this check compares
    /// against the *current* configured origin regardless of what the
    /// runtime ACL still happens to allow.
    #[error("calling webview origin does not match the service's configured origin")]
    OriginMismatch,
    /// `count` is present and negative.
    #[error("count must not be negative")]
    CountNegative,
    /// `count` is present and greater than [`MAX_COUNT`].
    #[error("count exceeds the maximum of {MAX_COUNT}")]
    CountTooLarge,
    /// `messages` has more than [`MAX_MESSAGES`] entries.
    #[error("messages exceeds the maximum of {MAX_MESSAGES}")]
    TooManyMessages,
    /// A message's `id`, `from`, `subject` or `link` is longer than
    /// [`MAX_STRING_LEN`] Unicode scalar values.
    #[error("a message string field exceeds the maximum of {MAX_STRING_LEN} characters")]
    StringTooLong,
    /// A message's `link` is not `https`, is not a valid URL, or is not
    /// on the service's own origin.
    #[error("a message link is not https on the service origin")]
    InvalidLink,
    /// `recipeId` is longer than [`MAX_RECIPE_ID_LEN`] characters.
    #[error("recipeId exceeds the maximum of {MAX_RECIPE_ID_LEN} characters")]
    RecipeIdTooLong,
    /// `recipeId` is empty or contains a character outside `[a-z0-9-]`.
    #[error("recipeId must match [a-z0-9-]")]
    RecipeIdInvalidChars,
    /// `iconCandidates` has more than [`MAX_ICON_CANDIDATES`] entries.
    #[error("iconCandidates exceeds the maximum of {MAX_ICON_CANDIDATES}")]
    TooManyIconCandidates,
    /// An icon candidate is not a valid URL.
    #[error("an iconCandidate is not a valid URL")]
    IconCandidateInvalidUrl,
    /// An icon candidate's scheme is not `http` or `https`.
    #[error("an iconCandidate must be http or https")]
    IconCandidateInvalidScheme,
    /// An icon candidate's host is a loopback, private or link-local
    /// address (design.md §2.2.6, §2.2.10) — including an IPv4-mapped
    /// IPv6 address whose mapped IPv4 address is one of those. No DNS
    /// resolution is performed: a domain name is never rejected on this
    /// basis.
    #[error("an iconCandidate host is loopback, private or link-local")]
    IconCandidateDisallowedHost,
}

impl ReportError {
    /// A stable, machine-readable identifier for this variant — the
    /// `kind` field of the `{ kind, message }` shape (mirrors
    /// [`crate::error::AppError::kind`]).
    pub fn kind(&self) -> &'static str {
        match self {
            ReportError::LabelMismatch => "label_mismatch",
            ReportError::UnknownService => "unknown_service",
            ReportError::OriginMismatch => "origin_mismatch",
            ReportError::CountNegative => "count_negative",
            ReportError::CountTooLarge => "count_too_large",
            ReportError::TooManyMessages => "too_many_messages",
            ReportError::StringTooLong => "string_too_long",
            ReportError::InvalidLink => "invalid_link",
            ReportError::RecipeIdTooLong => "recipe_id_too_long",
            ReportError::RecipeIdInvalidChars => "recipe_id_invalid_chars",
            ReportError::TooManyIconCandidates => "too_many_icon_candidates",
            ReportError::IconCandidateInvalidUrl => "icon_candidate_invalid_url",
            ReportError::IconCandidateInvalidScheme => "icon_candidate_invalid_scheme",
            ReportError::IconCandidateDisallowedHost => "icon_candidate_disallowed_host",
        }
    }
}

impl serde::Serialize for ReportError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("ReportError", 2)?;
        state.serialize_field("kind", self.kind())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}

/// A validated message reference: the same shape as [`super::dto::MessageRefDto`],
/// but every string is within bounds and `link`, if present, has already
/// been parsed and checked (design.md §2.2.6).
#[derive(Debug, Clone)]
pub struct ValidMessageRef {
    pub id: String,
    pub from: Option<String>,
    pub subject: Option<String>,
    pub link: Option<Url>,
}

/// A report that has passed every design.md §2.2.6 rule. The only way to
/// obtain one is [`validate`] returning `Ok` — there is no public
/// constructor, so a `ValidReport` value is a type-level proof of
/// validity.
///
/// `service_id` and `count` are read by `report_unread`'s success path
/// (the log line and the `unread` status store, design.md §2.2.7);
/// `count` and `messages` are also read by `notify::diff::evaluate`;
/// `icon_candidates` is read by `report_unread`'s icon-resolution path
/// (design.md §2.2.10) via [`ValidReport::icon_candidates`].
/// `recipe_id` and `observed_at` have no consumer yet (a later task:
/// notification text), so they stay `#[allow(dead_code)]` rather than
/// fabricating a reader that doesn't exist.
#[derive(Debug, Clone)]
pub struct ValidReport {
    service_id: ServiceId,
    count: Option<u32>,
    messages: Vec<ValidMessageRef>,
    #[allow(dead_code)]
    recipe_id: String,
    #[allow(dead_code)]
    observed_at: u64,
    icon_candidates: Vec<Url>,
}

impl ValidReport {
    /// The report's service id, for `report_unread`'s success log line.
    pub fn service_id(&self) -> &ServiceId {
        &self.service_id
    }

    /// The report's validated unread count (design.md §2.2.9's
    /// `notify::diff::evaluate`): `None` means the agent could not
    /// determine one.
    pub fn count(&self) -> Option<u32> {
        self.count
    }

    /// The report's validated message references, in report order
    /// (design.md §2.2.9's `notify::diff::evaluate`).
    pub fn messages(&self) -> &[ValidMessageRef] {
        &self.messages
    }

    /// The report's already-ordered icon candidates (design.md §2.2.10;
    /// best first: `apple-touch-icon`, then largest `rel=icon`, then
    /// `/favicon.ico`), for `report_unread` to hand to
    /// [`crate::services::ServiceManager::record_icon_candidates`] when
    /// non-empty.
    pub fn icon_candidates(&self) -> &[Url] {
        &self.icon_candidates
    }
}

/// Validates `dto` against every design.md §2.2.6 rule, in the order
/// listed there, returning the first violation.
///
/// `caller_label` is the invoking webview's label
/// (`tauri::Webview::label()`). `caller_url` is the invoking webview's
/// *current* URL (`tauri::Webview::url()`, already resolved by the
/// caller — see [`super::report_unread`]'s doc comment for what happens
/// when reading it fails); its origin must match the service's currently
/// configured origin, or the report is rejected, because a previously
/// granted runtime capability can outlive an edited service URL
/// (`agent_bridge::capability`'s module doc) and so is not on its own
/// proof that the webview is still showing the service's current
/// origin. `lookup_service_url` resolves a [`ServiceId`] to its
/// currently configured [`Url`] — `None` means the service does not
/// exist — so this function stays pure and independent of how the
/// caller looks that up (a live `ServiceManager` in production, a
/// fixture map in tests).
pub fn validate(
    dto: UnreadReportDto,
    caller_label: &str,
    caller_url: &Url,
    lookup_service_url: impl FnOnce(&ServiceId) -> Option<Url>,
) -> Result<ValidReport, ReportError> {
    let service_id = ServiceId::new(dto.service_id).map_err(|_| ReportError::LabelMismatch)?;
    if host::service_label(&service_id) != caller_label {
        return Err(ReportError::LabelMismatch);
    }
    let service_url = lookup_service_url(&service_id).ok_or(ReportError::UnknownService)?;
    let service_origin = service_url.origin();

    if caller_url.origin() != service_origin {
        return Err(ReportError::OriginMismatch);
    }

    let count = match dto.count {
        None => None,
        Some(count) if count < 0 => return Err(ReportError::CountNegative),
        Some(count) if count > MAX_COUNT => return Err(ReportError::CountTooLarge),
        Some(count) => Some(u32::try_from(count).map_err(|_| ReportError::CountTooLarge)?),
    };

    if dto.messages.len() > MAX_MESSAGES {
        return Err(ReportError::TooManyMessages);
    }

    let mut messages = Vec::with_capacity(dto.messages.len());
    for message in dto.messages {
        check_len(&message.id)?;
        if let Some(from) = &message.from {
            check_len(from)?;
        }
        if let Some(subject) = &message.subject {
            check_len(subject)?;
        }
        let link = match message.link {
            Some(raw) => {
                check_len(&raw)?;
                let url = Url::parse(&raw).map_err(|_| ReportError::InvalidLink)?;
                if url.scheme() != "https" || url.origin() != service_origin {
                    return Err(ReportError::InvalidLink);
                }
                Some(url)
            }
            None => None,
        };
        messages.push(ValidMessageRef {
            id: message.id,
            from: message.from,
            subject: message.subject,
            link,
        });
    }

    let recipe_id = dto.recipe_id;
    if recipe_id.chars().count() > MAX_RECIPE_ID_LEN {
        return Err(ReportError::RecipeIdTooLong);
    }
    if recipe_id.is_empty()
        || !recipe_id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(ReportError::RecipeIdInvalidChars);
    }

    if dto.icon_candidates.len() > MAX_ICON_CANDIDATES {
        return Err(ReportError::TooManyIconCandidates);
    }

    let mut icon_candidates = Vec::with_capacity(dto.icon_candidates.len());
    for raw in dto.icon_candidates {
        let url = Url::parse(&raw).map_err(|_| ReportError::IconCandidateInvalidUrl)?;
        match url.scheme() {
            "http" | "https" => {}
            _ => return Err(ReportError::IconCandidateInvalidScheme),
        }
        let host = url.host().ok_or(ReportError::IconCandidateInvalidUrl)?;
        if is_disallowed_host(&host) {
            return Err(ReportError::IconCandidateDisallowedHost);
        }
        icon_candidates.push(url);
    }

    Ok(ValidReport {
        service_id,
        count,
        messages,
        recipe_id,
        observed_at: dto.observed_at,
        icon_candidates,
    })
}

/// Rejects a string longer than [`MAX_STRING_LEN`] Unicode scalar
/// values.
fn check_len(value: &str) -> Result<(), ReportError> {
    if value.chars().count() > MAX_STRING_LEN {
        Err(ReportError::StringTooLong)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_bridge::dto::MessageRefDto;

    const SERVICE_ID: &str = "gmail-personal";
    const SERVICE_ORIGIN: &str = "https://mail.google.com";

    fn base_dto() -> UnreadReportDto {
        UnreadReportDto {
            service_id: SERVICE_ID.to_string(),
            count: Some(3),
            messages: Vec::new(),
            recipe_id: "gmail".to_string(),
            observed_at: 1_700_000_000_000,
            icon_candidates: Vec::new(),
        }
    }

    fn caller_label() -> String {
        host::service_label(&ServiceId::new(SERVICE_ID).expect("valid id"))
    }

    /// The calling webview's current URL when it matches the service's
    /// configured origin — the common case in tests that are not
    /// specifically exercising [`ReportError::OriginMismatch`].
    fn caller_url() -> Url {
        Url::parse(SERVICE_ORIGIN).expect("valid url")
    }

    fn lookup(_id: &ServiceId) -> Option<Url> {
        Some(Url::parse(SERVICE_ORIGIN).expect("valid url"))
    }

    fn no_service(_id: &ServiceId) -> Option<Url> {
        None
    }

    #[test]
    fn accepts_a_minimal_valid_report() {
        let report =
            validate(base_dto(), &caller_label(), &caller_url(), lookup).expect("should validate");
        assert_eq!(report.service_id().as_str(), SERVICE_ID);
    }

    #[test]
    fn accepts_null_count() {
        let mut dto = base_dto();
        dto.count = None;
        assert!(validate(dto, &caller_label(), &caller_url(), lookup).is_ok());
    }

    #[test]
    fn accepts_count_at_the_maximum() {
        let mut dto = base_dto();
        dto.count = Some(1_000_000);
        assert!(validate(dto, &caller_label(), &caller_url(), lookup).is_ok());
    }

    #[test]
    fn accepts_a_full_report_with_a_valid_link_and_icon_candidates() {
        let mut dto = base_dto();
        dto.messages = vec![MessageRefDto {
            id: "m1".to_string(),
            from: Some("a@example.com".to_string()),
            subject: Some("hello".to_string()),
            link: Some(format!("{SERVICE_ORIGIN}/mail/u/0/#inbox/abc")),
        }];
        dto.icon_candidates = vec![format!("{SERVICE_ORIGIN}/favicon.ico")];
        let report =
            validate(dto, &caller_label(), &caller_url(), lookup).expect("should validate");
        assert_eq!(report.icon_candidates.len(), 1);
        assert_eq!(report.messages.len(), 1);
        assert!(report.messages[0].link.is_some());
    }

    #[test]
    fn rejects_wrong_label() {
        let err = validate(base_dto(), "svc-someone-else", &caller_url(), lookup).unwrap_err();
        assert_eq!(err, ReportError::LabelMismatch);
    }

    #[test]
    fn rejects_malformed_service_id() {
        let mut dto = base_dto();
        dto.service_id = "Not Valid!".to_string();
        let err = validate(dto, "svc-anything", &caller_url(), lookup).unwrap_err();
        assert_eq!(err, ReportError::LabelMismatch);
    }

    #[test]
    fn rejects_unknown_service() {
        let err = validate(base_dto(), &caller_label(), &caller_url(), no_service).unwrap_err();
        assert_eq!(err, ReportError::UnknownService);
    }

    #[test]
    fn rejects_a_caller_url_on_a_different_origin_than_the_service() {
        let off_origin = Url::parse("https://evil.example.com/").expect("valid url");
        let err = validate(base_dto(), &caller_label(), &off_origin, lookup).unwrap_err();
        assert_eq!(err, ReportError::OriginMismatch);
    }

    #[test]
    fn rejects_a_caller_url_differing_only_by_scheme() {
        // Same host, different scheme: still a different origin.
        let off_origin = Url::parse("http://mail.google.com/").expect("valid url");
        let err = validate(base_dto(), &caller_label(), &off_origin, lookup).unwrap_err();
        assert_eq!(err, ReportError::OriginMismatch);
    }

    #[test]
    fn accepts_a_caller_url_on_the_same_origin_with_a_different_path() {
        // Only the origin is compared — path, query and fragment on the
        // caller's current URL are irrelevant.
        let same_origin =
            Url::parse(&format!("{SERVICE_ORIGIN}/mail/u/0/#inbox")).expect("valid url");
        assert!(validate(base_dto(), &caller_label(), &same_origin, lookup).is_ok());
    }

    #[test]
    fn origin_mismatch_serializes_with_the_expected_kind() {
        let value = serde_json::to_value(ReportError::OriginMismatch).expect("serialize");
        assert_eq!(value["kind"], "origin_mismatch");
    }

    #[test]
    fn rejects_a_negative_count() {
        let mut dto = base_dto();
        dto.count = Some(-1);
        let err = validate(dto, &caller_label(), &caller_url(), lookup).unwrap_err();
        assert_eq!(err, ReportError::CountNegative);
    }

    #[test]
    fn accepts_zero_and_the_maximum_count() {
        for count in [0, 1_000_000] {
            let mut dto = base_dto();
            dto.count = Some(count);
            assert!(
                validate(dto, &caller_label(), &caller_url(), lookup).is_ok(),
                "count {count}"
            );
        }
    }

    #[test]
    fn rejects_count_over_the_maximum() {
        let mut dto = base_dto();
        dto.count = Some(1_000_001);
        let err = validate(dto, &caller_label(), &caller_url(), lookup).unwrap_err();
        assert_eq!(err, ReportError::CountTooLarge);
    }

    #[test]
    fn rejects_more_than_100_messages() {
        let mut dto = base_dto();
        dto.messages = (0..101)
            .map(|i| MessageRefDto {
                id: format!("m{i}"),
                from: None,
                subject: None,
                link: None,
            })
            .collect();
        let err = validate(dto, &caller_label(), &caller_url(), lookup).unwrap_err();
        assert_eq!(err, ReportError::TooManyMessages);
    }

    #[test]
    fn accepts_exactly_100_messages() {
        let mut dto = base_dto();
        dto.messages = (0..100)
            .map(|i| MessageRefDto {
                id: format!("m{i}"),
                from: None,
                subject: None,
                link: None,
            })
            .collect();
        assert!(validate(dto, &caller_label(), &caller_url(), lookup).is_ok());
    }

    #[test]
    fn rejects_a_string_over_512_characters() {
        let mut dto = base_dto();
        dto.messages = vec![MessageRefDto {
            id: "a".repeat(513),
            from: None,
            subject: None,
            link: None,
        }];
        let err = validate(dto, &caller_label(), &caller_url(), lookup).unwrap_err();
        assert_eq!(err, ReportError::StringTooLong);
    }

    #[test]
    fn accepts_a_string_at_exactly_512_characters() {
        let mut dto = base_dto();
        dto.messages = vec![MessageRefDto {
            id: "a".repeat(512),
            from: None,
            subject: None,
            link: None,
        }];
        assert!(validate(dto, &caller_label(), &caller_url(), lookup).is_ok());
    }

    #[test]
    fn rejects_a_non_https_link() {
        let mut dto = base_dto();
        dto.messages = vec![MessageRefDto {
            id: "m1".to_string(),
            from: None,
            subject: None,
            link: Some("http://mail.google.com/inbox".to_string()),
        }];
        let err = validate(dto, &caller_label(), &caller_url(), lookup).unwrap_err();
        assert_eq!(err, ReportError::InvalidLink);
    }

    #[test]
    fn rejects_a_link_on_a_different_origin() {
        let mut dto = base_dto();
        dto.messages = vec![MessageRefDto {
            id: "m1".to_string(),
            from: None,
            subject: None,
            link: Some("https://evil.example.com/inbox".to_string()),
        }];
        let err = validate(dto, &caller_label(), &caller_url(), lookup).unwrap_err();
        assert_eq!(err, ReportError::InvalidLink);
    }

    #[test]
    fn rejects_recipe_id_over_64_characters() {
        let mut dto = base_dto();
        dto.recipe_id = "a".repeat(65);
        let err = validate(dto, &caller_label(), &caller_url(), lookup).unwrap_err();
        assert_eq!(err, ReportError::RecipeIdTooLong);
    }

    #[test]
    fn accepts_recipe_id_at_exactly_64_characters() {
        let mut dto = base_dto();
        dto.recipe_id = "a".repeat(64);
        assert!(validate(dto, &caller_label(), &caller_url(), lookup).is_ok());
    }

    #[test]
    fn rejects_recipe_id_with_uppercase_or_symbols() {
        let mut dto = base_dto();
        dto.recipe_id = "Gmail_v2".to_string();
        let err = validate(dto, &caller_label(), &caller_url(), lookup).unwrap_err();
        assert_eq!(err, ReportError::RecipeIdInvalidChars);
    }

    #[test]
    fn rejects_empty_recipe_id() {
        let mut dto = base_dto();
        dto.recipe_id = String::new();
        let err = validate(dto, &caller_label(), &caller_url(), lookup).unwrap_err();
        assert_eq!(err, ReportError::RecipeIdInvalidChars);
    }

    #[test]
    fn rejects_more_than_8_icon_candidates() {
        let mut dto = base_dto();
        dto.icon_candidates = (0..9)
            .map(|i| format!("{SERVICE_ORIGIN}/icon{i}.png"))
            .collect();
        let err = validate(dto, &caller_label(), &caller_url(), lookup).unwrap_err();
        assert_eq!(err, ReportError::TooManyIconCandidates);
    }

    #[test]
    fn accepts_exactly_8_icon_candidates() {
        let mut dto = base_dto();
        dto.icon_candidates = (0..8)
            .map(|i| format!("{SERVICE_ORIGIN}/icon{i}.png"))
            .collect();
        assert!(validate(dto, &caller_label(), &caller_url(), lookup).is_ok());
    }

    #[test]
    fn rejects_a_non_http_icon_candidate_scheme() {
        let mut dto = base_dto();
        dto.icon_candidates = vec!["ftp://mail.google.com/icon.png".to_string()];
        let err = validate(dto, &caller_label(), &caller_url(), lookup).unwrap_err();
        assert_eq!(err, ReportError::IconCandidateInvalidScheme);
    }

    #[test]
    fn accepts_an_http_icon_candidate() {
        let mut dto = base_dto();
        dto.icon_candidates = vec!["http://mail.google.com/icon.png".to_string()];
        assert!(validate(dto, &caller_label(), &caller_url(), lookup).is_ok());
    }

    #[test]
    fn rejects_an_invalid_icon_candidate_url() {
        let mut dto = base_dto();
        dto.icon_candidates = vec!["not a url".to_string()];
        let err = validate(dto, &caller_label(), &caller_url(), lookup).unwrap_err();
        assert_eq!(err, ReportError::IconCandidateInvalidUrl);
    }

    #[test]
    fn rejects_loopback_ipv4_icon_candidate() {
        let mut dto = base_dto();
        dto.icon_candidates = vec!["http://127.0.0.1/icon.png".to_string()];
        let err = validate(dto, &caller_label(), &caller_url(), lookup).unwrap_err();
        assert_eq!(err, ReportError::IconCandidateDisallowedHost);
    }

    #[test]
    fn rejects_private_ipv4_icon_candidate() {
        let mut dto = base_dto();
        dto.icon_candidates = vec!["http://10.0.0.5/icon.png".to_string()];
        let err = validate(dto, &caller_label(), &caller_url(), lookup).unwrap_err();
        assert_eq!(err, ReportError::IconCandidateDisallowedHost);
    }

    #[test]
    fn rejects_link_local_ipv4_icon_candidate() {
        let mut dto = base_dto();
        dto.icon_candidates = vec!["http://169.254.1.1/icon.png".to_string()];
        let err = validate(dto, &caller_label(), &caller_url(), lookup).unwrap_err();
        assert_eq!(err, ReportError::IconCandidateDisallowedHost);
    }

    #[test]
    fn rejects_loopback_ipv6_icon_candidate() {
        let mut dto = base_dto();
        dto.icon_candidates = vec!["http://[::1]/icon.png".to_string()];
        let err = validate(dto, &caller_label(), &caller_url(), lookup).unwrap_err();
        assert_eq!(err, ReportError::IconCandidateDisallowedHost);
    }

    #[test]
    fn rejects_link_local_ipv6_icon_candidate() {
        let mut dto = base_dto();
        dto.icon_candidates = vec!["http://[fe80::1]/icon.png".to_string()];
        let err = validate(dto, &caller_label(), &caller_url(), lookup).unwrap_err();
        assert_eq!(err, ReportError::IconCandidateDisallowedHost);
    }

    #[test]
    fn rejects_unique_local_ipv6_icon_candidate() {
        let mut dto = base_dto();
        dto.icon_candidates = vec!["http://[fc00::1]/icon.png".to_string()];
        let err = validate(dto, &caller_label(), &caller_url(), lookup).unwrap_err();
        assert_eq!(err, ReportError::IconCandidateDisallowedHost);
    }

    #[test]
    fn rejects_ipv4_mapped_ipv6_loopback_icon_candidate() {
        let mut dto = base_dto();
        dto.icon_candidates = vec!["http://[::ffff:127.0.0.1]/icon.png".to_string()];
        let err = validate(dto, &caller_label(), &caller_url(), lookup).unwrap_err();
        assert_eq!(err, ReportError::IconCandidateDisallowedHost);
    }

    #[test]
    fn rejects_ipv4_mapped_ipv6_private_icon_candidate() {
        let mut dto = base_dto();
        dto.icon_candidates = vec!["http://[::ffff:10.1.2.3]/icon.png".to_string()];
        let err = validate(dto, &caller_label(), &caller_url(), lookup).unwrap_err();
        assert_eq!(err, ReportError::IconCandidateDisallowedHost);
    }

    #[test]
    fn accepts_a_public_ipv4_icon_candidate() {
        let mut dto = base_dto();
        dto.icon_candidates = vec!["http://93.184.216.34/icon.png".to_string()];
        assert!(validate(dto, &caller_label(), &caller_url(), lookup).is_ok());
    }

    #[test]
    fn accepts_a_domain_icon_candidate_without_dns_resolution() {
        // No DNS resolution is performed (design.md §2.2.6): a domain
        // name is accepted regardless of what it might resolve to.
        let mut dto = base_dto();
        dto.icon_candidates = vec!["https://mail.google.com/favicon.ico".to_string()];
        assert!(validate(dto, &caller_label(), &caller_url(), lookup).is_ok());
    }

    #[test]
    fn report_error_serializes_as_kind_and_message() {
        let err = ReportError::CountTooLarge;
        let value = serde_json::to_value(err).expect("serialize");
        assert_eq!(value["kind"], "count_too_large");
        assert_eq!(value["message"], err.to_string());
        let obj = value.as_object().expect("object");
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["kind", "message"]);
    }
}
