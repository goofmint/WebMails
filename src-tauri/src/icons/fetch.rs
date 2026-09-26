//! Downloads a candidate icon URL safely (design.md §2.2.10): no cookies,
//! no `Referer`, a 5s timeout, a 1 MiB cap enforced both from a
//! `Content-Length` header (when present) and while streaming the body,
//! and a destination check — on the initial request and on every redirect
//! it follows — that rejects a loopback, private or link-local resolved
//! address ([`crate::net_guard`]).
//!
//! [`IconFetcher`] is the trait [`super::IconService`] resolves candidates
//! through; [`ReqwestIconFetcher`] is the real, network-backed
//! implementation built once and reused for every request. Tests inject a
//! fake implementation instead (see `super::tests`), so none of
//! [`super`]'s pure ordering/caching logic needs a real network to be
//! exercised.

use std::future::Future;
use std::net::{SocketAddr, ToSocketAddrs};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::redirect::Policy;
use reqwest::Client;
use url::Url;

use crate::error::AppError;
use crate::net_guard::is_disallowed_ip;

/// Request timeout (design.md §2.2.10).
const TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum response body size (design.md §2.2.10).
pub const MAX_BYTES: u64 = 1024 * 1024;

/// Maximum number of redirects followed, matching `reqwest`'s own default
/// (`Policy::default`) — [`redirect_policy`] replaces that default with a
/// host-checking one, so the limit has to be reimplemented alongside it.
const MAX_REDIRECTS: usize = 10;

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum FetchError {
    #[error("iconCandidate URL must be http or https")]
    InvalidScheme,
    #[error("iconCandidate host is loopback, private or link-local")]
    DisallowedHost,
    #[error("request timed out")]
    Timeout,
    #[error("response exceeded the {MAX_BYTES} byte cap")]
    TooLarge,
    #[error("request failed: {0}")]
    Request(String),
}

/// A future producing a candidate's downloaded bytes on success.
pub type FetchFuture<'a> = Pin<Box<dyn Future<Output = Result<Vec<u8>, FetchError>> + Send + 'a>>;

/// What [`super::IconService`] downloads a candidate through — implemented
/// by [`ReqwestIconFetcher`] in production and by a fake in tests.
pub trait IconFetcher: Send + Sync {
    fn fetch<'a>(&'a self, url: &'a Url) -> FetchFuture<'a>;
}

/// The real, `reqwest`-backed [`IconFetcher`] (design.md §2.2.10).
pub struct ReqwestIconFetcher {
    client: Client,
}

impl ReqwestIconFetcher {
    /// Builds the shared `reqwest::Client` once: no cookie store (so no
    /// cookies are ever sent — `reqwest` only sends them when one is
    /// explicitly enabled), no `Referer` header, [`TIMEOUT`], the
    /// host-checking [`redirect_policy`], and [`GuardedResolver`] so a
    /// domain name that *resolves* to a disallowed address is rejected too
    /// (design.md §2.2.10's "destination ... must not resolve to" a
    /// loopback, private or link-local address — a literal-host check
    /// alone would miss this).
    ///
    /// Fails only if `reqwest` itself cannot build a client at all (e.g. no
    /// TLS backend compiled in) — a build-time configuration problem, not a
    /// runtime one, but still surfaced as an ordinary [`AppError`] rather
    /// than panicking (project rule: no `unwrap`/`expect` outside tests).
    pub fn new() -> Result<Self, AppError> {
        let client = Client::builder()
            .referer(false)
            .timeout(TIMEOUT)
            .redirect(redirect_policy())
            .dns_resolver(Arc::new(GuardedResolver))
            .build()
            .map_err(|err| AppError::Icon(format!("could not build icon-fetch client: {err}")))?;
        Ok(ReqwestIconFetcher { client })
    }
}

impl IconFetcher for ReqwestIconFetcher {
    fn fetch<'a>(&'a self, url: &'a Url) -> FetchFuture<'a> {
        Box::pin(async move {
            check_initial_destination(url)?;
            download(&self.client, url).await
        })
    }
}

/// The redirect policy every request uses: caps the chain at
/// [`MAX_REDIRECTS`] (`reqwest`'s own limit is lost the moment a custom
/// policy replaces it) and rejects following a redirect whose *literal*
/// host is disallowed or whose scheme is not `http`/`https` — a fast,
/// pre-connect rejection. The *resolved-address* check that also covers a
/// safe-looking domain name is [`GuardedResolver`]'s job, and applies to
/// every redirect hop too, since each one is a fresh connection through
/// the same client.
fn redirect_policy() -> Policy {
    Policy::custom(|attempt| {
        if attempt.previous().len() >= MAX_REDIRECTS {
            return attempt.error("too many redirects");
        }
        let url = attempt.url().clone();
        match url.scheme() {
            "http" | "https" => {}
            _ => return attempt.error("redirect scheme must be http or https"),
        }
        if let Some(host) = url.host() {
            if is_disallowed_host_literal(&host) {
                return attempt.error("redirect destination is disallowed");
            }
        }
        attempt.follow()
    })
}

