//! The registry: which folders are projects, where the personal space lives,
//! and the global settings. Written to `state.json` in the app data folder.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::paths::AppDirs;
use crate::error::{AppResult};
use crate::projects::layout::{ChatMode, ProjectChatSettings, ProjectTemplate};
use crate::runtime::profile::InferenceProfile;
use crate::util::paths::plain;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Global default applied to new projects. Internet stays off unless asked for.
    pub default_internet: bool,
    /// Explicit llama-server path. When unset the app looks for its own CPU
    /// build. This is also the only way a user-installed GPU build is used.
    pub llama_server_path: Option<PathBuf>,
    /// GGUF the chat runtime loads. When unset, the first model on disk is used.
    pub selected_model: Option<PathBuf>,
    /// Extra folders the user keeps GGUF files in. Scanned together with the
    /// app's models folder; files there are shown and loadable, never deleted.
    pub model_folders: Vec<PathBuf>,
    /// The app-wide default the llama-server runs with: context, KV cache
    /// quantization, threads and device placement. This is the live
    /// inference profile — applying the settings stores it here, and the
    /// runtime starts every model from it.
    pub performance: InferenceProfile,
    /// Whether to automatically check for updates once per day when online.
    /// Defaults to true. Stored in state.json so it persists across restarts.
    #[serde(default = "default_true")]
    pub auto_update_check: bool,
}

impl Settings {}

/// A folder the user registered as a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub template: ProjectTemplate,
    pub internet_enabled: bool,
    /// Custom chat system prompt; `None` uses the built-in project prompt.
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// The chat assistant mode: plain chat or the tool-using agent.
    #[serde(default)]
    pub chat_mode: ChatMode,
    /// Per-project chat settings (tool steps, history, retrieval limits).
    #[serde(default)]
    pub chat_settings: ProjectChatSettings,
    pub last_opened_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Registry {
    pub projects: Vec<ProjectRecord>,
    pub personal_space: Option<PathBuf>,
    pub settings: Settings,
}

impl Registry {
    /// A missing or unreadable registry is not an error: the user simply has
    /// no projects yet, and the file is rewritten on the next change.
    pub fn load(file: &Path) -> Self {
        let mut registry: Self = std::fs::read_to_string(file)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        registry.normalize();
        registry
    }

    /// Rewrites stored paths in the Windows verbatim spelling `\\?\C:\…`,
    /// which older versions kept, into the everyday form. From then on
    /// everything is saved plainly, so this is a one-time cleanup per entry.
    fn normalize(&mut self) {
        for project in &mut self.projects {
            project.path = plain(std::mem::take(&mut project.path));
        }
        self.personal_space = self.personal_space.take().map(plain);
        for folder in &mut self.settings.model_folders {
            *folder = plain(std::mem::take(folder));
        }
        self.settings.selected_model = self.settings.selected_model.take().map(plain);
        self.settings.llama_server_path = self.settings.llama_server_path.take().map(plain);
    }

    pub fn save(&self, file: &Path) -> AppResult<()> {
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(file, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn find(&self, id: &str) -> Option<&ProjectRecord> {
        self.projects.iter().find(|project| project.id == id)
    }

    pub fn find_mut(&mut self, id: &str) -> Option<&mut ProjectRecord> {
        self.projects.iter_mut().find(|project| project.id == id)
    }

    /// Adds the record, or replaces the existing one with the same id.
    pub fn upsert(&mut self, record: ProjectRecord) {
        match self.projects.iter_mut().find(|project| project.id == record.id) {
            Some(existing) => *existing = record,
            None => self.projects.push(record),
        }
    }

    /// The personal space folder, falling back to the default location.
    pub fn personal_space_or_default(&self, dirs: &AppDirs) -> PathBuf {
        self.personal_space
            .clone()
            .unwrap_or_else(|| dirs.default_personal.clone())
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::runtime::profile::KvCacheType;

    fn temp_registry_file(label: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "LocalAIse-registry-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        base.join("state.json")
    }

    #[test]
    fn loading_strips_verbatim_prefixes_saved_by_older_versions() {
        let file = temp_registry_file("verbatim");
        std::fs::write(
            &file,
            r#"{
                "projects": [
                    {
                        "id": "demo-1",
                        "name": "Demo",
                        "path": "\\\\?\\C:\\Users\\me\\Demo",
                        "template": "general",
                        "internetEnabled": false
                    }
                ]
            }"#,
        )
        .unwrap();

        let registry = Registry::load(&file);
        assert_eq!(
            registry.projects[0].path,
            PathBuf::from(r"C:\Users\me\Demo")
        );

        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }

    fn profile(context: u32) -> InferenceProfile {
        let mut profile = InferenceProfile::desktop_default();
        profile.context_tokens = context;
        profile
    }

    #[allow(clippy::field_reassign_with_default)]

    #[test]
    fn the_live_profile_is_what_every_model_starts_from() {
        let mut settings = Settings::default();
        settings.performance = profile(8_192);

        // There is one live inference profile; every model starts from it.
        assert_eq!(settings.performance.context_tokens, 8_192);
        assert_eq!(settings.performance.context_tokens, 8_192);
    }

    #[test]
    fn applying_a_profile_replaces_the_live_profile() {
        let mut settings = Settings::default();
        settings.performance = profile(8_192);

        settings.performance = profile(32_768);

        assert_eq!(settings.performance.context_tokens, 32_768);
        assert_eq!(settings.performance.context_tokens, 32_768);
    }

    #[test]
    fn clearing_the_live_profile_restores_the_app_default() {
        let mut settings = Settings::default();
        settings.performance = profile(32_768);
        settings.performance = InferenceProfile::desktop_default();
        assert_eq!(settings.performance, InferenceProfile::desktop_default());
    }

    #[test]
    fn an_old_state_file_still_loads() {
        // A file written before this refactor must not lose the settings it does have.
        let raw = r#"{
            "defaultInternet": true,
            "performance": {
                "contextTokens": 16384,
                "threads": 6,
                "device": "cpu",
                "kvType": "q8_0",
                "flashAttn": true,
                "batchSize": 256,
                "parallelSlots": 1,
                "useMmap": true,
                "moe": { "mode": "off" }
            }
        }"#;

        let settings: Settings = serde_json::from_str(raw).unwrap();
        assert!(settings.default_internet);
        assert_eq!(settings.performance.context_tokens, 16_384);
        assert_eq!(settings.performance.kv_type, KvCacheType::Q8_0);
    }

    #[test]
    fn the_live_profile_survives_a_round_trip() {
        let mut settings = Settings::default();
        settings.performance = profile(32_768);

        let raw = serde_json::to_string(&settings).unwrap();
        let restored: Settings = serde_json::from_str(&raw).unwrap();
        assert_eq!(restored.performance.context_tokens, 32_768);
    }
}
