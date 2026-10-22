use std::path::Path;

/// Basic PDF stream text extraction without native C dependencies.
pub fn parse(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Failed to read PDF: {e}"))?;
    // Scan PDF text objects (between BT and ET, or plain text strings)
    let content = String::from_utf8_lossy(&bytes);
    let mut extracted = String::new();
    let mut in_parentheses = false;
    let mut current_token = String::new();

    for ch in content.chars() {
        if ch == '(' {
            in_parentheses = true;
            current_token.clear();
        } else if ch == ')' && in_parentheses {
            in_parentheses = false;
            let trimmed = current_token.trim();
            if trimmed.len() > 1 && trimmed.chars().any(|c| c.is_alphabetic()) {
                extracted.push_str(trimmed);
                extracted.push(' ');
            }
        } else if in_parentheses {
            current_token.push(ch);
        }
    }

    if extracted.trim().is_empty() {
        // Fallback: extract ASCII printable strings from the binary stream
        let mut word = String::new();
        for b in bytes {
            if b.is_ascii_graphic() || b == b' ' || b == b'\n' {
                word.push(b as char);
            } else {
                if word.len() > 3 && word.chars().any(|c| c.is_alphabetic()) {
                    extracted.push_str(&word);
                    extracted.push(' ');
                }
                word.clear();
            }
        }
    }

    Ok(extracted.trim().to_string())
}
