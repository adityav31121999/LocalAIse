use std::path::Path;

/// Basic XLSX XML text extraction fallback.
pub fn parse(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Failed to read XLSX: {e}"))?;
    let content = String::from_utf8_lossy(&bytes);
    let mut text = String::new();
    let mut in_tag = false;

    for ch in content.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag && (ch.is_alphanumeric() || ch.is_whitespace() || ch.is_ascii_punctuation()) {
            text.push(ch);
        }
    }

    Ok(text.trim().to_string())
}
