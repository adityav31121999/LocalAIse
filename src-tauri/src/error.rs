//! One error type for everything that crosses the Tauri boundary.
//!
//! Commands return `AppResult<T>`, and the message is what the UI shows, so
//! each variant explains the problem in terms a user can act on.

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Message(String),

    #[error("could not read or write a file: {0}")]
    Io(#[from] std::io::Error),

    #[error("could not read project state: {0}")]
    Json(#[from] serde_json::Error),

    #[error("no local model is selected. Add a GGUF file under the models folder.")]
    NoModelSelected,

    #[error("the llama-server binary was not found. Build llama.cpp or set its path in settings.")]
    ServerBinaryMissing,

    #[error("llama-server did not become ready in time")]
    ServerNotReady,

    #[error("llama-server rejected the request: {0}")]
    ServerRejected(String),

    #[error("path is outside the project folder: {0}")]
    OutsideSandbox(String),

    #[error("unknown project: {0}")]
    UnknownProject(String),

    #[error("this feature needs the desktop app, not a plain browser")]
    DesktopOnly,

    #[error("tauri error: {0}")]
    Tauri(#[from] tauri::Error),
}

impl AppError {
    pub fn msg(message: impl Into<String>) -> Self {
        AppError::Message(message.into())
    }
}

// Tauri serializes command errors; a plain string keeps the UI code simple.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
