//! SSRF-resistant fetching of calendar subscription URLs.
//!
//! * Only http/https (webcal is rewritten to https); no credentials in URLs.
//! * Every resolved address must be public unless it falls inside an
//!   explicitly trusted network (`TENDLY_TRUSTED_FETCH_NETWORKS`).
//! * The connection is pinned to the validated addresses so DNS cannot be
//!   re-pointed between the check and the request (DNS rebinding).
//! * Redirects are followed manually (max 5) and each hop is re-validated.
//! * Responses are size-limited while streaming; timeouts are enforced.

use futures_util::StreamExt;
use ipnet::IpNet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;
use url::Url;

#[derive(Clone, Debug)]
pub struct FetchPolicy {
    pub trusted: Vec<IpNet>,
    pub max_bytes: usize,
    pub timeout: Duration,
    pub max_redirects: u8,
    pub use_proxy: bool,
}

impl Default for FetchPolicy {
    fn default() -> Self {
        FetchPolicy { trusted: vec![], max_bytes: 5 * 1024 * 1024, timeout: Duration::from_secs(20), max_redirects: 5, use_proxy: false }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FetchError {
    #[error("Only http, https and webcal links are supported.")]
    Scheme,
    #[error("That link is not a valid web address.")]
    InvalidUrl,
    #[error("Links with a username or password in them are not supported.")]
    Credentials,
    #[error("That address points to a private or internal network, which is blocked by default.")]
    BlockedAddress,
    #[error("The host name could not be resolved.")]
    Dns,
    #[error("Too many redirects.")]
    TooManyRedirects,
    #[error("The calendar is larger than allowed.")]
    TooLarge,
    #[error("The calendar server answered with status {0}.")]
    Status(u16),
    #[error("The calendar server could not be reached ({0}).")]
    Network(String),
    #[error("The calendar is not valid text.")]
    Encoding,
}

pub struct Fetched {
    pub body: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub not_modified: bool,
}

pub fn validate_url(raw: &str) -> Result<Url, FetchError> {
    let raw = raw.trim();
    let rewritten = if let Some(rest) = raw.strip_prefix("webcal://") {
        format!("https://{rest}")
    } else if let Some(rest) = raw.strip_prefix("webcals://") {
        format!("https://{rest}")
    } else {
        raw.to_string()
    };
    let url = Url::parse(&rewritten).map_err(|_| FetchError::InvalidUrl)?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(FetchError::Scheme);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(FetchError::Credentials);
    }
    if url.host_str().map(str::is_empty).unwrap_or(true) {
        return Err(FetchError::InvalidUrl);
    }
    if rewritten.len() > 2048 {
        return Err(FetchError::InvalidUrl);
    }
    Ok(url)
}

fn v4_blocked(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || o[0] == 0
        || (o[0] == 100 && (o[1] & 0xc0) == 64) // CGNAT 100.64/10
        || (o[0] == 198 && (o[1] & 0xfe) == 18) // benchmarking 198.18/15
        || (o[0] == 192 && o[1] == 0 && o[2] == 0) // IETF protocol assignments
        || o[0] >= 240 // reserved
}

fn v6_blocked(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return v4_blocked(v4);
    }
    let s = ip.segments();
    // NAT64 well-known prefix embeds an IPv4 address.
    if s[0] == 0x64 && s[1] == 0xff9b && s[2..6] == [0, 0, 0, 0] {
        let v4 = Ipv4Addr::new((s[6] >> 8) as u8, s[6] as u8, (s[7] >> 8) as u8, s[7] as u8);
        return v4_blocked(v4);
    }
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (s[0] & 0xfe00) == 0xfc00 // unique local
        || (s[0] & 0xffc0) == 0xfe80 // link local
        || (s[0] & 0xffc0) == 0xfec0 // site local (deprecated)
        || s[0] == 0x2001 && s[1] == 0x0db8 // documentation
        || s[0] == 0x2002 // 6to4 can tunnel to private v4
        || (s[0] == 0 && s[1] == 0 && s[2] == 0 && s[3] == 0 && s[4] == 0 && s[5] == 0) // IPv4-compatible
}

