//! File tools: read, write, and list files inside a project folder.
//!
//! Writes are restricted to the project folder and never escape to the
//! wider filesystem. Paths are normalised so that `../` tricks fail.

use std::path::Path;

use crate::tools::ToolResult;

/// Read a file relative to the project root. Returns its text content.
pub fn read_file(project_root: &Path, rel_path: &str) -> ToolResult {
    if rel_path.is_empty() {
        return ToolResult::err("read_file", "path argument is required");
    }

    let target = match safe_join(project_root, rel_path) {
        Some(p) => p,
        None => return ToolResult::err("read_file", "path escapes project folder"),
    };

    match std::fs::read_to_string(&target) {
        Ok(content) => ToolResult::ok("read_file", content),
        Err(e) => ToolResult::err("read_file", format!("Cannot read {rel_path}: {e}")),
    }
}

/// List files (non-recursively) in `subdir` inside the project folder.
pub fn list_files(project_root: &Path, subdir: &str) -> ToolResult {
    let dir = if subdir.is_empty() {
        project_root.to_path_buf()
    } else {
        match safe_join(project_root, subdir) {
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

/// Write `content` to `rel_path` inside the project folder (outputs/ prefix enforced).
pub fn write_file(project_root: &Path, rel_path: &str, content: &str) -> ToolResult {
    if rel_path.is_empty() {
        return ToolResult::err("write_file", "path argument is required");
    }

    // Agent writes go to outputs/ so user files are never silently overwritten.
    let safe_path = if rel_path.starts_with("outputs/") || rel_path.starts_with("outputs\\") {
        rel_path.to_string()
    } else {
        format!("outputs/{rel_path}")
    };

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
    // Canonical, component-validated join: without canonicalizing the result
    // we stay correct for files that do not exist yet, including on Windows
    // where canonicalize() rewrites drive paths to \\?\C:\….
    let canonical_root = root.canonicalize().ok()?;
    Some(canonical_root.join(relative))
}
