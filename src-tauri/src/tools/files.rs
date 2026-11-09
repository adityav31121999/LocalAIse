//! File tools: read, write, and list files inside a project folder.
//!
//! Writes are restricted to the project folder and never escape to the
//! wider filesystem. Paths are normalised so that `../` tricks fail.

use std::path::Path;

use crate::error::{AppError, AppResult};
use crate::tools::ToolResult;
use crate::sandbox::paths::ensure_within;

/// Normalises a model-supplied relative path.
///
/// Chat messages reference files with a leading `@` (for example
/// `@outputs/file.txt`), where the `@` is only the app's mention marker.
/// A small model often copies that marker straight into a tool argument —
/// sometimes even into the middle of one (`outputs/@outputs/file.txt`) — so
/// every `@` that starts a path segment is stripped here. `@` used inside a
/// file name (`user@host.txt`) survives untouched. Quotes and stray
/// whitespace are trimmed for the same reason.
pub fn normalize_rel(raw: &str) -> String {
    let mut path = raw.trim().to_string();
    // Model quotes come and go: "outputs/file.txt".
    for quote in ['"', '\'', '`'] {
        while path.starts_with(quote) && path.ends_with(quote) && path.len() >= 2 {
            path = path[1..path.len() - 1].trim().to_string();
        }
    }
    // Strip a mention marker at the start of every segment. A marker after a
    // separator (`outputs/@outputs/file.txt`) is the same mistake one level in.
    let mut cleaned = String::with_capacity(path.len());
    for part in path.split(['/', '\\']) {
        let mut part = part.trim();
        if part.starts_with('@') {
            part = part[1..].trim();
        }
        if part.is_empty() {
            continue;
        }
        if !cleaned.is_empty() {
            cleaned.push('/');
        }
        cleaned.push_str(part);
    }
    // Drop a redundant "./" at the front.
    cleaned
        .strip_prefix("./")
        .map(str::to_string)
        .unwrap_or(cleaned)
}

/// Read a file relative to the project root. Returns its text content.
pub fn read_file(project_root: &Path, rel_path: &str) -> ToolResult {
    if rel_path.trim().is_empty() {
        return ToolResult::err("read_file", "path argument is required");
    }
    let rel_path = normalize_rel(rel_path);

    let target = match safe_join(project_root, &rel_path) {
        Some(p) => p,
        None => return ToolResult::err("read_file", "path escapes project folder"),
    };

    match std::fs::read_to_string(&target) {
        Ok(content) => ToolResult::ok("read_file", content),
        Err(e) => {
            // Check if it's a binary file
            if let Ok(bytes) = std::fs::read(&target) {
                if bytes.contains(&0) {
                    return ToolResult::err("read_file", format!(
                        "Cannot read {rel_path}: binary file (not text). Use read_file_bytes for binary content."
                    ));
                }
            }
            ToolResult::err("read_file", format!("Cannot read {rel_path}: {e}"))
        }
    }
}

/// Read a file relative to the project root as raw bytes.
/// Used for binary files like PDFs that need visual rendering.
pub fn read_file_bytes(project_root: &Path, rel_path: &str) -> AppResult<Vec<u8>> {
    if rel_path.is_empty() {
        return Err(AppError::msg("path argument is required"));
    }

    let target = ensure_within(project_root, Path::new(rel_path))?;

    match std::fs::read(&target) {
        Ok(bytes) => Ok(bytes),
        Err(e) => Err(AppError::msg(format!("Cannot read {rel_path}: {e}"))),
    }
}

/// List files (non-recursively) in `subdir` inside the project folder.
pub fn list_files(project_root: &Path, subdir: &str) -> ToolResult {
    let subdir = normalize_rel(subdir);
    let dir = if subdir.is_empty() {
        project_root.to_path_buf()
    } else {
        match safe_join(project_root, &subdir) {
            Some(p) => p,
            None => return ToolResult::err("list_files", "path escapes project folder"),
        }
    };

    match std::fs::read_dir(&dir) {
        Ok(entries) => {
            let mut names: Vec<String> = entries
                .flatten()
                .map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        format!("{name}/")
                    } else {
                        name
                    }
                })
                .collect();
            names.sort();
            ToolResult::ok("list_files", names.join("\n"))
        }
        Err(e) => ToolResult::err("list_files", format!("Cannot list {subdir}: {e}")),
    }
}

/// Resolves the destination for a write tool call (`write_file`/`append_file`).
///
/// Agent-created files land in `outputs/` so the user's tree is never silently
/// grown. But a write to a path that already exists in the project goes exactly
/// there: the user named that file, so storing the content under an
/// auto-created `outputs/diary/x.md` would duplicate it in the wrong place.
/// A path like `@outputs/file.txt` (mention marker echoed by the model)
/// normalises to `outputs/file.txt` above and keeps its folder here, instead
/// of growing into the bogus `outputs/@outputs/file.txt`.
fn write_destination(project_root: &Path, rel_path: &str) -> String {
    if rel_path.starts_with("outputs/") || rel_path.starts_with("outputs\\") {
        return rel_path.to_string();
    }
    if project_root.join(std::path::Path::new(rel_path)).is_file() {
        return rel_path.to_string();
    }
    format!("outputs/{rel_path}")
}

