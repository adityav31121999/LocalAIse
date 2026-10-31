//! Personal space: the diary pages that live outside any project.
//!
//! Diary commands take an optional `projectId`. With one, pages are read from
//! and written to that project's own `diary/` folder; without one they stay in
//! the personal space. Either way: one Markdown page per date.

use std::path::PathBuf;

use tauri::State;

use crate::error::{AppError, AppResult};
use crate::personal::diary::{self, DiaryPage};
use crate::AppState;

/// The folder a diary command works in: a project's root or the personal
/// space. The diary module (`crate::personal::diary`) appends `diary/` itself,
/// so nothing is created here and a project's pages end up at
/// `<project>/diary/` — never `<project>/diary/diary/`.
fn diary_root(state: &AppState, project_id: Option<&str>) -> AppResult<PathBuf> {
    match project_id {
        Some(id) => {
            let registry = state.registry()?;
            let record = registry
                .find(id)
                .ok_or_else(|| AppError::UnknownProject(id.to_string()))?;
            Ok(record.path.clone())
        }
        None => state.personal_space(),
    }
}

#[tauri::command]
pub fn list_diary_pages(
    state: State<'_, AppState>,
    project_id: Option<String>,
) -> AppResult<Vec<DiaryPage>> {
    diary::list(&diary_root(&state, project_id.as_deref())?)
}

#[tauri::command]
pub fn create_diary_page(
    state: State<'_, AppState>,
    project_id: Option<String>,
    date: Option<String>,
    title: String,
) -> AppResult<DiaryPage> {
    diary::create(&diary_root(&state, project_id.as_deref())?, date, title)
}

#[tauri::command]
pub fn save_diary_page(
    state: State<'_, AppState>,
    project_id: Option<String>,
    page: DiaryPage,
) -> AppResult<DiaryPage> {
    diary::save(&diary_root(&state, project_id.as_deref())?, &page)
}

#[tauri::command]
pub fn delete_diary_page(
    state: State<'_, AppState>,
    project_id: Option<String>,
    page_id: String,
) -> AppResult<()> {
    diary::delete(&diary_root(&state, project_id.as_deref())?, &page_id)
}

#[tauri::command]
pub fn list_calendar_events(state: State<'_, AppState>) -> AppResult<Vec<crate::personal::calendar::CalendarEvent>> {
    crate::personal::calendar::list_events(&state.personal_space()?)
}

#[tauri::command]
pub fn create_calendar_event(
    state: State<'_, AppState>,
    event: crate::personal::calendar::CalendarEvent,
) -> AppResult<crate::personal::calendar::CalendarEvent> {
    crate::personal::calendar::create_event(&state.personal_space()?, event)
}

#[tauri::command]
pub fn update_calendar_event(
    state: State<'_, AppState>,
    event: crate::personal::calendar::CalendarEvent,
) -> AppResult<crate::personal::calendar::CalendarEvent> {
    crate::personal::calendar::update_event(&state.personal_space()?, event)
}

#[tauri::command]
pub fn delete_calendar_event(state: State<'_, AppState>, event_id: String) -> AppResult<()> {
    crate::personal::calendar::delete_event(&state.personal_space()?, &event_id)
}

#[tauri::command]
pub fn export_calendar_ics(state: State<'_, AppState>) -> AppResult<String> {
    crate::personal::calendar::export_ics(&state.personal_space()?)
}

#[tauri::command]
pub fn import_calendar_ics(state: State<'_, AppState>, ics_content: String) -> AppResult<usize> {
    crate::personal::calendar::import_ics(&state.personal_space()?, &ics_content)
}

#[tauri::command]
pub fn search_personal(
    state: State<'_, AppState>,
    query: String,
) -> AppResult<Vec<crate::personal::index::PersonalSearchResult>> {
    Ok(crate::personal::index::search_personal_space(&state.personal_space()?, &query))
}
