//! SSRF-safe fetching for user-supplied URLs (feeds, discovery, extraction).
//!
//! Three defenses, applied at fetch time (not just at input validation, so a
//! DNS record that changes after subscription is still caught):
//!   1. `UrlGuard` — scheme allowlist + IP-range checks. Hostnames are resolved
//!      and *every* resolved address must be acceptable.
//!   2. `safe_get` — redirects are never followed automatically; each hop is
//!      re-validated with the guard before the next request is sent.
//!   3. `read_body_limited` / `read_body_truncated` — response bodies are read
//!      in chunks with a hard cap, so a huge (or endless) body cannot OOM the
//!      process.
//!
//! Fixed, trusted endpoints (Anthropic API, push services, Instapaper) do NOT
//! go through this module; they keep using the plain shared client.

use std::net::IpAddr;

use reqwest::{Client, RequestBuilder, Response, Url};

use super::error::{AppError, AppResult};

/// Maximum number of redirect hops `safe_get` will follow manually.
const MAX_REDIRECTS: usize = 5;

/// Policy for which destination addresses an outbound fetch may touch.
///
/// Defaults are the safe ones (everything non-global blocked). A home-LAN
/// deployment that legitimately subscribes to feeds on private addresses can
/// opt out via `ALLOW_PRIVATE_NETWORKS=true` (`allow_private` + `allow_loopback`).
#[derive(Debug, Clone, Copy, Default)]
pub struct UrlGuard {
    /// Permit RFC1918 / link-local / CGNAT / ULA etc. (private ranges).
    pub allow_private: bool,
    /// Permit 127.0.0.0/8 and ::1.
    pub allow_loopback: bool,
}

/// Classification of an IP address for the guard's decision.
#[derive(Debug, PartialEq, Eq)]
enum IpClass {
    /// Publicly routable — always OK.
    Global,
    /// Private-ish (RFC1918, link-local, CGNAT, ULA, documentation...) —
    /// OK only with `allow_private`.
    Private,
    /// Loopback — OK only with `allow_loopback`.
    Loopback,
    /// Never acceptable (unspecified, multicast, broadcast, reserved).
    Blocked,
}

fn classify_v4(ip: std::net::Ipv4Addr) -> IpClass {
    let o = ip.octets();
    if ip.is_loopback() {
        return IpClass::Loopback;
    }
    if ip.is_unspecified() || o[0] == 0 || ip.is_multicast() || ip.is_broadcast() || o[0] >= 240 {
        return IpClass::Blocked;
    }
    let private = ip.is_private()                      // 10/8, 172.16/12, 192.168/16
        || ip.is_link_local()                          // 169.254/16 (cloud metadata lives here)
        || (o[0] == 100 && (o[1] & 0xC0) == 64)        // 100.64/10 CGNAT
        || (o[0] == 192 && o[1] == 0 && o[2] == 0)     // 192.0.0/24 IETF protocol
        || (o[0] == 192 && o[1] == 0 && o[2] == 2)     // 192.0.2/24 TEST-NET-1
        || (o[0] == 198 && o[1] == 51 && o[2] == 100)  // TEST-NET-2
        || (o[0] == 203 && o[1] == 0 && o[2] == 113)   // TEST-NET-3
        || (o[0] == 198 && (o[1] & 0xFE) == 18); // 198.18/15 benchmark
    if private {
        IpClass::Private
    } else {
        IpClass::Global
    }
}

fn classify_v6(ip: std::net::Ipv6Addr) -> IpClass {
    // IPv4-mapped (::ffff:a.b.c.d) inherits the IPv4 classification, so the
    // v4 ranges cannot be smuggled past the guard in v6 clothing.
    if let Some(v4) = ip.to_ipv4_mapped() {
        return classify_v4(v4);
    }
    if ip.is_loopback() {
        return IpClass::Loopback;
    }
    if ip.is_unspecified() || ip.is_multicast() {
        return IpClass::Blocked;
    }
    let seg = ip.segments();
    let private = (seg[0] & 0xFE00) == 0xFC00          // fc00::/7 ULA
        || (seg[0] & 0xFFC0) == 0xFE80                 // fe80::/10 link-local
        || (seg[0] == 0x2001 && seg[1] == 0x0DB8); // 2001:db8::/32 documentation
    if private {
        IpClass::Private
    } else {
        IpClass::Global
    }
}

