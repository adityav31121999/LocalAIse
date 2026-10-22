//! The activity panel: what the app did inside one project folder.

use tauri::State;

use crate::activity::log as activity;
use crate::error::{AppError, AppResult};
use crate::projects::layout::ProjectPaths;
use crate::AppState;

#[tauri::command]
pub fn list_activity(
    state: State<'_, AppState>,
    project_id: String,
) -> AppResult<Vec<activity::ActivityEvent>> {
    let registry = state.registry()?;
    let record = registry
        .find(&project_id)
        .ok_or_else(|| AppError::UnknownProject(project_id.clone()))?;

    activity::list(&ProjectPaths::new(record.path.clone()))
}
