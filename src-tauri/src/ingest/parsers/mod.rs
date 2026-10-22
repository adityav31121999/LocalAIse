pub mod csv;
pub mod docx;
pub mod markdown;
pub mod pdf;
pub mod text;
pub mod xlsx;

use std::path::Path;

/// Parses supported document formats into readable text.
pub fn parse_file(path: &Path) -> Result<String, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "md" => markdown::parse(path),
        "txt" => text::parse(path),
        "csv" => csv::parse(path),
        "pdf" => pdf::parse(path),
        "docx" => docx::parse(path),
        "xlsx" => xlsx::parse(path),
        other => Err(format!("Unsupported format: .{other}")),
    }
}
