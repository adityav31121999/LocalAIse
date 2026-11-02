//! Background update checker with 24h rule and retry schedule.

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::error::AppResult;
use crate::updater::state::UpdaterState;

/// Retry delays in seconds: now, +5m, +15m, +60m, then stop until next launch.
const RETRY_DELAYS: &[u64] = &[0, 5 * 60, 15 * 60, 60 * 60];

/// Spawn the background update checker.
/// Runs on a tokio task, checks due status, retries on failure.
pub fn spawn_checker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Initial delay: small random offset to avoid thundering herd on app launch
        let initial_delay = Duration::from_millis(
            fastrand::u64(1000..5000)
        );
        tokio::time::sleep(initial_delay).await;

        let mut attempt = 0;
        loop {
            // Re-read settings before each attempt (toggle may have changed)
            let auto_update_check = {
                let state = match app.try_state::<crate::AppState>() {
                    Some(s) => s,
                    None => break,
                };
                let registry = match state.registry.lock() {
                    Ok(r) => r,
                    Err(_) => break,
                };
                registry.settings.auto_update_check
            };

            if !auto_update_check {
                // Toggle turned off; stop checking until next launch
                break;
            }

            // Check if due
            let due = {
                let updater_state = match app.try_state::<UpdaterState>() {
                    Some(s) => s,
                    None => break,
                };
                let guard = updater_state.lock().unwrap();
                guard.is_due(true)
            };

            if !due {
                break;
            }

            // Perform check
            let result = check_for_update(&app).await;

            match result {
                Ok(Some(update)) => {
                    // Update available - store info and emit event
                    let version = update.version.clone();
                    let body = update.body.clone().unwrap_or_default();
                    let date = update.date.map(|d| d.to_string()).unwrap_or_default();

                    // Record successful check
                    if let Some(updater_state) = app.try_state::<UpdaterState>() {
                        let mut guard = updater_state.lock().unwrap();
                        let _ = guard.record_success(&app);
                    }

                    // Emit event for frontend
                    app.emit("update-available", UpdateInfo {
                        version,
                        body,
                        date,
                    }).ok();

                    break; // Stop retrying; user will decide
                }
                Ok(None) => {
                    // No update available - record success and stop
                    if let Some(updater_state) = app.try_state::<UpdaterState>() {
                        let mut guard = updater_state.lock().unwrap();
                        let _ = guard.record_success(&app);
                    }
                    break;
                }
                Err(e) => {
                    // Network error, rate limit, etc. - don't record success, retry
                    log::debug!("Update check failed (attempt {}): {}", attempt + 1, e);
                    attempt += 1;
                    if attempt >= RETRY_DELAYS.len() {
                        break; // Exhausted retries
                    }
                    tokio::time::sleep(Duration::from_secs(RETRY_DELAYS[attempt])).await;
                }
            }
        }
    });
}

/// Check for update using tauri-plugin-updater.
/// Returns Ok(Some) if update available, Ok(None) if up to date, Err on failure.
async fn check_for_update(app: &AppHandle) -> AppResult<Option<tauri_plugin_updater::Update>> {
    let updater = app.updater()?;
    let update = updater.check().await?;
    Ok(update)
}

/// Information about an available update, sent to frontend.
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub body: String,
    pub date: String,
}