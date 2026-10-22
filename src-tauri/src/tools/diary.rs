//! Diary search tool: search personal diary entries for a query.

use std::path::Path;

use crate::tools::ToolResult;

/// Search diary entries by keyword in the personal space directory.
pub fn search_diary(personal_space: &Path, query: &str) -> ToolResult {
    if query.trim().is_empty() {
        return ToolResult::err("search_diary", "query argument is required");
    }

    let diary_dir = personal_space.join("diary");
    let Ok(read_dir) = std::fs::read_dir(&diary_dir) else {
        return ToolResult::ok("search_diary", "No diary entries found yet.");
    };

    let needle = query.to_lowercase();
    let mut matches: Vec<(String, String)> = Vec::new();

    for entry in read_dir.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "md" {
            continue;
        }

        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };

        if content.to_lowercase().contains(&needle) {
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
            // Return first matching paragraph
            let excerpt = content
                .split("\n\n")
                .find(|para| para.to_lowercase().contains(&needle))
                .unwrap_or(content.lines().next().unwrap_or(""))
                .chars()
                .take(300)
                .collect::<String>();
            matches.push((name, excerpt));
        }

        if matches.len() >= 5 {
            break;
        }
    }

    if matches.is_empty() {
        return ToolResult::ok("search_diary", format!("No diary entries match: {query}"));
    }

    let mut output = format!("Found {} diary entries matching: {query}\n\n", matches.len());
    for (date, excerpt) in &matches {
        output.push_str(&format!("**{date}**\n{excerpt}\n\n"));
    }

    ToolResult::ok("search_diary", output)
}
