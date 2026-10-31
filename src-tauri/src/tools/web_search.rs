//! Web search tool: perform a DuckDuckGo Lite search and return top results with URLs.
//! Only works when the project's internet toggle is enabled.

use std::path::Path;

use crate::network::gate;
use crate::tools::ToolResult;

/// Search result with title, URL, and snippet.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Search DuckDuckGo Lite and return parsed results with URLs.
pub async fn search(project_root: &Path, query: &str, internet_enabled: bool) -> ToolResult {
    if query.trim().is_empty() {
        return ToolResult::err("web_search", "query argument is required");
    }

    // DDG Lite HTML endpoint — no API key, no JavaScript required
    let encoded = encode_query(query);
    let url = format!("https://lite.duckduckgo.com/lite/?q={encoded}");

    // Fetch raw HTML to parse links, then also get cleaned text
    match gate::fetch_raw_url(project_root, &url, internet_enabled, &format!("web_search:{query}")).await {
        Ok(html) => {
            let results = parse_ddg_lite(&html);
            if results.is_empty() {
                return ToolResult::ok(
                    "web_search",
                    format!("No search results found for \u{201C}{query}\u{201D}"),
                );
            }
            
            // Format for display
            let display = results
                .iter()
                .take(5)
                .enumerate()
                .map(|(i, r)| format!("{}. [{title}]({url})\n   {snippet}", i + 1, title = r.title, url = r.url, snippet = r.snippet))
                .collect::<Vec<_>>()
                .join("\n\n");
            
            ToolResult::ok("web_search", format!("Search results for \u{201C}{query}\u{201D}:\n\n{display}"))
        }
        Err(e) => ToolResult::err("web_search", e.to_string()),
    }
}

/// Parse DuckDuckGo Lite HTML to extract title, URL, and snippet for each result.
fn parse_ddg_lite(html: &str) -> Vec<SearchResult> {
    let mut results = Vec::new();
    
    // DDG Lite uses a simple table structure:
    // <table> with rows: <tr><td><a href="...">Title</a></td></tr>
    // followed by <tr><td class="...">Snippet</td></tr>
    
    // First, extract all table rows
    let rows: Vec<&str> = html.split("<tr>").collect();
    
    for row in rows {
        // Look for anchor tags with href
        if let Some(href_start) = row.find("href=\"") {
            let href_start = href_start + 6;
            if let Some(href_end) = row[href_start..].find('"') {
                let url = &row[href_start..href_start + href_end];
                
                // Extract title text between > and </a>
                if let Some(title_start) = row.find('>') {
                    if let Some(title_end) = row[title_start + 1..].find("</a>") {
                        let title = &row[title_start + 1..title_start + 1 + title_end];
                        let title = clean_text(title);
                        
                        // Look for snippet in next table cell or same row
                        let snippet = extract_snippet(row).unwrap_or_default();
                        
                        if !title.is_empty() && url.starts_with("http") {
                            results.push(SearchResult {
                                title,
                                url: url.to_string(),
                                snippet,
                            });
                        }
                    }
                }
            }
        }
    }
    
    results
}

fn extract_snippet(row: &str) -> Option<String> {
    // Look for table cell with snippet class or just text after the link
    if let Some(td_start) = row.find("<td") {
        let after_td = &row[td_start + 3..];
        if let Some(gt) = after_td.find('>') {
            let content = &after_td[gt + 1..];
            if let Some(lt) = content.find('<') {
                let snippet = clean_text(&content[..lt]);
                if snippet.len() > 20 {
                    return Some(snippet);
                }
            }
        }
    }
    None
}

fn clean_text(text: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    
    for ch in text.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            result.push(ch);
        }
    }
    
    // Decode common HTML entities
    result = result
        .replace("&#38;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ");
    
    // Collapse whitespace
    let words: Vec<&str> = result.split_whitespace().collect();
    words.join(" ")
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



