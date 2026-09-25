//! Runtime capability registration and injection-script building for a
//! service webview (design.md §2.2.6, §4.3).
//!
//! Everything here is called from [`crate::services::ServiceManager`]
//! before [`crate::host::WebviewHost::create`] runs for a service — see
//! that module's own doc comment for the "only register an
//! (id, origin) combination once" bookkeeping this module does not itself
//! own.
//!
//! ## The identifier scheme
//!
//! The runtime capability's identifier is `svc-<id>` — deliberately the
//! *exact same string* as [`crate::host::service_label`]'s webview label,
//! matching design.md §2.2.6's own `CapabilityBuilder` example verbatim.
//! [`capability_identifier`] is a separate function from
//! [`crate::host::service_label`] only because the two are conceptually
//! different namespaces (a capability identifier vs. a webview label)
//! that happen, by design, to share one value.
//!
//! This module deliberately does **not** try to invent a per-origin or
//! per-generation suffix (e.g. `svc-<id>-2`) for the identifier, even
//! though a service can be destroyed and recreated at a different origin
//! (design.md §2.2.5: "URL or profile change → destroy and recreate").
//! Reading `tauri` 2.11.6's runtime ACL implementation
//! (`~/.cargo/registry/src/index.crates.io-*/tauri-2.11.6/src/ipc/authority.rs`,
//! `RuntimeAuthority::add_capability_inner`) shows why a differently-named
//! identifier would not help:
//!
//! - `add_capability_inner` resolves the new capability against the
//!   process's base ACL and then **`.extend()`s** the resulting
//!   `ResolvedCommand` entries onto `self.allowed_commands` (and
//!   `denied_commands`/scope maps), keyed by *command name*
//!   (`report_unread`) — not by capability identifier.
//! - Every call therefore only ever **adds** more permitted
//!   `(webview label pattern, origin pattern)` combinations for that
//!   command. Nothing is replaced, deduplicated or removed, regardless of
//!   whether the identifier is reused or freshly minted.
//!
//! Consequences, given there is no Tauri API to remove or replace a
//! capability at runtime (design.md §2.2.6 already states this for the
//! service-deletion case):
//!
//! 1. If a service is recreated at a **new** origin, the grant for its
//!    **previous** origin is not revoked — it stays valid for the
//!    `svc-<id>` webview label for the rest of the process's life, the
//!    same "inert" situation design.md describes for a deleted service's
//!    capability, except here the label is still very much in use. A
//!    differently-named identifier for the new origin would not change
//!    this: the old origin's `ResolvedCommand` entry for `report_unread`
//!    would still list `svc-<id>` in its webview patterns either way.
//! 2. Because of (1), re-registering the *same* `(id, origin)` pair is
//!    pure waste (another `ResolvedCommand` entry that duplicates an
//!    existing one) — not a correctness bug, but worth avoiding. This is
//!    why `ServiceManager` tracks which `(id, origin)` pairs have already
//!    been granted for the life of the process and only calls
//!    [`ensure_capability`] for a pair it has not already recorded as
//!    granted. That bookkeeping is purely an idempotency optimization; it
//!    cannot and does not revoke anything, and a service whose URL is
//!    edited back and forth between two origins will simply accumulate a
//!    grant for each origin it has ever used.
//!
//! `svc-<id>` is therefore kept as the one and only identifier per
//! service id, exactly as design.md's snippet shows, with this limitation
//! documented rather than worked around.

use serde::Serialize;
use tauri::{AppHandle, Manager, Wry};
use url::Url;

use crate::config::ServiceId;
use crate::error::AppResult;
use crate::host;

pub use crate::agent::AGENT_JS;

/// The `allow-report-unread` permission `build.rs` autogenerates for the
/// `report_unread` command (design.md §2.2.6; confirmed by
/// `docs/spikes/SP3.md`).
const ALLOW_REPORT_UNREAD_PERMISSION: &str = "allow-report-unread";

