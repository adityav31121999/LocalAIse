use std::path::Path;

/// Extract text from PDF using pdf-extract (pure Rust, no native deps).
pub fn parse(path: &Path) -> Result<String, String> {
    let output = pdf_extract::extract_text(path)
        .map_err(|e| format!("pdf-extract failed: {e}"))?;
    Ok(output.trim().to_string())
}