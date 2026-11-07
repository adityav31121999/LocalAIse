//! Network gate: strict choke point for outbound HTTP requests.
//! If the project's internet toggle is off, all requests are rejected.
//! Audits every request to activity.log.

use std::path::Path;
use std::time::Duration;

use crate::activity::log as activity;
use crate::error::{AppError, AppResult};
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
    let paths = ProjectPaths::new(project_root);

    if let Err(e) = policy::check_external_resolved(url, internet_enabled).await {
        let _ = activity::record(&paths, activity::ActivityKind::Network,
            "web_blocked", &format!("{url} ({purpose}): {e}"));
        return Err(e);
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                return attempt.error("too many redirects");
            }
            match policy::check_external(attempt.url().as_str(), true) {
                Ok(_) => attempt.follow(),
                Err(e) => attempt.error(e.to_string()),
            }
        }))
        .build()
        .map_err(|e| AppError::msg(format!("HTTP client error: {e}")))?;

    let response = client.get(url)
        .header("User-Agent", "localAIse-ResearchAgent/0.1.0")
        .send().await
        .map_err(|e| AppError::msg(format!("Failed to fetch {url}: {e}")))?;

    if !response.status().is_success() {
        return Err(AppError::msg(format!("Web request failed with HTTP {}", response.status())));
    }
    if response.content_length().map_or(false, |n| n > 5 * 1024 * 1024) {
        return Err(AppError::msg("Response too large (>5 MB)."));
    }

    let html = response.text().await
        .map_err(|e| AppError::msg(format!("Failed to read response body: {e}")))?;

    let _ = activity::record(&paths, activity::ActivityKind::Network,
        "web_fetch", &format!("{url} ({purpose})"));
    Ok(html)
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