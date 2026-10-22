//! Network gate: when a project's internet toggle is off, no outbound web
//! request may leave the app. The policy checks are pure, so these run
//! without any sockets.

use localaise_lib::network::policy;

#[test]
fn localhost_is_always_allowed() {
    // The inference server lives here; the toggle never blocks it.
    assert!(policy::check("http://127.0.0.1:8123/health", false).is_ok());
    assert!(policy::check("http://localhost:8123/v1/chat/completions", false).is_ok());
}

#[test]
fn toggle_off_blocks_every_public_request() {
    let blocked = policy::check("https://example.com/research-paper", false);
    assert!(blocked.is_err());
    assert!(blocked.err().unwrap().to_string().contains("disabled"));
}

#[test]
fn toggle_on_allows_public_internet() {
    assert!(policy::check("https://example.com/research-paper", true).is_ok());
}

#[test]
fn only_http_and_https_schemes_pass() {
    for url in [
        "file:///etc/passwd",
        "ftp://files.example.com/report.pdf",
        "data:text/html,hello",
        "javascript:alert(1)",
        "",
    ] {
        assert!(policy::check(url, true).is_err(), "scheme {url:?}");
    }
}

#[test]
fn private_ranges_are_never_reachable() {
    // SSRF shields: a user with the toggle on still must not reach the
    // machine's own network equipment or co-hosted services by IP.
    for host in [
        "http://10.0.0.5/db",
        "http://192.168.1.1/router-config",
        "http://169.254.169.254/metadata",
        "http://172.16.0.9/internal",
    ] {
        assert!(policy::check(host, true).is_err(), "{host} must be blocked");
    }
}

#[test]
fn only_the_actual_local_server_address_passes() {
    // The exact llama-server host is whitelisted; a different loopback IP is
    // not, so tunneling through other 127.x addresses is refused.
    assert!(policy::check("http://127.0.0.2:9000/sneaky", true).is_err());
    assert!(policy::check("http://127.0.0.1:8123/v1/models", true).is_ok());
}

#[test]
fn malformed_urls_are_refused() {
    assert!(policy::check("https://", false).is_err());
    assert!(policy::check("https:///path/no/host", false).is_err());
}
