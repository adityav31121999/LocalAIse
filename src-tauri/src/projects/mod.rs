pub mod chats;
pub mod files;
pub mod layout;
pub mod manager;
pub mod templates;
pub mod watcher;

use serde::Serialize;

use crate::projects::layout::{ChatMode, ProjectTemplate};

/// What the UI shows on a project card and in the project header.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    /// Absolute path, shown so the user can always see where their files are.
    pub path: String,
    pub template: ProjectTemplate,
    pub source_count: usize,
    pub internet_enabled: bool,
    /// Chat system prompt the user set. `None` means the built-in default.
    pub system_prompt: Option<String>,
    /// The assistant the project's chat uses: plain chat or tool-using agent.
    pub chat_mode: ChatMode,
    /// Whether the project keeps its own `diary/` folder (one page per day).
    pub diary_enabled: bool,
    /// Folders the user added to the project through the app.
    pub custom_folders: Vec<String>,
    pub model: String,
    pub last_opened_at: String,
}

/// One item in the project's Files view. `path` is relative to the project
/// root with `/` separators.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size_bytes: u64,
    pub modified_at: String,
}

/// One file in the project folder, as listed in the sources panel.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFile {
    pub id: String,
    /// Path relative to the project root, with `/` separators.
    pub name: String,
    pub status: SourceStatus,
    pub size_bytes: u64,
    pub modified_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceStatus {
    /// Parsed, chunked and embedded.
    Indexed,
    /// Being processed right now.
    Indexing,
    /// Found on disk, waiting for the ingest pipeline.
    Pending,
    /// Parsing or embedding failed.
    Error,
}
