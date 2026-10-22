//! Network policy: what kinds of outbound requests are allowed.
//!
//! The policy is intentionally restrictive:
//!   - llama-server on localhost is always allowed.
//!   - All public internet requests require `internet_enabled == true`.
//!   - No requests to private IP ranges other than 127.x.x.x are allowed.
//!   - No SSRF: only http/https schemes are accepted.

use std::net::IpAddr;

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
