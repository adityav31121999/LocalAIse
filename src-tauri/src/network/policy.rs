//! Network policy: what kinds of outbound requests are allowed.
//!
//! The policy is intentionally restrictive:
//!   - llama-server on localhost is always allowed.
//!   - All public internet requests require `internet_enabled == true`.
//!   - No requests to private IP ranges other than 127.x.x.x are allowed.
//!   - No SSRF: only http/https schemes are accepted.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use url::{Host, Url};

use crate::error::{AppError, AppResult};

/// Check whether a URL is safe to request given the current policy.
///
/// Returns `Ok(())` when the request should proceed,
/// `Err(AppError)` with a user-readable reason when it should be blocked.
pub fn check(url: &str, internet_enabled: bool) -> AppResult<()> {
    // Scheme check
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(AppError::msg("Only http:// and https:// URLs are supported."));
    }

    // Parse host
    let host = extract_host(url).ok_or_else(|| AppError::msg("Cannot parse host from URL"))?;

    // Always allow localhost (llama-server)
    if is_localhost(&host) {
        return Ok(());
    }

    // Block private / link-local / multicast ranges
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_blocked_ip(ip) {
            return Err(AppError::msg(format!(
                "Requests to {host} are not allowed. Only localhost and public internet addresses are permitted."
            )));
        }
    }

    // Public internet is only allowed when the toggle is on
    if !internet_enabled {
        return Err(AppError::msg(
            "Internet access is disabled for this project. Enable the internet toggle to fetch external sources.",
        ));
    }

    Ok(())
}

fn extract_host(url: &str) -> Option<String> {
    // Strip scheme
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    // Strip path/query
    let host_port = rest.split('/').next()?;
    // Strip port
    Some(host_port.split(':').next()?.to_string())
}

fn is_localhost(host: &str) -> bool {
    host == "localhost"
        || host == "127.0.0.1"
        || host == "::1"
        || host.ends_with(".localhost")
}

fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            // 10.x.x.x, 172.16-31.x.x, 192.168.x.x, 169.254.x.x, 0.x.x.x
            v4.is_private() || v4.is_link_local() || v4.is_loopback() || v4.is_unspecified()
        }
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
    }
}

fn blocked_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified()
        || ip.is_broadcast() || ip.is_multicast() || ip.is_documentation()
        || o[0] == 0
        || (o[0] == 100 && (o[1] & 0xC0) == 64) // CGNAT 100.64/10
}

fn blocked_v6(ip: Ipv6Addr) -> bool {
    let s = ip.segments();
    ip.is_loopback() || ip.is_unspecified() || ip.is_multicast()
        || (s[0] & 0xfe00) == 0xfc00      // unique local fc00::/7
        || (s[0] & 0xffc0) == 0xfe80      // link-local fe80::/10
        || ip.to_ipv4_mapped().is_some_and(blocked_v4)
}

fn ip_blocked(ip: IpAddr) -> bool {
    match ip { IpAddr::V4(v) => blocked_v4(v), IpAddr::V6(v) => blocked_v6(v) }
}

/// Strict check for *research* fetches: no localhost, no private ranges,
/// no userinfo tricks. Uses a real URL parser (which also normalizes
/// decimal/hex IPs like http://2130706433/ into Ipv4).
pub fn check_external(url: &str, internet_enabled: bool) -> AppResult<Url> {
    let u = Url::parse(url).map_err(|_| AppError::msg("Invalid URL."))?;
    if !matches!(u.scheme(), "http" | "https") {
        return Err(AppError::msg("Only http:// and https:// URLs are supported."));
    }
    if !u.username().is_empty() || u.password().is_some() {
        return Err(AppError::msg("URLs with embedded credentials are not allowed."));
    }
    let blocked = |h: &str| AppError::msg(format!("Requests to {h} are not allowed."));
    match u.host() {
        None => return Err(AppError::msg("Cannot parse host from URL.")),
        Some(Host::Domain(d)) => {
            let d = d.to_ascii_lowercase();
            if d == "localhost" || d.ends_with(".localhost") || d.ends_with(".local") || d.ends_with(".internal") {
                return Err(blocked(&d));
            }
        }
        Some(Host::Ipv4(ip)) if blocked_v4(ip) => return Err(blocked(&ip.to_string())),
        Some(Host::Ipv6(ip)) if blocked_v6(ip) => return Err(blocked(&ip.to_string())),
        _ => {}
    }
    if !internet_enabled {
        return Err(AppError::msg(
            "Internet access is disabled for this project. Enable the internet toggle to fetch external sources.",
        ));
    }
    Ok(u)
}

/// Same as `check_external`, plus rejects hostnames that resolve to blocked IPs.
pub async fn check_external_resolved(url: &str, internet_enabled: bool) -> AppResult<()> {
    let u = check_external(url, internet_enabled)?;
    if let Some(Host::Domain(d)) = u.host() {
        let port = u.port_or_known_default().unwrap_or(443);
        let addrs = tokio::net::lookup_host((d, port))
            .await
            .map_err(|e| AppError::msg(format!("DNS lookup failed for {d}: {e}")))?;
        for a in addrs {
            if ip_blocked(a.ip()) {
                return Err(AppError::msg(format!("{d} resolves to a blocked address.")));
            }
        }
    }
    Ok(())
}