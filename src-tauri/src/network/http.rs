//! Typed HTTP helpers for outbound requests that go through the network gate.
//!
//! All network traffic initiated by the agent or research loop MUST use
//! `gate::fetch_url` (or these wrappers) so that the activity log is always
//! populated and the internet toggle is always respected.

use std::time::Duration;

use crate::error::{AppError, AppResult};

/// Build a shared HTTP client for localhost-only requests (llama-server).
/// No TLS, no proxy support — the server always runs on 127.0.0.1.
pub fn localhost_client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| AppError::msg(format!("Failed to build HTTP client: {e}")))
}

/// Build a shared HTTP client for outbound web requests.
/// 15-second timeout prevents the agent from hanging on slow pages.
pub fn web_client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("localAIse-Agent/0.1.0")
        .build()
        .map_err(|e| AppError::msg(format!("Failed to build HTTP client: {e}")))
}

/// Fetch the body of a URL as text, returning a user-readable error on failure.
pub async fn get_text(client: &reqwest::Client, url: &str) -> AppResult<String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::msg(format!("HTTP GET {url} failed: {e}")))?;

    if !response.status().is_success() {
        return Err(AppError::msg(format!(
            "HTTP {} from {url}",
            response.status()
        )));
    }

    response
        .text()
        .await
        .map_err(|e| AppError::msg(format!("Failed to read body from {url}: {e}")))
}
