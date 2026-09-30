//! Classifies a service webview's new-window request (`target="_blank"`,
//! `window.open`) against the service's own configured URL (Task #95,
//! design.md's "Service webview new-window requests" section).
//!
//! Deliberately host-independent: the only inputs are a pair of
//! [`url::Url`]s (the service's configured URL and the requested URL), with
//! no dependency on Tauri's `Webview`/`NewWindowFeatures` types, so
//! [`classify`] can be exercised by the table-driven tests below without a
//! running webview. [`super::multiwebview`] and (behind Cargo feature
//! `host-child-windows`) [`super::child_windows`] are the only callers that
//! turn a [`Classification`] into an actual `NewWindowResponse`.

use std::net::{Ipv4Addr, Ipv6Addr};

use url::{Host, Url};

/// The outcome of comparing a requested new-window URL against the
/// service's configured URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classification {
    /// Same registrable domain (or, for an IP-literal/unknown-suffix host,
    /// the identical host) as the configured URL — navigate the existing
    /// service webview in place instead of opening a new window.
    SameView,
    /// A different `http`/`https` host — open it in the default browser.
    External,
    /// Not an `http`/`https` URL, or the URL has no host — never opened.
    Deny,
}

/// Classifies `requested` (the URL a service webview asked to open in a new
/// window) against `configured` (that service's own configured URL), in
/// the order design.md's "Service webview new-window requests" section
/// specifies:
///
/// 1. A scheme other than `http`/`https` is always [`Classification::Deny`].
/// 2. A missing host (on either URL) is [`Classification::Deny`].
/// 3. An IP-literal requested host is compared against `configured`'s host
///    *before* any PSL lookup: a type-and-value match (`Ipv4` to `Ipv4`,
///    `Ipv6` to `Ipv6`) is [`Classification::SameView`]; anything else,
///    including a match across IPv4/IPv6, is [`Classification::External`].
/// 4. Otherwise both hosts are domains: each is compared using its
///    registrable domain when its public suffix is known
///    ([`psl::Suffix::is_known`]), or its full lowercased, trailing-dot-
///    stripped host when it is not — see [`comparison_key`].
///
/// Port and userinfo never enter the comparison — [`Url::host`] already
/// excludes both.
pub fn classify(configured: &Url, requested: &Url) -> Classification {
    let scheme = requested.scheme();
    if scheme != "http" && scheme != "https" {
        return Classification::Deny;
    }

    let (Some(requested_host), Some(configured_host)) = (requested.host(), configured.host())
    else {
        return Classification::Deny;
    };

    match requested_host {
        Host::Ipv4(requested_ip) => classify_ipv4(configured_host, requested_ip),
        Host::Ipv6(requested_ip) => classify_ipv6(configured_host, requested_ip),
        Host::Domain(requested_domain) => classify_domain(configured_host, requested_domain),
    }
}

/// [`classify`]'s IPv4 branch: `SameView` only for an exact
/// [`Host::Ipv4`] match against `configured_host` — an IPv6 or domain
/// `configured_host` is always `External`, never a type coercion.
fn classify_ipv4(configured_host: Host<&str>, requested_ip: Ipv4Addr) -> Classification {
    match configured_host {
        Host::Ipv4(configured_ip) if configured_ip == requested_ip => Classification::SameView,
        _ => Classification::External,
    }
}

/// [`classify`]'s IPv6 branch, mirroring [`classify_ipv4`].
fn classify_ipv6(configured_host: Host<&str>, requested_ip: Ipv6Addr) -> Classification {
    match configured_host {
        Host::Ipv6(configured_ip) if configured_ip == requested_ip => Classification::SameView,
        _ => Classification::External,
    }
}

/// [`classify`]'s domain branch: `External` if `configured_host` is not
/// itself a domain (an IP-literal service URL can never match a domain
/// request), otherwise a [`comparison_key`] match.
fn classify_domain(configured_host: Host<&str>, requested_domain: &str) -> Classification {
    let Host::Domain(configured_domain) = configured_host else {
        return Classification::External;
    };
    if comparison_key(requested_domain) == comparison_key(configured_domain) {
        Classification::SameView
    } else {
        Classification::External
    }
}