impl UrlGuard {
    /// Build from app config (single `ALLOW_PRIVATE_NETWORKS` switch).
    pub fn from_config(config: &super::config::AppConfig) -> Self {
        Self {
            allow_private: config.allow_private_networks,
            allow_loopback: config.allow_private_networks,
        }
    }

    /// Is this concrete address acceptable under the policy?
    fn ip_allowed(&self, ip: IpAddr) -> bool {
        let class = match ip {
            IpAddr::V4(v4) => classify_v4(v4),
            IpAddr::V6(v6) => classify_v6(v6),
        };
        match class {
            IpClass::Global => true,
            IpClass::Private => self.allow_private,
            IpClass::Loopback => self.allow_loopback,
            IpClass::Blocked => false,
        }
    }

    /// Validate one URL: scheme must be http(s); the host (literal IP, or every
    /// address the hostname resolves to) must be acceptable.
    ///
    /// Note: the subsequent request re-resolves DNS, so a fast-flux record could
    /// in theory swap between check and connect (TOCTOU). This still removes the
    /// practical SSRF surface for a self-hosted reader; pinning resolved IPs is
    /// intentionally out of scope.
    pub async fn check_url(&self, url: &Url) -> Result<(), String> {
        if !matches!(url.scheme(), "http" | "https") {
            return Err(format!("scheme not allowed: {}", url.scheme()));
        }
        let host = url
            .host_str()
            .ok_or_else(|| "url has no host".to_string())?;
        // IPv6 literals come back bracketed ("[::1]"); strip for parsing.
        let bare = host.trim_start_matches('[').trim_end_matches(']');
        if let Ok(ip) = bare.parse::<IpAddr>() {
            if !self.ip_allowed(ip) {
                return Err(format!("address not allowed by policy: {ip}"));
            }
            return Ok(());
        }
        let port = url.port_or_known_default().unwrap_or(443);
        let addrs: Vec<_> = tokio::net::lookup_host((bare, port))
            .await
            .map_err(|e| format!("dns resolution failed for {bare}: {e}"))?
            .collect();
        if addrs.is_empty() {
            return Err(format!("dns resolved no addresses for {bare}"));
        }
        // ALL resolved addresses must pass — a mixed record (one public,
        // one internal) is treated as hostile.
        for addr in addrs {
            if !self.ip_allowed(addr.ip()) {
                return Err(format!(
                    "address not allowed by policy: {bare} -> {}",
                    addr.ip()
                ));
            }
        }
        Ok(())
    }
}

/// GET a user-supplied URL with per-hop SSRF validation.
///
/// `client` must be built with `redirect::Policy::none()` (see `http_external`
/// in `AppState`) — this function follows redirects itself so that every hop
/// passes through `guard.check_url`. `customize` lets call sites add headers /
/// timeouts without this module growing options.
pub async fn safe_get<F>(
    client: &Client,
    guard: &UrlGuard,
    url: &str,
    customize: F,
) -> AppResult<Response>
where
    F: Fn(RequestBuilder) -> RequestBuilder,
{
    let mut current =
        Url::parse(url).map_err(|e| AppError::Validation(format!("invalid url: {e}")))?;

    for _hop in 0..=MAX_REDIRECTS {
        guard
            .check_url(&current)
            .await
            .map_err(AppError::Validation)?;

        let resp = customize(client.get(current.clone()))
            .send()
            .await
            .map_err(|e| AppError::Upstream(e.to_string()))?;

        if resp.status().is_redirection() {
            let location = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| AppError::Upstream("redirect without location".into()))?;
            // Relative Location is resolved against the current URL.
            current = current
                .join(location)
                .map_err(|e| AppError::Upstream(format!("bad redirect target: {e}")))?;
            continue;
        }

        return resp
            .error_for_status()
            .map_err(|e| AppError::Upstream(e.to_string()));
    }

    Err(AppError::Upstream(format!(
        "too many redirects (>{MAX_REDIRECTS})"
    )))
}

