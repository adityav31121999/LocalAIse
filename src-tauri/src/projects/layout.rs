//! The layout of a project folder.
//!
//! A project directory is plain folders the user can open in any explorer:
//! `data/` holds the source files, `chats/` holds one JSON file per chat
//! session, `diary/` holds one Markdown page per day, `outputs/` receives the
//! assistant's exports, and everything else is free space for the user's own
//! folders. All app-internal state lives under `.LocalAIse/`, whose
//! `project.json` is the manifest describing the directory's structure.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AppResult;

pub const STATE_DIR: &str = ".LocalAIse";
pub const PROJECT_FILE: &str = "project.json";
pub const OUTPUTS_DIR: &str = "outputs";
pub const CHATS_DIR: &str = "chats";
pub const DATA_DIR: &str = "data";
pub const DIARY_DIR: &str = "diary";
pub const ACTIVITY_FILE: &str = "activity.log";

/// Folder names the app manages. Users cannot create projects folders with
/// these names, and the knowledge base never reads inside them.
pub const RESERVED_DIRS: [&str; 3] = [CHATS_DIR, DIARY_DIR, OUTPUTS_DIR];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(Default)]
pub enum ProjectTemplate {
    Budget,
    ContentWriting,
    Research,
    #[default]
    General,
}


/// The project's folder structure, recorded in `project.json`. The folder
/// names are defaults; only `chats`, `data` and `outputs` may differ from a
/// project that was created before this manifest existed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectFolders {
    /// One JSON file per saved chat session.
    pub chats: String,
    /// The user's source files; the knowledge base reads here first.
    pub data: String,
    /// One Markdown page per date, when the project keeps its own diary.
    pub diary: String,
    /// Files the assistant produces for the user.
    pub outputs: String,
    /// Extra folders the user created from the project's sources panel.
    pub custom: Vec<String>,
}

impl Default for ProjectFolders {
    fn default() -> Self {
        Self {
            chats: CHATS_DIR.to_string(),
            data: DATA_DIR.to_string(),
            diary: DIARY_DIR.to_string(),
            outputs: OUTPUTS_DIR.to_string(),
            custom: Vec::new(),
        }
    }
}

/// Contents of `<project>/project.json` - the manifest that manages the whole
/// project directory.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectFile {
    pub id: String,
    pub name: String,
    pub template: ProjectTemplate,
    /// Per-project internet toggle. Off unless the user turns it on.
    pub internet_enabled: bool,
    /// Per-project chat system prompt. `None` keeps the built-in default.
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// The directory's folder layout (see `ProjectFolders`).
    #[serde(default)]
    pub folders: ProjectFolders,
    /// Whether the project keeps its own `diary/` of one page per day.
    #[serde(default)]
    pub diary_enabled: bool,
    /// Relative folders or single files (with `/` separators) the knowledge
    /// base may read. Empty means: everything outside the app's own folders.
    #[serde(default)]
    pub rag_paths: Vec<String>,
}

impl ProjectFile {
    pub fn new(id: String, name: String, template: ProjectTemplate, internet_enabled: bool) -> Self {
        Self {
            id,
            name,
            template,
            internet_enabled,
            system_prompt: None,
            folders: ProjectFolders::default(),
            diary_enabled: false,
            rag_paths: Vec::new(),
        }
    }
}

pub struct ProjectPaths {
    root: PathBuf,
}

impl ProjectPaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn state_dir(&self) -> PathBuf {
        self.root.join(STATE_DIR)
    }

    /// The project manifest, kept at the project root so the directory is
    /// self-describing: `project.json` plus the folders it lists.
    pub fn project_file(&self) -> PathBuf {
        self.root.join(PROJECT_FILE)
    }

    /// Older projects kept the manifest inside `.LocalAIse/`; still readable.
    fn legacy_project_file(&self) -> PathBuf {
        self.state_dir().join(PROJECT_FILE)
    }

    pub fn activity_file(&self) -> PathBuf {
        self.state_dir().join(ACTIVITY_FILE)
    }

    pub fn outputs_dir(&self) -> PathBuf {
        self.root.join(OUTPUTS_DIR)
    }

    pub fn chats_dir(&self) -> PathBuf {
        self.root.join(CHATS_DIR)
    }

    pub fn data_dir(&self) -> PathBuf {
        self.root.join(DATA_DIR)
    }

    pub fn diary_dir(&self) -> PathBuf {
        self.root.join(DIARY_DIR)
    }

    /// Creates the folder structure the manifest describes, without touching
    /// anything that already exists. New and re-registered projects both call
    /// this, so an older project gains the structure on next open.
    pub fn ensure_structure(&self, folders: &ProjectFolders, diary_enabled: bool) -> AppResult<()> {
        for name in [&folders.chats, &folders.data, &folders.outputs] {
            if !name.is_empty() {
                std::fs::create_dir_all(self.root.join(name))?;
            }
        }
        if diary_enabled && !folders.diary.is_empty() {
            std::fs::create_dir_all(self.root.join(&folders.diary))?;
        }
        Ok(())
    }

    /// Reads the project manifest, falling back to the old `.LocalAIse/`
    /// location for projects created before the manifest moved to the root.
    pub fn read(&self) -> Option<ProjectFile> {
        if let Some(file) = self.read_file(self.project_file()) {
            return Some(file);
        }
        self.read_file(self.legacy_project_file())
    }

    fn read_file(&self, path: PathBuf) -> Option<ProjectFile> {
        let raw = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    pub fn write(&self, file: &ProjectFile) -> AppResult<()> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::write(self.project_file(), serde_json::to_string_pretty(file)?)?;
        Ok(())
    }
}
