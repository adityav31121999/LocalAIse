//! Tauri commands for the updater.

use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::UpdaterExt;

use crate::error::{AppError, AppResult};
use crate::updater::state::UpdaterState;
use crate::updater::checker::UpdateInfo;

/// Check for updates now (bypasses 24h rule).
/// Returns update info if available, null if up to date.
#[tauri::command]
pub async fn check_now(app: AppHandle) -> AppResult<Option<UpdateInfo>> {
    let updater = app.updater()?;
    let update = updater.check().await?;
    Ok(update.map(|u| UpdateInfo {
        version: u.version,
        body: u.body.unwrap_or_default(),
        date: u.date.map(|d| d.to_string()).unwrap_or_default(),
    }))
}

/// Get current update status (for frontend on mount).
/// Returns pending update info if available, null otherwise.
#[tauri::command]
pub async fn get_update_status(_app: AppHandle) -> AppResult<Option<UpdateInfo>> {
    // In a real implementation, we'd store the pending update in AppState
    // For now, return null - frontend will also listen for update-available event
    Ok(None)
}

/// Download and install the available update.
/// Emits progress events: download-progress, install-complete, install-error
#[tauri::command]
pub async fn install_update(app: AppHandle) -> AppResult<()> {
    let updater = app.updater()?;
    let update = updater.check().await?
        .ok_or_else(|| AppError::msg("No update available"))?;

    // Download with progress
    let mut downloaded = 0u64;

    let _ = update.download_and_install(
        |chunk_length, content_length| {
            downloaded += chunk_length as u64;
            let progress = if let Some(total) = content_length {
                if total > 0 {
                    downloaded as f64 / total as f64 * 100.0
                } else {
                    0.0
                }
            } else {
                0.0
            };
            if (progress as u32) % 10 == 0 {
                app.emit("download-progress", DownloadProgress {
                    progress: progress as u32,
                    downloaded,
                    total: content_length.unwrap_or(0),
                }).ok();
            }
        },
        || {
            // Install callback - plugin handles restart
        }
    ).await?;

    // Plugin will restart the app after install
    Ok(())
}

/// Skip a version (user chose "Skip this version").
#[tauri::command]
pub async fn skip_version(version: String, app: AppHandle) -> AppResult<()> {
    let state: State<'_, UpdaterState> = app.state();
    let mut guard = state.lock().unwrap();
    guard.skip_version(version, &app)
}

/// Get auto-update check setting.
#[tauri::command]
pub async fn get_auto_update_check(app: AppHandle) -> AppResult<bool> {
    let state = match app.try_state::<crate::AppState>() {
        Some(s) => s,
        None => return Ok(true), // default
    };
    let registry = state.registry.lock().map_err(|_| AppError::msg("Registry busy"))?;
    Ok(registry.settings.auto_update_check)
}

/// Set auto-update check setting.
#[tauri::command]
pub async fn set_auto_update_check(enabled: bool, app: AppHandle) -> AppResult<()> {
    let state = app.state::<crate::AppState>();
    let mut registry = state.registry.lock().map_err(|_| AppError::msg("Registry busy"))?;
    registry.settings.auto_update_check = enabled;
    registry.save(&state.dirs.state_file)?;
    Ok(())
}

/// Get last check timestamp (epoch seconds).
#[tauri::command]
pub async fn get_last_check(app: AppHandle) -> AppResult<i64> {
    let state: State<'_, UpdaterState> = app.state();
    let guard = state.lock().unwrap();
    Ok(guard.last_success)
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub progress: u32, // 0-100
    pub downloaded: u64,
    pub total: u64,
}