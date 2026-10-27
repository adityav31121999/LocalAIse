use std::path::Path;

/// Extract text from PDF using pdf-extract (pure Rust, no native deps).
///
/// pdf-extract can panic on malformed or unusual PDFs (e.g. fonts with
/// unresolvable glyphs). Wrap those calls in `catch_unwind` so a single bad
/// document cannot crash the whole app — callers fall back to visual
/// rendering (`file_bytes_base64`) instead.
fn run_pdf_extract<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce() -> R + std::panic::UnwindSafe,
{
    match std::panic::catch_unwind(f) {
        Ok(result) => Ok(result),
        Err(panic_err) => {
            let message = if let Some(s) = panic_err.downcast_ref::<&str>() {
                format!("pdf-extract panicked: {s}")
            } else if let Some(s) = panic_err.downcast_ref::<String>() {
                format!("pdf-extract panicked: {s}")
            } else {
                "pdf-extract panicked: unknown panic".to_string()
            };
            log::warn!("{message}");
            Err(message)
        }
    }
}

/// Extract text from PDF using pdf-extract (pure Rust, no native deps).
pub fn parse(path: &Path) -> Result<String, String> {
    let output = run_pdf_extract(|| pdf_extract::extract_text(path))?
        .map_err(|e| format!("pdf-extract failed: {e}"))?;
    Ok(output.trim().to_string())
}

/// Extract text from PDF bytes using pdf-extract.
pub fn parse_from_bytes(bytes: &[u8]) -> Result<String, String> {
    let output = run_pdf_extract(|| pdf_extract::extract_text_from_mem(bytes))?
        .map_err(|e| format!("pdf-extract failed: {e}"))?;
    Ok(output.trim().to_string())
}