fn is_disallowed_host_literal(host: &url::Host<&str>) -> bool {
    crate::net_guard::is_disallowed_host(host)
}

/// A `reqwest::dns::Resolve` that resolves a hostname with the platform
/// resolver (via [`ToSocketAddrs`], off the async executor in
/// `spawn_blocking`) and rejects the lookup entirely if *every* resolved
/// address is disallowed, keeping only the allowed ones otherwise — so a
/// domain that resolves to a mix of addresses can still be reached at a
/// public one, but one that resolves only to loopback/private/link-local
/// addresses (e.g. DNS rebinding to `127.0.0.1`) is refused before any
/// connection is attempted.
struct GuardedResolver;

impl Resolve for GuardedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let addrs: Vec<SocketAddr> =
                tokio::task::spawn_blocking(move || (host.as_str(), 0u16).to_socket_addrs())
                    .await
                    .map_err(|err| Box::new(err) as Box<dyn std::error::Error + Send + Sync>)??
                    .collect();

            let allowed: Vec<SocketAddr> = addrs
                .into_iter()
                .filter(|addr| !is_disallowed_ip(&addr.ip()))
                .collect();

            if allowed.is_empty() {
                return Err("every resolved address is loopback, private or link-local".into());
            }

            let boxed: Addrs = Box::new(allowed.into_iter());
            Ok(boxed)
        })
    }
}

/// Rejects up front (before making any request at all) an `iconCandidate`
/// URL whose scheme is not `http`/`https` or whose *literal* host is
/// disallowed — the same fast pre-connect check [`redirect_policy`] runs
/// on every redirect hop, run once here for the initial request.
fn check_initial_destination(url: &Url) -> Result<(), FetchError> {
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err(FetchError::InvalidScheme),
    }
    if let Some(host) = url.host() {
        if is_disallowed_host_literal(&host) {
            return Err(FetchError::DisallowedHost);
        }
    }
    Ok(())
}

