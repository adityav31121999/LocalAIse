//! Web fetch tool: fetch and extract text from a URL.
//! Only works when the project's internet toggle is enabled.

use std::path::Path;

use crate::network::gate;
use crate::tools::ToolResult;

/// Fetch the text content of a URL.
pub async fn fetch(project_root: &Path, url: &str, internet_enabled: bool) -> ToolResult {
    if url.trim().is_empty() {
        return ToolResult::err("web_fetch", "url argument is required");
    }

    if !url.starts_with("http://") && !url.starts_with("https://") {
        return ToolResult::err("web_fetch", "url must start with http:// or https://");
    }

    match gate::fetch_url(project_root, url, internet_enabled, "agent tool").await {
        Ok(content) => {
            let preview: String = content.chars().take(2000).collect();
            ToolResult::ok("web_fetch", format!("Content from {url}:\n\n{preview}"))
        }
        Err(e) => ToolResult::err("web_fetch", e.to_string()),
    }
}