pub fn ip_allowed(ip: IpAddr, trusted: &[IpNet]) -> bool {
    if trusted.iter().any(|n| n.contains(&ip)) {
        return true;
    }
    match ip {
        IpAddr::V4(v4) => !v4_blocked(v4),
        IpAddr::V6(v6) => !v6_blocked(v6),
    }
}

async fn resolve_checked(url: &Url, policy: &FetchPolicy) -> Result<Vec<SocketAddr>, FetchError> {
    let host = url.host_str().ok_or(FetchError::InvalidUrl)?;
    let port = url.port_or_known_default().ok_or(FetchError::InvalidUrl)?;
    let host_for_lookup = host.trim_start_matches('[').trim_end_matches(']');
    let addrs: Vec<SocketAddr> = match host_for_lookup.parse::<IpAddr>() {
        Ok(ip) => vec![SocketAddr::new(ip, port)],
        Err(_) => tokio::net::lookup_host((host_for_lookup, port)).await.map_err(|_| FetchError::Dns)?.collect(),
    };
    if addrs.is_empty() {
        return Err(FetchError::Dns);
    }
    // Every address must be allowed; otherwise an attacker could mix one public
    // and one internal record and win the race.
    if addrs.iter().any(|a| !ip_allowed(a.ip(), &policy.trusted)) {
        return Err(FetchError::BlockedAddress);
    }
    Ok(addrs)
}

