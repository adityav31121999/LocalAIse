use std::path::Path;

use super::chunk::{chunk_text, Chunk};
use super::parsers::parse_file;

/// Runs full parsing and chunking pipeline on a file.
pub fn ingest_file(root_folder: &Path, file_rel_path: &Path) -> Result<Vec<Chunk>, String> {
    let full_path = root_folder.join(file_rel_path);
    let text = parse_file(&full_path)?;
    let file_name = file_rel_path.to_string_lossy().replace('\\', "/");
    Ok(chunk_text(&file_name, &text, 800, 150))
}