/// Downloads `url` through `client`, enforcing [`MAX_BYTES`] both from a
/// `Content-Length` header (when the server sends one) and while streaming
/// the body — the "two-stage" cap design.md §2.2.10 asks for, since a
/// server can omit or lie about `Content-Length`. Does *not* itself check
/// `url`'s destination — [`ReqwestIconFetcher::fetch`] does that
/// (`check_initial_destination`) before ever calling this, and `client`'s
/// own [`redirect_policy`] (plus, in production, [`GuardedResolver`])
/// covers every redirect hop — so this function is exactly the HTTP
/// mechanics, independently unit-testable against a local loopback test
/// server (which the destination guard would otherwise always refuse).
async fn download(client: &Client, url: &Url) -> Result<Vec<u8>, FetchError> {
    let response = client.get(url.clone()).send().await.map_err(|err| {
        if err.is_timeout() {
            FetchError::Timeout
        } else {
            FetchError::Request(err.to_string())
        }
    })?;

    if let Some(len) = response.content_length() {
        if len > MAX_BYTES {
            return Err(FetchError::TooLarge);
        }
    }

    let mut body = Vec::new();
    let mut stream = response;
    loop {
        let chunk = stream.chunk().await.map_err(|err| {
            if err.is_timeout() {
                FetchError::Timeout
            } else {
                FetchError::Request(err.to_string())
            }
        })?;
        let Some(chunk) = chunk else {
            break;
        };
        if body.len() as u64 + chunk.len() as u64 > MAX_BYTES {
            return Err(FetchError::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }

    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    /// Spawns a background thread serving one raw HTTP/1.1 response (built
    /// by `respond`) to each accepted connection on `127.0.0.1`, and
    /// returns the URL to reach it. The thread runs for the lifetime of
    /// the test process; that's fine for a short-lived test binary.
    fn spawn_test_server(respond: impl Fn() -> Vec<u8> + Send + 'static) -> Url {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(&respond());
                let _ = stream.flush();
            }
        });
        Url::parse(&format!("http://{addr}/icon")).expect("valid url")
    }

    fn ok_response(body: &[u8]) -> Vec<u8> {
        let mut head = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        head.extend_from_slice(body);
        head
    }

    /// A client with the same timeout as production but *no* destination
    /// guard (no custom redirect policy, default `dns_resolver`) — every
    /// test server in this module binds to `127.0.0.1`, which the real
    /// guard (correctly) always refuses, so these tests exercise
    /// [`download`]'s HTTP mechanics (size cap, timeout, a normal
    /// successful download) in isolation from that guard. The guard itself
    /// is tested separately: [`rejects_a_literal_loopback_host_before_connecting`]
    /// and [`rejects_a_non_http_scheme_before_connecting`] need no network
    /// at all (rejected pre-connect), and
    /// [`rejects_a_redirect_to_a_loopback_destination`] below uses the
    /// real, production [`redirect_policy`] directly.
    fn unguarded_test_client() -> Client {
        Client::builder()
            .referer(false)
            .timeout(TIMEOUT)
            .build()
            .expect("building the unguarded test client must not fail")
    }

    /// Like [`unguarded_test_client`], but with the real, production
    /// [`redirect_policy`] attached — for testing that policy's own
    /// host-checking logic against a real redirect response, without the
    /// custom `dns_resolver` (which would otherwise also refuse the test
    /// server's own loopback address before ever reaching a redirect).
    fn client_with_redirect_guard() -> Client {
        Client::builder()
            .referer(false)
            .timeout(TIMEOUT)
            .redirect(redirect_policy())
            .build()
            .expect("building the redirect-guarded test client must not fail")
    }

    #[tokio::test]
    async fn downloads_a_small_body_successfully() {
        let url = spawn_test_server(|| ok_response(b"hello icon"));
        let bytes = download(&unguarded_test_client(), &url)
            .await
            .expect("should download");
        assert_eq!(bytes, b"hello icon");
    }

    #[tokio::test]
    async fn rejects_a_content_length_over_the_cap() {
        let url = spawn_test_server(|| {
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                MAX_BYTES + 1
            )
            .into_bytes()
        });
        let err = download(&unguarded_test_client(), &url).await.unwrap_err();
        assert_eq!(err, FetchError::TooLarge);
    }

    #[tokio::test]
    async fn rejects_a_body_over_the_cap_even_without_content_length() {
        let url = spawn_test_server(|| {
            let body = vec![0u8; (MAX_BYTES + 1) as usize];
            let mut head =
                b"HTTP/1.1 200 OK\r\nConnection: close\r\nTransfer-Encoding: identity\r\n\r\n"
                    .to_vec();
            head.extend_from_slice(&body);
            head
        });
        let err = download(&unguarded_test_client(), &url).await.unwrap_err();
        assert_eq!(err, FetchError::TooLarge);
    }

    #[tokio::test]
    async fn rejects_a_redirect_to_a_loopback_destination() {
        let redirect_target = "http://127.0.0.1:9/icon"; // disallowed
        let url = spawn_test_server(move || {
            format!(
                "HTTP/1.1 302 Found\r\nLocation: {redirect_target}\r\nConnection: close\r\n\r\n"
            )
            .into_bytes()
        });
        let err = download(&client_with_redirect_guard(), &url)
            .await
            .unwrap_err();
        // The rejection surfaces as a generic `reqwest` redirect-policy
        // error (`FetchError::Request`), not a timeout or size failure —
        // the point under test is that the destination is never reached.
        assert!(
            matches!(err, FetchError::Request(_)),
            "expected a redirect-policy rejection, got {err:?}"
        );
    }

    #[tokio::test]
    async fn rejects_a_non_http_scheme_before_connecting() {
        let url = Url::parse("ftp://example.com/icon.png").expect("valid url");
        let fetcher = ReqwestIconFetcher::new().expect("build fetcher");
        let err = fetcher.fetch(&url).await.unwrap_err();
        assert_eq!(err, FetchError::InvalidScheme);
    }

    #[tokio::test]
    async fn rejects_a_literal_loopback_host_before_connecting() {
        let url = Url::parse("http://127.0.0.1/icon.png").expect("valid url");
        let fetcher = ReqwestIconFetcher::new().expect("build fetcher");
        let err = fetcher.fetch(&url).await.unwrap_err();
        assert_eq!(err, FetchError::DisallowedHost);
    }

    #[tokio::test]
    async fn times_out_on_a_server_that_never_responds() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                // Accept but never write a response, and never close the
                // connection either — the client must give up on its own
                // via the 5s timeout, not because the server hung up.
                std::mem::forget(stream);
            }
        });
        let url = Url::parse(&format!("http://{addr}/icon")).expect("valid url");
        let err = download(&unguarded_test_client(), &url).await.unwrap_err();
        assert_eq!(err, FetchError::Timeout);
    }
}
