//! Network gate: strict choke point for outbound HTTP requests.
//! If the project's internet toggle is off, all requests are rejected.
//! Audits every request to activity.log.

use std::path::Path;
use std::time::Duration;

use crate::activity::log as activity;
use crate::error::{AppError, AppResult};
use crate::projects::layout::ProjectPaths;

/// Fetches web page content only if `internet_enabled` is true.
pub async fn fetch_url(
    project_root: &Path,
    url: &str,
    internet_enabled: bool,
    purpose: &str,
) -> AppResult<String> {
    if !internet_enabled {
        return Err(AppError::msg(
            "Internet access is disabled for this project. Enable the internet toggle to fetch web sources.",
        ));
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| AppError::msg(format!("HTTP client error: {e}")))?;

    let response = client
        .get(url)
        .header("User-Agent", "localAIse-ResearchAgent/0.1.0")
        .send()
        .await
        .map_err(|e| AppError::msg(format!("Failed to fetch {url}: {e}")))?;

    if !response.status().is_success() {
        return Err(AppError::msg(format!(
            "Web request failed with HTTP {}",
            response.status()
        )));
    }

    let html = response
        .text()
        .await
        .map_err(|e| AppError::msg(format!("Failed to read response body: {e}")))?;

    // Audit the outbound web access in activity.log
    let _ = activity::record(
        &ProjectPaths::new(project_root),
        activity::ActivityKind::Network,
        "web_fetch",
        &format!("{url} ({purpose})"),
    );

    // Strip HTML tags for clean text extraction
    Ok(clean_html(&html))
}

fn clean_html(html: &str) -> String {
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
