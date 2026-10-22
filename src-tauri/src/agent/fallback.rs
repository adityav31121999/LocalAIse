//! Graceful fallback behaviour when the local model is unavailable.
//!
//! When the llama-server is not running (no model installed, process crashed,
//! or the user just launched the app for the first time), any command that
//! needs the model calls into this module so that the frontend always gets a
//! helpful message rather than a raw Rust error.

/// Returns a user-friendly explanation when no model is installed or running.
pub fn no_model_message() -> String {
    "No local model is running yet.\n\n\
     To get started:\n\
     1. Open Settings → Models and install a GGUF model, or\n\
     2. Drop any GGUF file into the models folder shown in the Hardware Check.\n\n\
     The model runs entirely on this machine — no internet connection is required \
     once the file is downloaded."
        .to_string()
}

/// Returns a user-friendly explanation when the server binary is missing.
pub fn no_server_message() -> String {
    "The llama.cpp runtime is not on this machine yet.\n\n\
     The installer ships llama-server; when running from source, build it once \
     with `npm run fetch-llama-server` in the repository folder.\n\n\
     See the README for details."
        .to_string()
}

/// Returns a user-friendly explanation when the server started but is not ready.
pub fn server_not_ready_message(port: u16, detail: &str) -> String {
    format!(
        "The local model server started on port {port} but is not responding yet.\n\n\
         Detail: {detail}\n\n\
         This can happen when:\n\
         • The model file is very large and still loading\n\
         • Another app is already using that port\n\
         • The server crashed before it became ready\n\n\
         Try closing and reopening the project. If the problem persists, check \
         the activity log for more information."
    )
}

/// Returns a concise status line for the status bar when inference is degraded.
pub fn degraded_status(reason: &str) -> String {
    format!("Model unavailable · {reason}")
}
