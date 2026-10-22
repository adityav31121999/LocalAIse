//! Directory browsing and file operations for the project's Files view.
//!
//! The user owns the project folder, so the app exposes an ordinary file
//! manager: list a directory, create files and folders, rename, copy and
//! delete. Every path is resolved through the sandbox helper, so a relative
//! path can never climb out of the project root.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::projects::layout::{ProjectPaths, PROJECT_FILE, RESERVED_DIRS, STATE_DIR};
use crate::projects::DirectoryEntry;
use crate::sandbox::paths::ensure_within;
use crate::util::time;

/// Normalizes a relative path from the UI to `/` separators without leading or
/// trailing slashes. The empty string means the project root.
fn clean_relative(path: &str) -> String {
    path.replace('\\', "/")
        .trim_matches('/')
        .to_string()
}

fn join_relative(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

/// Resolves a relative path and makes sure it stays inside the project.
fn resolve(root: &Path, relative: &str) -> AppResult<PathBuf> {
    let cleaned = clean_relative(relative);
    ensure_within(root, Path::new(&cleaned))
}

/// Rejects the app's own state and the manifest, which the directory view must
/// not move or delete.
fn ensure_not_protected(relative: &str) -> AppResult<()> {
    let cleaned = clean_relative(relative);
    let first = cleaned.split('/').next().unwrap_or("");
    let protected = cleaned.is_empty()
        || first.eq_ignore_ascii_case(STATE_DIR)
        || cleaned.eq_ignore_ascii_case(PROJECT_FILE);
    if protected {
        return Err(AppError::msg(
            "that item is managed by the app and cannot be changed here.",
        ));
    }
    Ok(())
}

/// A folder or file name typed by the user. Separators and the characters
/// Windows refuses are rejected rather than silently rewritten.
fn validate_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::msg("type a name first."));
    }
    if name == "." || name == ".." {
        return Err(AppError::msg("“.” and “..” are not valid names."));
    }
    if name
        .chars()
        .any(|character| matches!(character, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
    {
        return Err(AppError::msg(
            "a name cannot contain / \\ : * ? \" < > or |.",
        ));
    }
    if name.chars().any(|character| character.is_control()) {
        return Err(AppError::msg("a name cannot contain control characters."));
    }
    if name.len() > 255 {
        return Err(AppError::msg("that name is too long."));
    }
    Ok(name.to_string())
}

fn ensure_free(path: &Path) -> AppResult<()> {
    if path.exists() {
        return Err(AppError::msg(format!(
            "“{}” already exists.",
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string())
        )));
    }
    Ok(())
}

/// Updates the manifest's list of custom top-level folders after a change, so
/// `project.json` keeps describing the directory. Best-effort: a missing
/// manifest is fine for older folders.
fn update_custom_folders(root: &Path, mutate: impl FnOnce(&mut Vec<String>)) {
    let paths = ProjectPaths::new(root);
    if let Some(mut file) = paths.read() {
        mutate(&mut file.folders.custom);
        let _ = paths.write(&file);
    }
}

fn is_reserved_at_root(name: &str) -> bool {
    RESERVED_DIRS
        .iter()
        .any(|reserved| name.eq_ignore_ascii_case(reserved))
        || name.eq_ignore_ascii_case(crate::projects::layout::DATA_DIR)
        || name.eq_ignore_ascii_case(STATE_DIR)
}

