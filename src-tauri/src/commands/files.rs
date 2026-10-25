//! Directory commands backing the project Files view.

use std::path::PathBuf;

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::{AppError, AppResult};
use crate::projects::files;
use crate::projects::DirectoryEntry;
use crate::AppState;

fn project_root(state: &State<'_, AppState>, project_id: &str) -> AppResult<PathBuf> {
    let registry = state.registry()?;
    let record = registry
        .find(project_id)
        .ok_or_else(|| AppError::UnknownProject(project_id.to_string()))?;
    Ok(PathBuf::from(&record.path))
}

#[tauri::command]
pub async fn list_project_directory(
    state: State<'_, AppState>,
    project_id: String,
    path: String,
) -> AppResult<Vec<DirectoryEntry>> {
    let root = project_root(&state, &project_id)?;
    files::list(&root, &path)
}

#[tauri::command]
pub fn create_project_entry(
    state: State<'_, AppState>,
    project_id: String,
    parent: String,
    name: String,
    is_dir: bool,
) -> AppResult<()> {
    let root = project_root(&state, &project_id)?;
    files::create(&root, &parent, &name, is_dir)
}

#[tauri::command]
pub fn rename_project_entry(
    state: State<'_, AppState>,
    project_id: String,
    path: String,
    new_name: String,
) -> AppResult<()> {
    let root = project_root(&state, &project_id)?;
    files::rename(&root, &path, &new_name)
}

#[tauri::command]
pub fn copy_project_entry(
    state: State<'_, AppState>,
    project_id: String,
    path: String,
) -> AppResult<()> {
    let root = project_root(&state, &project_id)?;
    files::copy(&root, &path)
}

/// Picks one or more files from the user's machine and copies their bytes into
/// the open project folder. Returns the names that were imported; an empty list
/// means the picker was cancelled.
#[tauri::command]
pub async fn import_project_files(
    app: AppHandle,
    state: State<'_, AppState>,
    project_id: String,
    parent: String,
) -> AppResult<Vec<String>> {
    let root = project_root(&state, &project_id)?;

    let Some(picked) = app
        .dialog()
        .file()
        .set_title("Add files to this project folder")
        .blocking_pick_files()
    else {
        return Ok(Vec::new());
    };

    let mut imported = Vec::new();
    for file in picked {
        let source = file
            .into_path()
            .map_err(|error| AppError::msg(format!("that file cannot be read: {error}")))?;
        imported.push(files::copy_into(&root, &parent, &source)?);
    }
    Ok(imported)
}

#[tauri::command]
pub fn delete_project_entry(
    state: State<'_, AppState>,
    project_id: String,
    path: String,
) -> AppResult<()> {
    let root = project_root(&state, &project_id)?;
    files::delete(&root, &path)
}