/// Read a response body in chunks, failing once it exceeds `max_bytes`.
/// Unlike `Response::bytes()`, this aborts mid-stream so an attacker-sized
/// body cannot exhaust memory.
pub async fn read_body_limited(mut resp: Response, max_bytes: usize) -> AppResult<Vec<u8>> {
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| AppError::Upstream(e.to_string()))?
    {
        if buf.len() + chunk.len() > max_bytes {
            return Err(AppError::Validation(format!(
                "response body exceeds {max_bytes} bytes"
            )));
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

/// Like `read_body_limited` but truncates instead of failing (used by feed
/// discovery, where the interesting `<link>` tags live near the top anyway).
pub async fn read_body_truncated(mut resp: Response, max_bytes: usize) -> AppResult<Vec<u8>> {
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| AppError::Upstream(e.to_string()))?
    {
        let room = max_bytes.saturating_sub(buf.len());
        if room == 0 {
            break;
        }
        let take = room.min(chunk.len());
        buf.extend_from_slice(&chunk[..take]);
        if take < chunk.len() {
            break;
        }
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::Redirect;
    use axum::routing::get;
    use axum::Router;

    fn v4(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    // ---- IP classification -------------------------------------------------

    #[test]
    fn blocks_loopback_private_and_reserved_by_default() {
        let guard = UrlGuard::default();
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254", // cloud metadata
            "100.64.0.1",      // CGNAT
            "0.0.0.0",
            "224.0.0.1",       // multicast
            "255.255.255.255", // broadcast
            "240.0.0.1",       // reserved
            "192.0.2.1",       // TEST-NET-1
            "198.51.100.7",    // TEST-NET-2
            "203.0.113.9",     // TEST-NET-3
            "198.18.0.1",      // benchmark
        ] {
            assert!(!guard.ip_allowed(v4(ip)), "{ip} should be blocked");
        }
        for ip in ["::1", "fc00::1", "fd12::1", "fe80::1", "ff02::1", "::"] {
            assert!(
                !guard.ip_allowed(ip.parse().unwrap()),
                "{ip} should be blocked"
            );
        }
        // IPv4-mapped IPv6 must not smuggle private ranges through.
        assert!(!guard.ip_allowed("::ffff:192.168.1.1".parse().unwrap()));
        assert!(!guard.ip_allowed("::ffff:10.0.0.1".parse().unwrap()));
    }

    #[test]
    fn allows_global_addresses() {
        let guard = UrlGuard::default();
        for ip in ["93.184.216.34", "1.1.1.1", "8.8.8.8"] {
            assert!(guard.ip_allowed(v4(ip)), "{ip} should be allowed");
        }
        assert!(guard.ip_allowed("2606:4700::1111".parse().unwrap()));
    }

    #[test]
    fn allow_flags_open_up_private_and_loopback_but_not_blocked() {
        let guard = UrlGuard {
            allow_private: true,
            allow_loopback: true,
        };
        assert!(guard.ip_allowed(v4("10.0.0.1")));
        assert!(guard.ip_allowed(v4("192.168.1.1")));
        assert!(guard.ip_allowed(v4("127.0.0.1")));
        assert!(guard.ip_allowed("::1".parse().unwrap()));
        // Multicast / unspecified stay blocked regardless.
        assert!(!guard.ip_allowed(v4("224.0.0.1")));
        assert!(!guard.ip_allowed(v4("0.0.0.0")));
    }

    #[test]
    fn loopback_flag_alone_keeps_private_blocked() {
        let guard = UrlGuard {
            allow_private: false,
            allow_loopback: true,
        };
        assert!(guard.ip_allowed(v4("127.0.0.1")));
        assert!(!guard.ip_allowed(v4("10.0.0.1")));
        assert!(!guard.ip_allowed(v4("169.254.169.254")));
    }

    // ---- check_url ---------------------------------------------------------

    #[tokio::test]
    async fn check_url_rejects_non_http_schemes() {
        let guard = UrlGuard::default();
        let url = Url::parse("ftp://example.com/feed.xml").unwrap();
        assert!(guard.check_url(&url).await.is_err());
        let url = Url::parse("file:///etc/passwd").unwrap();
        assert!(guard.check_url(&url).await.is_err());
    }

    #[tokio::test]
    async fn check_url_rejects_private_ip_literals() {
        let guard = UrlGuard::default();
        for u in [
            "http://127.0.0.1:8080/",
            "http://192.168.1.1/feed",
            "http://169.254.169.254/latest/meta-data/",
            "http://[::1]/",
            "http://[fd00::1]/",
        ] {
            let url = Url::parse(u).unwrap();
            assert!(guard.check_url(&url).await.is_err(), "{u} should fail");
        }
    }

    #[tokio::test]
    async fn check_url_rejects_hostname_resolving_to_loopback() {
        let guard = UrlGuard::default();
        // "localhost" resolves via the hosts file — no external DNS needed.
        let url = Url::parse("http://localhost:9/feed").unwrap();
        assert!(guard.check_url(&url).await.is_err());
    }

    #[tokio::test]
    async fn check_url_allows_loopback_when_permitted() {
        let guard = UrlGuard {
            allow_private: false,
            allow_loopback: true,
        };
        let url = Url::parse("http://127.0.0.1:8080/feed").unwrap();
        assert!(guard.check_url(&url).await.is_ok());
    }

    // ---- safe_get redirect handling -----------------------------------------

    /// Spawn a tiny local server; returns its base URL.
    async fn spawn_server(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}")
    }

    fn no_redirect_client() -> Client {
        Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap()
    }

    /// Guard for local-server tests: loopback allowed, private still blocked,
    /// which lets us assert per-hop blocking with a real redirect.
    fn test_guard() -> UrlGuard {
        UrlGuard {
            allow_private: false,
            allow_loopback: true,
        }
    }

    #[tokio::test]
    async fn safe_get_follows_redirects_to_final_response() {
        let app = Router::new()
            .route("/start", get(|| async { Redirect::permanent("/final") }))
            .route("/final", get(|| async { "ok" }));
        let base = spawn_server(app).await;

        let resp = safe_get(
            &no_redirect_client(),
            &test_guard(),
            &format!("{base}/start"),
            |rb| rb,
        )
        .await
        .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().await.unwrap(), "ok");
    }

    #[tokio::test]
    async fn safe_get_blocks_redirect_to_private_address() {
        let app = Router::new().route(
            "/start",
            get(|| async { Redirect::permanent("http://192.168.1.1/steal") }),
        );
        let base = spawn_server(app).await;

        let err = safe_get(
            &no_redirect_client(),
            &test_guard(),
            &format!("{base}/start"),
            |rb| rb,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, AppError::Validation(ref m) if m.contains("not allowed")),
            "unexpected error: {err:?}"
        );
    }

    #[tokio::test]
    async fn safe_get_gives_up_after_max_redirects() {
        let app = Router::new().route("/loop", get(|| async { Redirect::permanent("/loop") }));
        let base = spawn_server(app).await;

        let err = safe_get(
            &no_redirect_client(),
            &test_guard(),
            &format!("{base}/loop"),
            |rb| rb,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, AppError::Upstream(ref m) if m.contains("too many redirects")),
            "unexpected error: {err:?}"
        );
    }

    #[tokio::test]
    async fn safe_get_rejects_initial_private_url() {
        // No server needed — blocked before any connection.
        let err = safe_get(
            &no_redirect_client(),
            &UrlGuard::default(),
            "http://169.254.169.254/latest/meta-data/",
            |rb| rb,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    // ---- body caps -----------------------------------------------------------

    #[tokio::test]
    async fn read_body_limited_rejects_oversized_body() {
        let app = Router::new().route("/big", get(|| async { "x".repeat(64 * 1024) }));
        let base = spawn_server(app).await;

        let resp = safe_get(
            &no_redirect_client(),
            &test_guard(),
            &format!("{base}/big"),
            |rb| rb,
        )
        .await
        .unwrap();
        let err = read_body_limited(resp, 1024).await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[tokio::test]
    async fn read_body_limited_accepts_body_within_cap() {
        let app = Router::new().route("/small", get(|| async { "hello" }));
        let base = spawn_server(app).await;

        let resp = safe_get(
            &no_redirect_client(),
            &test_guard(),
            &format!("{base}/small"),
            |rb| rb,
        )
        .await
        .unwrap();
        let body = read_body_limited(resp, 1024).await.unwrap();
        assert_eq!(body, b"hello");
    }

    #[tokio::test]
    async fn read_body_truncated_caps_without_error() {
        let app = Router::new().route("/big", get(|| async { "x".repeat(64 * 1024) }));
        let base = spawn_server(app).await;

        let resp = safe_get(
            &no_redirect_client(),
            &test_guard(),
            &format!("{base}/big"),
            |rb| rb,
        )
        .await
        .unwrap();
        let body = read_body_truncated(resp, 1000).await.unwrap();
        assert_eq!(body.len(), 1000);
    }
}
