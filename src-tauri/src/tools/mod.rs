//! Agent-callable tools that the research loop can use.
//!
//! Each tool is a pure function: it takes arguments, does its work, and
//! returns a plain string result the model can read back. The caller is
//! responsible for checking the internet toggle before calling web tools.

pub mod calendar;
pub mod diary;
pub mod files;
pub mod search_local;
pub mod web_fetch;
pub mod web_search;

use std::path::Path;

pub use calendar::*;
pub use diary::*;
pub use files::*;
pub use search_local::*;
pub use web_fetch::*;
pub use web_search::*;

/// The result of any tool call.
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub tool: String,
    pub output: String,
    pub error: Option<String>,
}

impl ToolResult {
    pub fn ok(tool: impl Into<String>, output: impl Into<String>) -> Self {
        Self { tool: tool.into(), output: output.into(), error: None }
    }

    pub fn err(tool: impl Into<String>, error: impl Into<String>) -> Self {
        let msg = error.into();
        Self { tool: tool.into(), output: String::new(), error: Some(msg) }
    }

    /// Returns the effective content for the model: the output, or the error if there was one.
    pub fn content(&self) -> &str {
        if let Some(e) = &self.error { e } else { &self.output }
    }
}

/// Dispatch a named tool call with a JSON-value argument map.
pub async fn call(
    tool_name: &str,
    args: &serde_json::Value,
    project_root: &Path,
    personal_space: &Path,
    internet_enabled: bool,
) -> ToolResult {
    match tool_name {
        "read_file" => {
            let path = args["path"].as_str().unwrap_or("");
            files::read_file(project_root, path)
        }
        "list_files" => {
            let subdir = args["subdir"].as_str().unwrap_or("");
            files::list_files(project_root, subdir)
        }
        "write_file" => {
            let path = args["path"].as_str().unwrap_or("");
            let content = args["content"].as_str().unwrap_or("");
            files::write_file(project_root, path, content)
        }
        "search_local" => {
            let query = args["query"].as_str().unwrap_or("");
            search_local::search_local(project_root, query)
        }
        "search_diary" => {
            let query = args["query"].as_str().unwrap_or("");
            diary::search_diary(personal_space, query)
        }
        "list_events" => calendar::list_events_tool(personal_space),
        "web_fetch" => {
            let url = args["url"].as_str().unwrap_or("");
            web_fetch::fetch(project_root, url, internet_enabled).await
        }
        "web_search" => {
            let query = args["query"].as_str().unwrap_or("");
            web_search::search(project_root, query, internet_enabled).await
        }
        unknown => ToolResult::err(unknown, format!("Unknown tool: {unknown}")),
    }
}
