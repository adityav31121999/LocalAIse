//! Directory commands backing the project Files view.

use std::path::PathBuf;

use tauri::State;

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
pub fn list_project_directory(
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

#[tauri::command]
pub fn delete_project_entry(
    state: State<'_, AppState>,
    project_id: String,
    path: String,
) -> AppResult<()> {
    let root = project_root(&state, &project_id)?;
    files::delete(&root, &path)
}
