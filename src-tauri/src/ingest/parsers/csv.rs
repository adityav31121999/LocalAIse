use std::path::Path;

/// Extracts plain text from CSV documents with basic column alignment.
pub fn parse(path: &Path) -> Result<String, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("Failed to read CSV: {e}"))?;
    let mut out = String::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            out.push_str(trimmed);
            out.push('\n');
        }
    }
    Ok(out)
}
