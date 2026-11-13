//! Network gate: strict choke point for outbound HTTP requests.
//! If the project's internet toggle is off, all requests are rejected.
//! Audits every request to activity.log.

use std::path::Path;

use crate::activity::log as activity;
use crate::error::{AppError, AppResult};
use crate::network::http_client::{HttpClient, HttpClientConfig, RealHttpClient};
use crate::network::policy;
use crate::projects::layout::ProjectPaths;

/// Fetches web page content only if `internet_enabled` is true.
pub async fn fetch_url(
    project_root: &Path,
    url: &str,
    internet_enabled: bool,
    purpose: &str,
) -> AppResult<String> {
    fetch_raw_url(project_root, url, internet_enabled, purpose)
        .await
        .map(|html| clean_html(&html))
}

/// Fetches raw HTML without stripping tags (for parsing).
pub async fn fetch_raw_url(
    project_root: &Path, url: &str, internet_enabled: bool, purpose: &str,
) -> AppResult<String> {
    fetch_raw_url_with_client(project_root, url, internet_enabled, purpose, &default_client()?).await
}

/// Fetches raw HTML using a provided HTTP client (for testing with fixtures).
pub async fn fetch_raw_url_with_client(
    project_root: &Path,
    url: &str,
    internet_enabled: bool,
    purpose: &str,
    client: &dyn HttpClient,
) -> AppResult<String> {
    let paths = ProjectPaths::new(project_root);

    if let Err(e) = policy::check_external_resolved(url, internet_enabled).await {
        let _ = activity::record(&paths, activity::ActivityKind::Network,
            "web_blocked", &format!("{url} ({purpose}): {e}"));
        return Err(e.into());
    }

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("User-Agent", "localAIse-ResearchAgent/0.1.0".parse().unwrap());

    let response = client.get_with_redirect_policy(
        url,
        headers,
        5,
        Box::new(move |redirect_url: String| {
            policy::check_external(&redirect_url, true).map(|_| ()).map_err(|e| e.to_string())
        }),
    ).await?;

    if !is_success_status(response.status) {
        return Err(AppError::msg(format!("Web request failed with HTTP {}", response.status)));
    }

    if response.body.len() as u64 > 5 * 1024 * 1024 {
        return Err(AppError::msg("Response too large (>5 MB)."));
    }

    let _ = activity::record(&paths, activity::ActivityKind::Network,
        "web_fetch", &format!("{url} ({purpose})"));
    Ok(response.body)
}

/// Fetches web page content using a provided HTTP client.
pub async fn fetch_url_with_client(
    project_root: &Path,
    url: &str,
    internet_enabled: bool,
    purpose: &str,
    client: &dyn HttpClient,
) -> AppResult<String> {
    fetch_raw_url_with_client(project_root, url, internet_enabled, purpose, client)
        .await
        .map(|html| clean_html(&html))
}

fn is_success_status(status: u16) -> bool {
    (200..300).contains(&status)
}

fn default_client() -> AppResult<RealHttpClient> {
    RealHttpClient::new(HttpClientConfig::default())
}

/// Audit hook for the chromium path, which can't go through reqwest.
pub fn audit_browser(project_root: &Path, url: &str, purpose: &str) {
    let _ = activity::record(&ProjectPaths::new(project_root),
        activity::ActivityKind::Network, "web_fetch_browser", &format!("{url} ({purpose})"));
}

pub fn clean_html(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;

    // Fast text extraction
    for ch in html.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag && (ch.is_alphanumeric() || ch.is_whitespace() || ch.is_ascii_punctuation()) {
            text.push(ch);
        }
    }

    // Collapse multiple whitespaces
    let mut words = Vec::new();
    for word in text.split_whitespace() {
        if !word.is_empty() {
            words.push(word);
        }
    }
    words.join(" ")
}