/// How often the agent re-sends a report, milliseconds (design.md
/// §2.2.6's `reportIntervalMs`). Named so `build_injection_script` never
/// spells out the literal `30000` inline.
pub const REPORT_INTERVAL_MS: u64 = 30_000;

/// A URL's origin is opaque (e.g. `data:`, `blob:`, or another scheme
/// `url::Url::origin()` cannot express as `scheme://host[:port]`) and so
/// cannot be used to scope a runtime capability.
#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
#[error("service URL has an opaque origin and cannot be used for a runtime capability")]
pub struct OpaqueOriginError;

/// Derives a service's origin — scheme, host and effective port — from
/// its configured URL (design.md §2.2.6's `{origin}/*`), rejecting an
/// opaque origin outright. Pure and independent of Tauri.
///
/// The returned string is the origin's ASCII serialization
/// (`scheme://host` with the port included only when it differs from the
/// scheme's default, e.g. `https://mail.example.com` or
/// `https://mail.example.com:8443`) — exactly the form design.md's
/// `format!("{origin}/*")` expects.
pub fn service_origin(url: &Url) -> Result<String, OpaqueOriginError> {
    match url.origin() {
        url::Origin::Opaque(_) => Err(OpaqueOriginError),
        tuple @ url::Origin::Tuple(..) => Ok(tuple.ascii_serialization()),
    }
}

/// The runtime capability's identifier for service `id` — see this
/// module's doc comment for why it is deliberately identical to
/// [`crate::host::service_label`]'s webview label.
pub fn capability_identifier(id: &ServiceId) -> String {
    host::service_label(id)
}

/// Registers the runtime capability that lets service `id`'s webview
/// invoke `report_unread` from `origin` (design.md §2.2.6's
/// `CapabilityBuilder` example): identifier [`capability_identifier`],
/// scoped to `{origin}/*` and webview label
/// [`crate::host::service_label`], granting only
/// `allow-report-unread`.
///
/// Callers (`ServiceManager`) are responsible for only calling this once
/// per `(id, origin)` pair for the life of the process — see this
/// module's doc comment for why calling it again for the same pair is
/// wasted rather than harmful, and why calling it for a *different*
/// origin does not revoke the previous one.
pub fn ensure_capability(
    app_handle: &AppHandle<Wry>,
    id: &ServiceId,
    origin: &str,
) -> AppResult<()> {
    app_handle.add_capability(
        tauri::ipc::CapabilityBuilder::new(capability_identifier(id))
            .remote(format!("{origin}/*"))
            .webview(host::service_label(id))
            .permission(ALLOW_REPORT_UNREAD_PERMISSION),
    )?;
    Ok(())
}

/// The JSON shape injected as `window.__ELUMA__` (design.md §2.2.6),
/// serialized with `serde_json` — never by string-concatenating values
/// directly into the script.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InjectedConfig<'a> {
    service_id: &'a str,
    service_url: &'a str,
    report_interval_ms: u64,
    reconcile_interval_ms: u64,
}