pub fn list(root: &Path, relative: &str) -> AppResult<Vec<DirectoryEntry>> {
    let dir = resolve(root, relative)?;
    if !dir.is_dir() {
        return Err(AppError::msg("that location is not a folder."));
    }

    let parent = clean_relative(relative);
    let mut entries = Vec::new();

    for item in fs::read_dir(&dir)? {
        let item = item?;
        let name = item.file_name().to_string_lossy().to_string();

        // The app's internal index and activity log stay out of the file view.
        if name.eq_ignore_ascii_case(STATE_DIR) {
            continue;
        }

        let Ok(metadata) = item.metadata() else {
            continue;
        };
        let modified_at = metadata
            .modified()
            .map(time::to_iso)
            .unwrap_or_else(|_| time::now_iso());
        entries.push(DirectoryEntry {
            name: name.clone(),
            path: join_relative(&parent, &name),
            is_dir: metadata.is_dir(),
            size_bytes: if metadata.is_dir() { 0 } else { metadata.len() },
            modified_at,
        });
    }

    // Folders first, then case-insensitive by name.
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

pub fn create(root: &Path, parent: &str, name: &str, is_dir: bool) -> AppResult<()> {
    let parent = clean_relative(parent);
    let name = validate_name(name)?;
    let parent_path = resolve(root, &parent)?;

    if !parent_path.is_dir() {
        return Err(AppError::msg("that location is not a folder."));
    }
    if parent.is_empty() && is_reserved_at_root(&name) {
        return Err(AppError::msg(format!(
            "“{name}” is reserved for the app; pick another name."
        )));
    }

    let target = parent_path.join(&name);
    ensure_free(&target)?;

    if is_dir {
        fs::create_dir(&target)?;
        if parent.is_empty() {
            update_custom_folders(root, |custom| {
                if !custom.iter().any(|item| item.eq_ignore_ascii_case(&name)) {
                    custom.push(name.clone());
                }
            });
        }
    } else {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
    }

    Ok(())
}

pub fn rename(root: &Path, relative: &str, new_name: &str) -> AppResult<()> {
    let relative = clean_relative(relative);
    ensure_not_protected(&relative)?;

    let new_name = validate_name(new_name)?;
    let source = resolve(root, &relative)?;
    if !source.exists() {
        return Err(AppError::msg("that item no longer exists."));
    }

    let parent = source.parent().unwrap_or(root);
    let parent_relative = relative.rsplit_once('/').map(|(head, _)| head).unwrap_or("");

    if parent_relative.is_empty() && is_reserved_at_root(&new_name) {
        return Err(AppError::msg(format!(
            "“{new_name}” is reserved for the app; pick another name."
        )));
    }

    let target = parent.join(&new_name);
    if target != source {
        ensure_free(&target)?;
    }

    let old_name = source
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let was_dir = source.is_dir();

    fs::rename(&source, &target)?;

    if parent_relative.is_empty() && was_dir {
        update_custom_folders(root, |custom| {
            if let Some(slot) = custom
                .iter_mut()
                .find(|item| item.eq_ignore_ascii_case(&old_name))
            {
                *slot = new_name;
            }
        });
    }

    Ok(())
}

/// Copies a file or folder next to itself, using a "copy" suffix that does not
/// collide with an existing item.
pub fn copy(root: &Path, relative: &str) -> AppResult<()> {
    let relative = clean_relative(relative);
    ensure_not_protected(&relative)?;

    let source = resolve(root, &relative)?;
    if !source.exists() {
        return Err(AppError::msg("that item no longer exists."));
    }

    let parent = source.parent().unwrap_or(root);
    let target = unique_copy_path(parent, &source);
    copy_tree(&source, &target)?;

    let parent_relative = relative.rsplit_once('/').map(|(head, _)| head).unwrap_or("");
    if parent_relative.is_empty() && source.is_dir() {
        let name = target
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        update_custom_folders(root, |custom| {
            if !custom.iter().any(|item| item.eq_ignore_ascii_case(&name)) {
                custom.push(name.clone());
            }
        });
    }

    Ok(())
}

pub fn delete(root: &Path, relative: &str) -> AppResult<()> {
    let relative = clean_relative(relative);
    ensure_not_protected(&relative)?;

    let target = resolve(root, &relative)?;
    if !target.exists() {
        return Err(AppError::msg("that item no longer exists."));
    }

    let parent_relative = relative.rsplit_once('/').map(|(head, _)| head).unwrap_or("");
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let was_dir = target.is_dir();

    if was_dir {
        fs::remove_dir_all(&target)?;
    } else {
        fs::remove_file(&target)?;
    }

    if parent_relative.is_empty() && was_dir {
        update_custom_folders(root, |custom| custom.retain(|item| !item.eq_ignore_ascii_case(&name)));
    }

    Ok(())
}

fn unique_copy_path(parent: &Path, source: &Path) -> PathBuf {
    let stem = source
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "item".to_string());
    let extension = source
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();

    let mut index = 1;
    loop {
        let suffix = if index == 1 {
            " copy".to_string()
        } else {
            format!(" copy {index}")
        };
        let candidate = parent.join(format!("{stem}{suffix}{extension}"));
        if !candidate.exists() {
            return candidate;
        }
        index += 1;
    }
}

fn copy_tree(source: &Path, target: &Path) -> AppResult<()> {
    let metadata = fs::symlink_metadata(source)?;

    if metadata.is_dir() {
        fs::create_dir(target)?;
        for item in fs::read_dir(source)? {
            let item = item?;
            copy_tree(&item.path(), &target.join(item.file_name()))?;
        }
    } else {
        fs::copy(source, target)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "LocalAIse-files-{label}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn lists_folders_before_files_and_hides_app_state() {
        let root = temp_root("list");
        fs::create_dir_all(root.join("data")).unwrap();
        fs::create_dir_all(root.join(STATE_DIR)).unwrap();
        fs::write(root.join("notes.md"), "# Notes").unwrap();

        let entries = list(&root, "").unwrap();
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, vec!["data", "notes.md"]);
        assert!(entries[0].is_dir);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn create_rename_copy_and_delete_round_trip() {
        let root = temp_root("ops");
        create(&root, "", "drafts", true).unwrap();
        create(&root, "drafts", "notes.md", false).unwrap();
        fs::write(root.join("drafts").join("notes.md"), "hello").unwrap();

        rename(&root, "drafts/notes.md", "ideas.md").unwrap();
        assert!(root.join("drafts").join("ideas.md").exists());

        copy(&root, "drafts").unwrap();
        assert!(root.join("drafts copy").is_dir());
        assert!(root.join("drafts copy").join("ideas.md").exists());

        delete(&root, "drafts copy").unwrap();
        assert!(!root.join("drafts copy").exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn refuses_to_leave_the_project() {
        let root = temp_root("sandbox");
        create(&root, "", "notes.md", false).unwrap();
        assert!(delete(&root, "../outside.md").is_err());
        assert!(delete(&root, STATE_DIR).is_err());
        assert!(delete(&root, PROJECT_FILE).is_err());
        assert!(create(&root, "", "data", true).is_err());

        let _ = fs::remove_dir_all(&root);
    }
}
