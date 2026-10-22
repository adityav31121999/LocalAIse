//! Web search tool: perform a DuckDuckGo Lite search and return top results.
//! Only works when the project's internet toggle is enabled.

use std::path::Path;

use crate::network::gate;
use crate::tools::ToolResult;

/// Search DuckDuckGo Lite and return the top result titles + snippets.
pub async fn search(project_root: &Path, query: &str, internet_enabled: bool) -> ToolResult {
    if query.trim().is_empty() {
        return ToolResult::err("web_search", "query argument is required");
    }

    // DDG Lite HTML endpoint — no API key, no JavaScript required
    let encoded = encode_query(query);
    let url = format!("https://lite.duckduckgo.com/lite/?q={encoded}");

    match gate::fetch_url(project_root, &url, internet_enabled, &format!("web_search:{query}")).await {
        Ok(text) => {
            // The gate already strips HTML; extract the first ~2000 chars
            let preview: String = text.chars().take(2000).collect();
            ToolResult::ok(
                "web_search",
                format!("Search results for \u{201C}{query}\u{201D}:\n\n{preview}"),
            )
        }
        Err(e) => ToolResult::err("web_search", e.to_string()),
    }
}

fn encode_query(q: &str) -> String {
    q.chars()
        .map(|c| match c {
            ' ' => '+',
            c if c.is_alphanumeric() || c == '-' || c == '_' => c,
            _ => '_',
        })
        .collect()
}
