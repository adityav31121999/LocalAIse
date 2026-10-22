use std::path::Path;

/// Extracts plain text from Markdown documents.
pub fn parse(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("Failed to read markdown file: {e}"))
}
