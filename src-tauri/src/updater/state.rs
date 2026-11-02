//! Atomic updater state persistence: last_success (epoch seconds) and skipped_version.

use std::fs;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::config::paths::AppDirs;
use crate::error::AppResult;

const STATE_FILE: &str = "updater-state.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdaterStateInner {
    /// Unix timestamp (seconds) of the last successful update check.
    /// 0 means never checked.
    pub last_success: i64,
    /// Version string the user chose to skip (e.g., "0.1.2").
    /// None means no version is skipped.
    pub skipped_version: Option<String>,
}

/// Thread-safe wrapper for Tauri state management.
pub type UpdaterState = Mutex<UpdaterStateInner>;

impl UpdaterStateInner {
    /// Load state from disk. Missing or corrupt file yields defaults.
    pub fn load(app: &AppHandle) -> Self {
        let dirs = match AppDirs::resolve(app) {
            Ok(d) => d,
            Err(_) => return Self::default(),
        };
        let path = dirs.base.join(STATE_FILE);
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Persist state atomically: write to temp file, then rename.
    pub fn save(&self, app: &AppHandle) -> AppResult<()> {
        let dirs = AppDirs::resolve(app)?;
        let path = dirs.base.join(STATE_FILE);
        let tmp = path.with_extension("tmp");
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&tmp, json)?;
        fs::rename(tmp, path)?;
        Ok(())
    }

    /// Current time as epoch seconds.
    fn now() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    /// Check if an update check is due (24 hours since last success).
    /// Treats future timestamps as "due" (clock change).
    pub fn is_due(&self, auto_update_check: bool) -> bool {
        if !auto_update_check {
            return false;
        }
        let now = Self::now();
        if self.last_success == 0 {
            return true;
        }
        if now < self.last_success {
            return true; // clock moved backward
        }
        now - self.last_success >= 24 * 60 * 60
    }

    /// Record a successful check (updates last_success to now).
    pub fn record_success(&mut self, app: &AppHandle) -> AppResult<()> {
        self.last_success = Self::now();
        self.save(app)
    }

    /// Mark a version as skipped.
    pub fn skip_version(&mut self, version: String, app: &AppHandle) -> AppResult<()> {
        self.skipped_version = Some(version);
        self.save(app)
    }

    /// Clear the skipped version (e.g., when a newer version is available).
    pub fn clear_skip(&mut self, app: &AppHandle) -> AppResult<()> {
        self.skipped_version = None;
        self.save(app)
    }

    /// Check if a version is skipped.
    pub fn is_skipped(&self, version: &str) -> bool {
        self.skipped_version.as_deref() == Some(version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn mock_state(last_success: i64, skipped: Option<String>) -> UpdaterStateInner {
        UpdaterStateInner {
            last_success,
            skipped_version: skipped,
        }
    }

    #[test]
    fn never_checked_is_due() {
        let s = mock_state(0, None);
        assert!(s.is_due(true));
    }

    #[test]
    fn exactly_24h_is_due() {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let s = mock_state(now - 24 * 3600, None);
        assert!(s.is_due(true));
    }

    #[test]
    fn twenty_three_hours_not_due() {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let s = mock_state(now - 23 * 3600, None);
        assert!(!s.is_due(true));
    }

    #[test]
    fn future_timestamp_is_due() {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let s = mock_state(now + 3600, None);
        assert!(s.is_due(true));
    }

    #[test]
    fn disabled_not_due() {
        let s = mock_state(0, None);
        assert!(!s.is_due(false));
    }

    #[test]
    fn skip_version_works() {
        let mut s = mock_state(0, None);
        s.skipped_version = Some("0.1.2".into());
        assert!(s.is_skipped("0.1.2"));
        assert!(!s.is_skipped("0.1.3"));
    }
}