/// Builds the script injected into a service webview (design.md §2.2.6):
/// `window.__ELUMA__ = {json};\n{AGENT_JS}`, where `{json}` is
/// `serde_json`-serialized (camelCase keys) and `{AGENT_JS}` is
/// [`crate::agent::AGENT_JS`], the agent bundle `pnpm build:agent`
/// produces.
///
/// `reconcile_interval_seconds` is `Config.settings.reconcile_interval_seconds`
/// (design.md §2.2.1) and is converted to milliseconds here, once, in one
/// place.
pub fn build_injection_script(
    id: &ServiceId,
    url: &Url,
    reconcile_interval_seconds: u32,
) -> Result<String, serde_json::Error> {
    let service_id = id.as_str();
    let service_url = url.as_str();
    let config = InjectedConfig {
        service_id,
        service_url,
        report_interval_ms: REPORT_INTERVAL_MS,
        reconcile_interval_ms: u64::from(reconcile_interval_seconds) * 1000,
    };
    let json = serde_json::to_string(&config)?;
    Ok(format!("window.__ELUMA__ = {json};\n{AGENT_JS}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: &str) -> ServiceId {
        ServiceId::new(value).expect("valid service id")
    }

    // --- service_origin ----------------------------------------------

    #[test]
    fn derives_origin_from_https_url_omitting_default_port() {
        let url = Url::parse("https://mail.example.com/inbox?x=1").expect("valid url");
        assert_eq!(
            service_origin(&url).expect("origin"),
            "https://mail.example.com"
        );
    }

    #[test]
    fn derives_origin_keeping_a_non_default_port() {
        let url = Url::parse("https://mail.example.com:8443/inbox").expect("valid url");
        assert_eq!(
            service_origin(&url).expect("origin"),
            "https://mail.example.com:8443"
        );
    }

    #[test]
    fn derives_origin_for_http_omitting_default_port() {
        let url = Url::parse("http://mail.example.com/inbox").expect("valid url");
        assert_eq!(
            service_origin(&url).expect("origin"),
            "http://mail.example.com"
        );
    }

    #[test]
    fn rejects_an_opaque_origin() {
        let url = Url::parse("data:text/plain,hello").expect("valid url");
        assert_eq!(service_origin(&url), Err(OpaqueOriginError));
    }

    #[test]
    fn different_hosts_yield_different_origins() {
        let a = Url::parse("https://mail.example.com/").expect("valid url");
        let b = Url::parse("https://mail.other.com/").expect("valid url");
        assert_ne!(
            service_origin(&a).expect("origin"),
            service_origin(&b).expect("origin")
        );
    }

    // --- capability_identifier -----------------------------------------

    #[test]
    fn capability_identifier_matches_the_webview_label() {
        let service_id = id("gmail-personal");
        assert_eq!(
            capability_identifier(&service_id),
            host::service_label(&service_id)
        );
        assert_eq!(capability_identifier(&service_id), "svc-gmail-personal");
    }

    // --- build_injection_script -----------------------------------------

    #[test]
    fn injection_script_has_the_expected_shape() {
        let service_id = id("gmail-personal");
        let url = Url::parse("https://mail.google.com/mail/u/0/").expect("valid url");
        let script = build_injection_script(&service_id, &url, 60).expect("build script");

        let expected_prefix = "window.__ELUMA__ = ";
        assert!(script.starts_with(expected_prefix));
        assert!(
            script.ends_with(AGENT_JS),
            "script must end with AGENT_JS verbatim"
        );

        let json_part = script
            .strip_prefix(expected_prefix)
            .and_then(|rest| rest.split_once(";\n"))
            .map(|(json, _agent)| json)
            .expect("script has `window.__ELUMA__ = {json};\\n{AGENT_JS}` shape");

        let value: serde_json::Value = serde_json::from_str(json_part).expect("valid json");
        assert_eq!(value["serviceId"], "gmail-personal");
        assert_eq!(value["serviceUrl"], "https://mail.google.com/mail/u/0/");
        assert_eq!(value["reportIntervalMs"], REPORT_INTERVAL_MS);
        assert_eq!(value["reconcileIntervalMs"], 60_000);

        let obj = value.as_object().expect("object");
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "reconcileIntervalMs",
                "reportIntervalMs",
                "serviceId",
                "serviceUrl",
            ]
        );
    }

    #[test]
    fn reconcile_interval_seconds_converts_to_milliseconds() {
        let service_id = id("icloud");
        let url = Url::parse("https://www.icloud.com/mail").expect("valid url");
        let script = build_injection_script(&service_id, &url, 1).expect("build script");
        assert!(script.contains("\"reconcileIntervalMs\":1000"));
    }

    #[test]
    fn injection_script_never_duplicates_agent_js_content() {
        // A regression guard: the script must contain AGENT_JS exactly
        // once, as the tail — never string-concatenated more than once,
        // and never with the JSON values substituted anywhere inside it.
        let service_id = id("gmail-personal");
        let url = Url::parse("https://mail.google.com/").expect("valid url");
        let script = build_injection_script(&service_id, &url, 30).expect("build script");
        assert_eq!(script.matches(AGENT_JS).count(), 1);
    }
}