pub async fn fetch_calendar(raw_url: &str, policy: &FetchPolicy, etag: Option<&str>, last_modified: Option<&str>) -> Result<Fetched, FetchError> {
    let mut url = validate_url(raw_url)?;
    for _ in 0..=policy.max_redirects {
        let addrs = resolve_checked(&url, policy).await?;
        let host = url.host_str().ok_or(FetchError::InvalidUrl)?.trim_start_matches('[').trim_end_matches(']').to_string();
        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(policy.timeout)
            .connect_timeout(Duration::from_secs(8))
            .user_agent(concat!("Tendly/", env!("CARGO_PKG_VERSION"), " (calendar subscription)"));
        if host.parse::<IpAddr>().is_err() {
            builder = builder.resolve_to_addrs(&host, &addrs);
        }
        if !policy.use_proxy {
            builder = builder.no_proxy();
        }
        let client = builder.build().map_err(|e| FetchError::Network(e.to_string()))?;
        let mut req = client.get(url.clone()).header("accept", "text/calendar, text/plain;q=0.8, */*;q=0.1");
        if let Some(e) = etag {
            req = req.header("if-none-match", e);
        }
        if let Some(lm) = last_modified {
            req = req.header("if-modified-since", lm);
        }
        let resp = req.send().await.map_err(|e| FetchError::Network(if e.is_timeout() { "timed out".into() } else { "connection failed".into() }))?;
        let status = resp.status();
        if status.is_redirection() && status.as_u16() != 304 {
            let loc = resp.headers().get("location").and_then(|v| v.to_str().ok()).ok_or(FetchError::Status(status.as_u16()))?;
            let next = url.join(loc).map_err(|_| FetchError::InvalidUrl)?;
            url = validate_url(next.as_str())?;
            continue;
        }
        if status.as_u16() == 304 {
            return Ok(Fetched { body: None, etag: etag.map(String::from), last_modified: last_modified.map(String::from), not_modified: true });
        }
        if !status.is_success() {
            return Err(FetchError::Status(status.as_u16()));
        }
        if resp.content_length().map(|l| l as usize > policy.max_bytes).unwrap_or(false) {
            return Err(FetchError::TooLarge);
        }
        let header = |n: &str| resp.headers().get(n).and_then(|v| v.to_str().ok()).map(|s| s.chars().take(200).collect::<String>());
        let new_etag = header("etag");
        let new_lm = header("last-modified");
        let mut body: Vec<u8> = Vec::new();
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| FetchError::Network("interrupted".into()))?;
            if body.len() + chunk.len() > policy.max_bytes {
                return Err(FetchError::TooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        let text = String::from_utf8(body).map_err(|_| FetchError::Encoding)?;
        return Ok(Fetched { body: Some(text), etag: new_etag, last_modified: new_lm, not_modified: false });
    }
    Err(FetchError::TooManyRedirects)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use axum::Router;

    #[test]
    fn url_validation() {
        assert_eq!(validate_url("webcal://cal.example.com/a.ics").unwrap().scheme(), "https");
        assert_eq!(validate_url("ftp://x/y"), Err(FetchError::Scheme));
        assert_eq!(validate_url("file:///etc/passwd"), Err(FetchError::Scheme));
        assert_eq!(validate_url("https://user:pw@cal.example.com/"), Err(FetchError::Credentials));
        assert_eq!(validate_url("not a url"), Err(FetchError::InvalidUrl));
    }

    #[test]
    fn address_policy() {
        let blocked = [
            "127.0.0.1", "10.1.2.3", "192.168.0.10", "172.16.5.4", "169.254.169.254", "100.64.0.1", "0.0.0.0",
            "::1", "fd00::1", "fe80::1", "::ffff:127.0.0.1", "::ffff:169.254.169.254", "64:ff9b::a9fe:a9fe", "2002:7f00:1::1",
            "198.18.0.1", "255.255.255.255", "224.0.0.1",
        ];
        for b in blocked {
            assert!(!ip_allowed(b.parse().unwrap(), &[]), "{b} should be blocked");
        }
        for ok in ["93.184.216.34", "2606:4700:4700::1111", "1.1.1.1"] {
            assert!(ip_allowed(ok.parse().unwrap(), &[]), "{ok} should be allowed");
        }
        let trusted: Vec<IpNet> = vec!["192.168.1.0/24".parse().unwrap()];
        assert!(ip_allowed("192.168.1.20".parse().unwrap(), &trusted));
        assert!(!ip_allowed("192.168.2.20".parse().unwrap(), &trusted));
    }

    async fn serve(router: Router) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        addr
    }

    fn trusted_local() -> FetchPolicy {
        FetchPolicy { trusted: vec!["127.0.0.0/8".parse().unwrap()], ..FetchPolicy::default() }
    }

    #[tokio::test]
    async fn blocks_loopback_by_default_and_fetches_when_trusted() {
        let addr = serve(Router::new().route("/cal.ics", get(|| async { ([("etag", "\"v1\"")], "BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n") }))).await;
        let url = format!("http://{addr}/cal.ics");
        assert_eq!(fetch_calendar(&url, &FetchPolicy::default(), None, None).await.err(), Some(FetchError::BlockedAddress));
        let got = fetch_calendar(&url, &trusted_local(), None, None).await.unwrap();
        assert!(got.body.unwrap().starts_with("BEGIN:VCALENDAR"));
        assert_eq!(got.etag.as_deref(), Some("\"v1\""));
    }

    #[tokio::test]
    async fn redirect_to_metadata_is_blocked() {
        let addr = serve(Router::new().route(
            "/r",
            get(|| async { (axum::http::StatusCode::FOUND, [("location", "http://169.254.169.254/latest/meta-data/")], "") }),
        ))
        .await;
        let res = fetch_calendar(&format!("http://{addr}/r"), &trusted_local(), None, None).await;
        assert_eq!(res.err(), Some(FetchError::BlockedAddress));
    }

    #[tokio::test]
    async fn oversized_and_redirect_loops() {
        let addr = serve(
            Router::new()
                .route("/big", get(|| async { "x".repeat(2000) }))
                .route("/loop", get(|| async { (axum::http::StatusCode::FOUND, [("location", "/loop")], "") })),
        )
        .await;
        let small = FetchPolicy { max_bytes: 1000, ..trusted_local() };
        assert_eq!(fetch_calendar(&format!("http://{addr}/big"), &small, None, None).await.err(), Some(FetchError::TooLarge));
        assert_eq!(fetch_calendar(&format!("http://{addr}/loop"), &trusted_local(), None, None).await.err(), Some(FetchError::TooManyRedirects));
    }
}
