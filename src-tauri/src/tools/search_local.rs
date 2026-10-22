//! Local search tool: keyword search across the project index.

use std::path::Path;

use crate::index::db::IndexDb;
use crate::retrieval::search::search_chunks;
use crate::tools::ToolResult;

/// Search indexed chunks in the project using keyword matching.
pub fn search_local(project_root: &Path, query: &str) -> ToolResult {
    if query.trim().is_empty() {
        return ToolResult::err("search_local", "query argument is required");
    }

    let db = IndexDb::new(project_root);
    let index = db.load();

    if index.chunks.is_empty() {
        return ToolResult::ok(
            "search_local",
            "No files have been indexed yet. Ask the user to run re-index first.",
        );
    }

    let results = search_chunks(&index, query, 5);

    if results.is_empty() {
        return ToolResult::ok("search_local", format!("No matches found for: {query}"));
    }

    let mut output = format!("Found {} relevant chunks for: {query}\n\n", results.len());
    for (i, scored) in results.iter().enumerate() {
        let preview = if scored.chunk.content.len() > 400 {
            format!("{}…", &scored.chunk.content[..400])
        } else {
            scored.chunk.content.clone()
        };
        let page_note = scored.chunk.page.map(|p| format!(" (p.{p})")).unwrap_or_default();
        output.push_str(&format!(
            "[{}] {} {page_note}\n{preview}\n\n",
            i + 1,
            scored.chunk.file_name
        ));
    }

    ToolResult::ok("search_local", output)
}
