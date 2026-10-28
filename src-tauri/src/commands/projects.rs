//! Project and source commands. These back the work-directory list, the sources
//! panel, and the two buttons that add a folder to the app.

use std::path::PathBuf;

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::config::app_config::Registry;
use crate::error::{AppError, AppResult};
use crate::projects::chats::{self, ChatSession, ChatSummary};
use crate::projects::layout::{ChatMode, ProjectPaths, ProjectTemplate};
use crate::projects::manager::{self, Context};
use crate::projects::{Project, SourceFile};
use crate::AppState;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;

#[tauri::command]
pub fn list_projects(state: State<'_, AppState>) -> AppResult<Vec<Project>> {
    let registry = state.registry()?;
    Ok(manager::list(&Context {
        registry: &registry,
        dirs: &state.dirs,
    }))
}

/// Reads a document file and extracts its text content.
/// For PDFs, also returns base64-encoded bytes for visual rendering.
/// If `extract_text` is false (default), only returns bytes for PDFs without text extraction.
#[tauri::command]
pub fn read_document(
    state: State<'_, AppState>,
    project_id: String,
    file_path: String,
    extract_text: Option<bool>,
) -> AppResult<DocumentContent> {
    let registry = state.registry()?;
    let record = registry
        .find(&project_id)
        .ok_or_else(|| AppError::UnknownProject(project_id.clone()))?;
    let root = PathBuf::from(&record.path);
    
    // Resolve path within project
    let target = crate::sandbox::paths::ensure_within(&root, std::path::Path::new(&file_path))?;
    
    if !target.exists() {
        return Err(AppError::msg(format!("File not found: {file_path}")));
    }
    
    let ext = target.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    
    let file_name = target.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&file_path)
        .to_string();
    
    let mime_type = match ext.as_str() {
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "doc" => "application/msword",
        "csv" => "text/csv",
        "json" => "application/json",
        _ => "application/octet-stream",
    };
    
    // Extract text content only if requested (default false for PDFs to avoid unsolicited parsing)
    let should_extract_text = extract_text.unwrap_or(false) || ext != "pdf";
    let text = if should_extract_text {
        if ext == "pdf" {
            crate::ingest::parsers::pdf::parse(&target)
                .map_err(|e| AppError::msg(format!("Failed to parse PDF: {e}")))?
        } else {
            std::fs::read_to_string(&target)
                .map_err(|e| AppError::msg(format!("Cannot read {file_path}: {e}")))?
        }
    } else {
        // For PDFs without text extraction, return empty string
        String::new()
    };
    
    let metadata = std::fs::metadata(&target)
        .map_err(|e| AppError::msg(format!("Cannot read metadata: {e}")))?;
    let size_bytes = metadata.len();
    
    // For PDFs: one read serves both the base64 payload for the viewer and
    // the page count below.
    let pdf_bytes = if ext == "pdf" {
        Some(
            std::fs::read(&target)
                .map_err(|e| AppError::msg(format!("Cannot read PDF bytes: {e}")))?,
        )
    } else {
        None
    };
    let file_bytes_base64 = pdf_bytes
        .as_deref()
        .map(|bytes| BASE64_STANDARD.encode(bytes));

    // Page count, best effort: uncompressed PDFs (typical LaTeX output) write
    // one plain `/Type /Page` object per page (the tree summary is
    // `/Type /Pages`, which prefixes the same string, so subtract it).
    // pdf-extract produces no usable page markers — counting form feeds
    // always yielded 1 here. Object-stream/compressed PDFs hide the markers,
    // and the viewer's pdf.js reports the true count anyway, so report
    // nothing rather than guess.
    let page_count = pdf_bytes.as_deref().and_then(|bytes| {
        let raw = String::from_utf8_lossy(bytes);
        let page_objects = raw.matches("/Type /Page").count();
        let pages_nodes = raw.matches("/Type /Pages").count();
        let count = page_objects.saturating_sub(pages_nodes);
        (count > 0).then_some(count)
    });
    
    Ok(DocumentContent {
        file_name,
        file_path,
        mime_type: mime_type.to_string(),
        text,
        page_count,
        size_bytes,
        file_bytes_base64,
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentContent {
    file_name: String,
    file_path: String,
    mime_type: String,
    text: String,
    page_count: Option<usize>,
    size_bytes: u64,
    file_bytes_base64: Option<String>,
}

#[tauri::command]
pub fn open_project(state: State<'_, AppState>, project_id: String) -> AppResult<Project> {
    let mut registry = state.registry()?;
    manager::open(&mut registry, &state.dirs, &project_id)
}

#[tauri::command]
pub async fn list_sources(state: State<'_, AppState>, project_id: String) -> AppResult<Vec<SourceFile>> {
    let registry = state.registry()?;
    manager::sources(&registry, &project_id)
}

#[tauri::command]
pub async fn reindex_sources(state: State<'_, AppState>, project_id: String) -> AppResult<Vec<SourceFile>> {
    let registry = state.registry()?;
    let record = registry
        .find(&project_id)
        .ok_or_else(|| AppError::UnknownProject(project_id.clone()))?;

    let root = PathBuf::from(&record.path);
    let sources = manager::scan_sources(&record.id, &root)?;
    let index_db = crate::index::db::IndexDb::new(&root);

    for source in &sources {
        let rel_path = std::path::Path::new(&source.name);
        if let Ok(chunks) = crate::ingest::pipeline::ingest_file(&root, rel_path) {
            let _ = index_db.insert_chunks(&source.name, &source.modified_at, chunks);
        }
    }

    manager::scan_sources(&record.id, &root)
}

/// Unregisters a project. When `delete_files` is set the folder itself is
/// removed from disk; otherwise every file stays where it was.
#[tauri::command]
pub fn remove_project(
    state: State<'_, AppState>,
    project_id: String,
    delete_files: bool,
) -> AppResult<()> {
    let mut registry = state.registry()?;
    manager::remove(&mut registry, &state.dirs, &project_id, delete_files)
}

/// Changes the project's display name without moving its folder.
#[tauri::command]
pub fn rename_project(
    state: State<'_, AppState>,
    project_id: String,
    name: String,
) -> AppResult<Project> {
    let mut registry = state.registry()?;
    manager::rename(&mut registry, &state.dirs, &project_id, &name)
}

#[tauri::command]
pub fn set_project_internet(
    state: State<'_, AppState>,
    project_id: String,
    enabled: bool,
) -> AppResult<Project> {
    let mut registry = state.registry()?;
    manager::set_internet(&mut registry, &state.dirs, &project_id, enabled)
}

#[tauri::command]
pub fn set_project_system_prompt(
    state: State<'_, AppState>,
    project_id: String,
    prompt: Option<String>,
) -> AppResult<Project> {
    let mut registry = state.registry()?;
    manager::set_system_prompt(&mut registry, &state.dirs, &project_id, prompt)
}

/// Switches the chat between the plain assistant and the tool-using agent.
#[tauri::command]
pub fn set_project_chat_mode(
    state: State<'_, AppState>,
    project_id: String,
    mode: ChatMode,
) -> AppResult<Project> {
    let mut registry = state.registry()?;
    manager::set_chat_mode(&mut registry, &state.dirs, &project_id, mode)
}

/// Turns the project's own `diary/` folder on or off. Turning it off never
/// deletes a page.
#[tauri::command]
pub fn set_project_diary(
    state: State<'_, AppState>,
    project_id: String,
    enabled: bool,
) -> AppResult<Project> {
    let mut registry = state.registry()?;
    manager::set_diary(&mut registry, &state.dirs, &project_id, enabled)
}

/// Creates a user folder inside the project and records it in `project.json`.
#[tauri::command]
pub fn create_project_folder(
    state: State<'_, AppState>,
    project_id: String,
    name: String,
) -> AppResult<Project> {
    let registry = state.registry()?;
    manager::add_subfolder(&registry, &state.dirs, &project_id, &name)
}

/// Saved chat sessions for a project, newest first.
#[tauri::command]
pub fn list_project_chats(
    state: State<'_, AppState>,
    project_id: String,
) -> AppResult<Vec<ChatSummary>> {
    let registry = state.registry()?;
    let dir = chat_dir(&registry, &project_id)?;
    Ok(chats::list(&dir))
}

/// Reopens one saved conversation.
#[tauri::command]
pub fn read_project_chat(
    state: State<'_, AppState>,
    project_id: String,
    chat_id: String,
) -> AppResult<ChatSession> {
    let registry = state.registry()?;
    let dir = chat_dir(&registry, &project_id)?;
    chats::load(&dir, &chat_id)
}

#[tauri::command]
pub fn delete_project_chat(
    state: State<'_, AppState>,
    project_id: String,
    chat_id: String,
) -> AppResult<()> {
    let registry = state.registry()?;
    let dir = chat_dir(&registry, &project_id)?;
    chats::delete(&dir, &chat_id)
}

/// `<project>/chats`, resolved from the registry entry.
fn chat_dir(registry: &Registry, project_id: &str) -> AppResult<PathBuf> {
    let record = registry
        .find(project_id)
        .ok_or_else(|| AppError::UnknownProject(project_id.to_string()))?;
    Ok(ProjectPaths::new(&record.path).chats_dir())
}

/// Registers a folder the user already has. Returns `None` when they cancel.
#[tauri::command]
pub async fn add_project_folder(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<Option<Project>> {
    let Some(folder) = pick_folder(&app, "Choose a folder to work in")? else {
        return Ok(None);
    };

    let mut registry = state.registry()?;
    manager::add_folder(&mut registry, &state.dirs, &folder).map(Some)
}

/// Creates a new folder for the chosen template. Returns `None` when the user
/// cancels the picker that chooses where it lives.
#[tauri::command]
pub async fn create_project(
    app: AppHandle,
    state: State<'_, AppState>,
    template: ProjectTemplate,
) -> AppResult<Option<Project>> {
    let Some(parent) = pick_folder(&app, "Where should the new project live?")? else {
        return Ok(None);
    };

    let mut registry = state.registry()?;
    manager::create_folder(&mut registry, &state.dirs, &parent, template).map(Some)
}

/// Opens the native folder picker. Only ever called from a worker thread, which
/// is why these two commands are `async`: the dialog must not block the window.
fn pick_folder(app: &AppHandle, title: &str) -> AppResult<Option<PathBuf>> {
    let Some(picked) = app.dialog().file().set_title(title).blocking_pick_folder() else {
        return Ok(None);
    };

    picked
        .into_path()
        .map(Some)
        .map_err(|error| AppError::msg(format!("that location cannot be read: {error}")))
}
