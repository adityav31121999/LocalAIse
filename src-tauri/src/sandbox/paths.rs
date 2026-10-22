//! Path rules for tools that touch files.
//!
//! Defined once so every caller — the project manager now, agent file tools
//! later — can reuse the same check instead of inventing its own.

use std::path::{Component, Path, PathBuf};

use crate::error::{AppError, AppResult};

/// True when the path tries to climb out of its folder with `..`.
pub fn has_parent_escape(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir))
}

/// Resolves `candidate` against `root` and refuses anything that lands outside
/// it, including symlink targets that point out of the folder.
///
/// Relative candidates are joined onto the canonical root; absolute ones are
/// accepted only when they stay inside the root. Paths that do not exist yet
/// are allowed, since callers often create files; the check is then purely on
/// the path's components. Working per-component also avoids the Windows trap
/// where `canonicalize()` returns an extended length path (`\\?\C:\…`) that a
/// plain joined path never starts with.
pub fn ensure_within(root: &Path, candidate: &Path) -> AppResult<PathBuf> {
    if has_parent_escape(candidate) {
        return Err(AppError::OutsideSandbox(candidate.display().to_string()));
    }

    let canonical_root = root.canonicalize().map_err(|error| {
        AppError::msg(format!("cannot resolve {}: {error}", root.display()))
    })?;

    let resolved: PathBuf = if candidate.is_absolute() {
        // Accept absolute candidates only when they live inside the root,
        // compared component by component. Windows canonicalized roots use a
        // verbatim prefix (`\\?\C:`), so prefixes are compared loosely.
        let root_parts: Vec<Component> = canonical_root.components().collect();
        let cand_parts: Vec<Component> = candidate.components().collect();
        if cand_parts.len() < root_parts.len()
            || root_parts
                .iter()
                .zip(&cand_parts)
                .any(|(root_part, cand_part)| !same_component(root_part, cand_part))
        {
            return Err(AppError::OutsideSandbox(candidate.display().to_string()));
        }
        candidate.to_path_buf()
    } else {
        let mut joined = canonical_root.clone();
        for component in candidate.components() {
            match component {
                Component::Normal(name) => joined.push(name),
                Component::CurDir => {}
                // ParentDir is excluded above; a Prefix component can only
                // appear in absolute paths.
                other => {
                    return Err(AppError::OutsideSandbox(format!("{other:?}")));
                }
            }
        }
        joined
    };

    // Resolve what exists so a symlink inside the folder cannot point
    // outside it.
    if let Ok(real) = resolved.canonicalize() {
        if !real.starts_with(&canonical_root) {
            return Err(AppError::OutsideSandbox(resolved.display().to_string()));
        }
    }

    Ok(resolved)
}

/// Component comparison that treats the Windows `\\?\C:` verbatim prefix as
/// equal to a plain `C:` prefix, and (on Windows only) compares folder names
/// case-insensitively. When the components are not demonstrably the same, the
/// answer is "different", which errs on the side of refusing.
fn same_component(a: &Component, b: &Component) -> bool {
    if a == b {
        return true;
    }
    let strip_verbatim = |part: &Component| {
        part.as_os_str()
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .to_string()
    };
    match (a, b) {
        (Component::Prefix(_), Component::Prefix(_))
        | (Component::RootDir, Component::RootDir) => strip_verbatim(a) == strip_verbatim(b),
        (Component::Normal(x), Component::Normal(y)) if cfg!(windows) => {
            x.eq_ignore_ascii_case(y)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_files_resolve_without_existing_on_disk() {
        // Temp dirs canonicalize to \\?\C:\… on Windows; joined-like checks
        // against that prefix used to fail for files that do not exist yet.
        let base = std::env::temp_dir();
        let root = base.join(format!("LocalAIse-path-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let candidate = Path::new("outputs/new/summary.md");
        let resolved = ensure_within(&root, candidate).expect("in-project new path");
        assert!(resolved.starts_with(root.canonicalize().unwrap()));
        let _ = std::fs::remove_dir_all(&root);
    }
}