/// Write `content` to `rel_path` inside the project folder (new files are
/// created under outputs/, existing files are written in place).
pub fn write_file(project_root: &Path, rel_path: &str, content: &str) -> ToolResult {
    let rel_path = normalize_rel(rel_path);
    if rel_path.is_empty() {
        return ToolResult::err("write_file", "path argument is required");
    }

    let safe_path = write_destination(project_root, &rel_path);

    let target = match safe_join(project_root, &safe_path) {
        Some(p) => p,
        None => return ToolResult::err("write_file", "path escapes project folder"),
    };

    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    match std::fs::write(&target, content) {
        Ok(()) => ToolResult::ok("write_file", format!("Written to {safe_path}")),
        Err(e) => ToolResult::err("write_file", format!("Cannot write {safe_path}: {e}")),
    }
}

/// Append `content` to the end of `rel_path` inside the project folder (new
/// files are created under outputs/, existing files are appended in place).
pub fn append_file(project_root: &Path, rel_path: &str, content: &str) -> ToolResult {
    let rel_path = normalize_rel(rel_path);
    if rel_path.is_empty() {
        return ToolResult::err("append_file", "path argument is required");
    }

    let safe_path = write_destination(project_root, &rel_path);

    let target = match safe_join(project_root, &safe_path) {
        Some(p) => p,
        None => return ToolResult::err("append_file", "path escapes project folder"),
    };

    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    use std::io::Write;
    match std::fs::OpenOptions::new().create(true).append(true).open(&target) {
        Ok(mut file) => match file.write_all(content.as_bytes()) {
            Ok(()) => ToolResult::ok("append_file", format!("Appended to {safe_path}")),
            Err(e) => ToolResult::err("append_file", format!("Cannot append to {safe_path}: {e}")),
        },
        Err(e) => ToolResult::err("append_file", format!("Cannot open {safe_path}: {e}")),
    }
}

/// Joins `rel` to `root`, returning `None` if the result escapes `root`.
fn safe_join(root: &Path, rel: &str) -> Option<std::path::PathBuf> {
    let relative = std::path::Path::new(rel);
    if relative.is_absolute() {
        return None;
    }
    if relative
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return None;
    }
    // Use the root as-is without canonicalizing, to avoid failures on
    // Windows/OneDrive paths where canonicalize() can fail.
    Some(root.to_path_buf().join(relative))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mention_markers_are_stripped_from_paths() {
        assert_eq!(normalize_rel("@outputs/file.txt"), "outputs/file.txt");
        assert_eq!(normalize_rel("@data/notes.md"), "data/notes.md");
        assert_eq!(normalize_rel("file.txt"), "file.txt");
    }

    #[test]
    fn quoted_paths_lose_their_quotes() {
        assert_eq!(normalize_rel("\"outputs/file.txt\""), "outputs/file.txt");
        assert_eq!(normalize_rel("'data/x.md'"), "data/x.md");
    }

    #[test]
    fn relative_prefixes_are_stripped() {
        assert_eq!(normalize_rel("./outputs/file.txt"), "outputs/file.txt");
        assert_eq!(normalize_rel("@ ./outputs/file.txt"), "outputs/file.txt");
    }

    #[test]
    fn writes_to_existing_files_go_to_the_named_path() {
        use std::fs;

        let root = std::env::temp_dir().join("tools_files_tests_existing");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("diary")).unwrap();
        fs::write(root.join("diary/2026-11-07.md"), "# Day\n").unwrap();

        // The file exists, so an append lands there, not in outputs/diary/.
        let result = append_file(&root, "diary/2026-11-07.md", "line\n");
        assert!(result.error.is_none());
        assert_eq!(fs::read_to_string(root.join("diary/2026-11-07.md")).unwrap(), "# Day\nline\n");
        assert!(!root.join("outputs").exists());

        // Same for a full overwrite of an existing file outside outputs/.
        let result = write_file(&root, "diary/2026-11-07.md", "# Rewritten\n");
        assert!(result.error.is_none());
        assert_eq!(fs::read_to_string(root.join("diary/2026-11-07.md")).unwrap(), "# Rewritten\n");
        assert!(!root.join("outputs").exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn writes_to_new_files_still_land_in_outputs() {
        use std::fs;

        let root = std::env::temp_dir().join("tools_files_tests_new");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let result = write_file(&root, "summary.md", "# Summary\n");
        assert!(result.error.is_none());
        assert!(root.join("outputs/summary.md").is_file());
        assert!(!root.join("summary.md").exists());

        let _ = fs::remove_dir_all(&root);
    }
}