/// The value two domains are compared by: `host`, with any trailing `.`
/// removed and ASCII-lowercased, then reduced to its registrable domain via
/// [`psl::domain`] — but only when that domain's suffix is
/// [`is_known`](psl::Suffix::is_known) (an ICANN or private entry in the
/// public suffix list). An unknown suffix (e.g. `.invalid`) keeps the full
/// normalized host instead of trusting an unlisted TLD's label split, so
/// two different subdomains under an unknown suffix never compare equal.
fn comparison_key(host: &str) -> String {
    let normalized = host.trim_end_matches('.').to_ascii_lowercase();
    match psl::domain(normalized.as_bytes()) {
        Some(domain) if domain.suffix().is_known() => {
            String::from_utf8_lossy(domain.as_bytes()).into_owned()
        }
        _ => normalized,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(raw: &str) -> Url {
        Url::parse(raw).expect("valid test URL")
    }

    #[test]
    fn same_view_domain_cases() {
        let cases: &[(&str, &str)] = &[
            // Registrable domain match across subdomains (Gmail's "Add
            // another account" flow).
            (
                "https://mail.google.com/mail/u/0/",
                "https://accounts.google.com/AddSession",
            ),
            // Host comparison is case-insensitive.
            ("https://mail.google.com/", "https://MAIL.GOOGLE.COM/"),
            // A fully-qualified trailing dot does not change the host.
            ("https://mail.google.com/", "https://mail.google.com./"),
        ];
        for (configured, requested) in cases {
            assert_eq!(
                classify(&url(configured), &url(requested)),
                Classification::SameView,
                "configured={configured} requested={requested}"
            );
        }
    }

    #[test]
    fn external_domain_cases() {
        let cases: &[(&str, &str)] = &[
            // Same known suffix (`co.uk`), different registrable domain.
            ("https://a.example.co.uk/", "https://b.other.co.uk/"),
            // `github.io` is a known *private* PSL suffix, so each
            // `<user>.github.io` is its own registrable domain.
            ("https://alice.github.io/", "https://bob.github.io/"),
            // Classic look-alike subdomain: `evil.com` is the registrable
            // domain, not `google.com`.
            ("https://mail.google.com/", "https://google.com.evil.com/"),
            // Userinfo never enters the comparison — the host here is
            // `evil.com`, not `mail.google.com`.
            (
                "https://mail.google.com/",
                "https://mail.google.com@evil.com/",
            ),
            // Wholly unrelated domain.
            (
                "https://mail.google.com/",
                "https://totally-unrelated.example/",
            ),
        ];
        for (configured, requested) in cases {
            assert_eq!(
                classify(&url(configured), &url(requested)),
                Classification::External,
                "configured={configured} requested={requested}"
            );
        }
    }

    #[test]
    fn unknown_suffix_falls_back_to_full_host_equality() {
        let cases: &[(&str, &str, Classification)] = &[
            (
                "https://example.invalid/",
                "https://example.invalid/",
                Classification::SameView,
            ),
            (
                "https://example.invalid/",
                "https://sub.example.invalid/",
                Classification::External,
            ),
        ];
        for (configured, requested, expected) in cases {
            assert_eq!(
                classify(&url(configured), &url(requested)),
                *expected,
                "configured={configured} requested={requested}"
            );
        }
    }

    #[test]
    fn localhost_and_ip_literals_need_an_exact_match() {
        let same_view: &[(&str, &str)] = &[
            ("http://localhost/", "http://localhost/"),
            // Port never enters the comparison.
            ("http://127.0.0.1/", "http://127.0.0.1:8080/"),
            ("http://[::1]/", "http://[::1]/"),
        ];
        for (configured, requested) in same_view {
            assert_eq!(
                classify(&url(configured), &url(requested)),
                Classification::SameView,
                "configured={configured} requested={requested}"
            );
        }

        let external: &[(&str, &str)] = &[
            // A domain host is never equal to an IP-literal host.
            ("http://localhost/", "http://127.0.0.1/"),
            ("http://127.0.0.1/", "http://127.0.0.2/"),
            ("http://[::1]/", "http://[::2]/"),
            // IPv4 and IPv6 are never conflated, even for the "same"
            // address in another family.
            ("http://127.0.0.1/", "http://[::1]/"),
            ("http://[::1]/", "http://127.0.0.1/"),
        ];
        for (configured, requested) in external {
            assert_eq!(
                classify(&url(configured), &url(requested)),
                Classification::External,
                "configured={configured} requested={requested}"
            );
        }
    }

    #[test]
    fn non_http_schemes_are_always_denied() {
        let configured = url("https://mail.google.com/");
        let requested_urls = [
            "javascript:alert(1)",
            "data:text/html,hi",
            "blob:https://mail.google.com/abcd-efgh",
            "file:///etc/passwd",
            "about:blank",
            "mailto:someone@example.com",
            "tel:+1234567890",
            "eluma-custom:action",
        ];
        for raw in requested_urls {
            let requested = Url::parse(raw).expect("valid test URL");
            assert_eq!(
                classify(&configured, &requested),
                Classification::Deny,
                "requested={raw}"
            );
        }
    }
}
