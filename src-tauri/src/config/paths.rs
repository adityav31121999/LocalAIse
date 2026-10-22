//! Platform directories.
//!
//! App state (registry, downloaded models, llama binaries) lives under the
//! platform app-data dir. Everything the user owns — project folders and the
//! personal space — lives in folders they pick, with sensible defaults under
//! their Documents folder.

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug)]
pub struct AppDirs {
    /// App state root. Deleting it forgets projects without touching user files.
    pub base: PathBuf,
    /// `base/state.json` — the project registry and global settings.
    pub state_file: PathBuf,
    /// Where downloaded GGUF files are kept.
    pub models: PathBuf,
    /// Where llama-server builds are kept.
    pub llama: PathBuf,
    /// Default parent for projects the app creates.
    pub default_projects: PathBuf,
    /// Default personal space folder for diary and calendar.
    pub default_personal: PathBuf,
}

impl AppDirs {
    pub fn resolve(app: &AppHandle) -> AppResult<Self> {
        let base = app
            .path()
            .app_data_dir()
            .map_err(|error| AppError::msg(format!("cannot resolve the app data folder: {error}")))?
            .join("LocalAIse");

        let documents = app.path().document_dir().ok();
        let home = app.path().home_dir().ok();
        let fallback = || documents.clone().or_else(|| home.clone()).unwrap_or_else(|| base.clone());

        let dirs = AppDirs {
            state_file: base.join("state.json"),
            models: base.join("models"),
            llama: base.join("llama"),
            default_projects: fallback().join("localAIse"),
            default_personal: fallback().join("localAIse-Personal"),
            base,
        };

        for path in [&dirs.base, &dirs.models, &dirs.llama] {
            std::fs::create_dir_all(path)?;
        }

        Ok(dirs)
    }
}
