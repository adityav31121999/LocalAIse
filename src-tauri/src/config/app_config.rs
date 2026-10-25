//! The registry: which folders are projects, where the personal space lives,
//! and the global settings. Written to `state.json` in the app data folder.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::paths::AppDirs;
use crate::error::{AppError, AppResult};
use crate::projects::layout::{ChatMode, ProjectTemplate};
use crate::runtime::profile::{InferenceProfile, ProfileScope};
use crate::util::paths::plain;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// quantization, threads and device placement.
    ///
    /// A model or a project may store its own profile; this one is what they
    /// inherit when they do not.
    pub performance: InferenceProfile,
    /// Profiles stored for a single model, keyed by the model's path.
    #[serde(default)]
    pub model_profiles: BTreeMap<String, InferenceProfile>,
    /// Profiles stored for a single project, keyed by the project id.
    #[serde(default)]
    pub project_profiles: BTreeMap<String, InferenceProfile>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_internet: false,
            llama_server_path: None,
            selected_model: None,
            model_folders: Vec::new(),
            performance: InferenceProfile::desktop_default(),
            model_profiles: BTreeMap::new(),
            project_profiles: BTreeMap::new(),
        }
    }
}

impl Settings {
    /// The key a model's profile is stored under.
    ///
    /// Windows paths are compared without regard to case, because the same
    /// file can reach the app spelled in more than one way. Everywhere else
    /// the path is used as it is.
    pub fn model_key(path: &Path) -> String {
        let text = path.to_string_lossy().into_owned();
        if cfg!(windows) {
            text.to_lowercase()
        } else {
            text
        }
    }

    /// The profile stored for one model.
    pub fn model_profile(&self, model: Option<&Path>) -> Option<&InferenceProfile> {
        let path = model?;
        self.model_profiles.get(&Self::model_key(path))
    }

    /// The profile stored for one project.
    pub fn project_profile(&self, project_id: Option<&str>) -> Option<&InferenceProfile> {
        let id = project_id?;
        self.project_profiles.get(id)
    }

    /// The profile that is in effect for a model in a project.
    ///
    /// The project wins over the model, and the model over the global
    /// default: the more specific the scope, the later it was asked for.
    pub fn profile_for(&self, model: Option<&Path>, project_id: Option<&str>) -> &InferenceProfile {
        self.project_profile(project_id)
            .or_else(|| self.model_profile(model))
            .unwrap_or(&self.performance)
    }

    /// Which scope the effective profile came from.
    pub fn scope_for(&self, model: Option<&Path>, project_id: Option<&str>) -> ProfileScope {
        if self.project_profile(project_id).is_some() {
            ProfileScope::Project
        } else if self.model_profile(model).is_some() {
            ProfileScope::Model
        } else {
            ProfileScope::Global
        }
    }

    /// The profile a scope starts from: its own when it has one, otherwise
    /// the next scope down.
    pub fn base_for(
        &self,
        scope: ProfileScope,
        model: Option<&Path>,
        project_id: Option<&str>,
    ) -> &InferenceProfile {
        match scope {
            ProfileScope::Global => &self.performance,
            ProfileScope::Model => self
                .model_profile(model)
                .unwrap_or(&self.performance),
            ProfileScope::Project => self
                .project_profile(project_id)
                .or_else(|| self.model_profile(model))
                .unwrap_or(&self.performance),
        }
    }

    /// Stores a profile in a scope.
    ///
    /// The global scope always exists; the other two need the thing they
    /// belong to, so that a profile is never written where it cannot be
    /// found again.
    pub fn store_profile(
        &mut self,
        scope: ProfileScope,
        model: Option<&Path>,
        project_id: Option<&str>,
        profile: InferenceProfile,
    ) -> AppResult<()> {
        match scope {
            ProfileScope::Global => {
                self.performance = profile;
            }
            ProfileScope::Model => {
                let path = model.ok_or_else(|| AppError::msg("No model is selected."))?;
                self.model_profiles.insert(Self::model_key(path), profile);
            }
            ProfileScope::Project => {
                let id = project_id
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| AppError::msg("No project is open."))?;
                self.project_profiles.insert(id.to_string(), profile);
            }
        }
        Ok(())
    }

    /// Removes the override of a scope, so the next scope down applies again.
    ///
    /// The global scope has nothing above it to fall back to, so clearing it
    /// restores the app's own default.
    pub fn clear_profile(
        &mut self,
        scope: ProfileScope,
        model: Option<&Path>,
        project_id: Option<&str>,
    ) -> AppResult<()> {
        match scope {
            ProfileScope::Global => {
                self.performance = InferenceProfile::desktop_default();
            }
            ProfileScope::Model => {
                let path = model.ok_or_else(|| AppError::msg("No model is selected."))?;
                self.model_profiles.remove(&Self::model_key(path));
            }
            ProfileScope::Project => {
                let id = project_id
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| AppError::msg("No project is open."))?;
                self.project_profiles.remove(id);
            }
        }
        Ok(())
    }
}

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
mod tests {
    use super::*;
    use crate::runtime::profile::KvCacheType;

    fn model(path: &str) -> PathBuf {
        PathBuf::from(path)
    }

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

    #[test]
    fn the_global_default_applies_without_overrides() {
        let mut settings = Settings::default();
        settings.performance = profile(8_192);

        let path = model("/models/llama.gguf");
        assert_eq!(
            settings.profile_for(Some(&path), Some("project-1")).context_tokens,
            8_192
        );
        assert_eq!(settings.scope_for(Some(&path), Some("project-1")), ProfileScope::Global);
    }

    #[test]
    fn a_model_profile_overrides_the_global_default() {
        let mut settings = Settings::default();
        settings.performance = profile(8_192);
        settings
            .store_profile(ProfileScope::Model, Some(&model("/models/llama.gguf")), None, profile(32_768))
            .unwrap();

        let path = model("/models/llama.gguf");
        assert_eq!(settings.profile_for(Some(&path), None).context_tokens, 32_768);
        assert_eq!(settings.scope_for(Some(&path), None), ProfileScope::Model);

        // Another model still uses the global default.
        let other = model("/models/other.gguf");
        assert_eq!(settings.profile_for(Some(&other), None).context_tokens, 8_192);
    }

    #[test]
    fn a_project_profile_wins_over_the_model_profile() {
        let mut settings = Settings::default();
        settings.performance = profile(8_192);
        settings
            .store_profile(
                ProfileScope::Model,
                Some(&model("/models/llama.gguf")),
                None,
                profile(32_768),
            )
            .unwrap();
        settings
            .store_profile(
                ProfileScope::Project,
                Some(&model("/models/llama.gguf")),
                Some("project-1"),
                profile(65_536),
            )
            .unwrap();

        let path = model("/models/llama.gguf");
        assert_eq!(
            settings.profile_for(Some(&path), Some("project-1")).context_tokens,
            65_536
        );
        assert_eq!(
            settings.scope_for(Some(&path), Some("project-1")),
            ProfileScope::Project
        );

        // Outside the project the model's own settings apply again.
        assert_eq!(settings.profile_for(Some(&path), None).context_tokens, 32_768);
    }

    #[test]
    fn removing_an_override_falls_back_to_the_scope_below() {
        let mut settings = Settings::default();
        settings.performance = profile(8_192);
        let path = model("/models/llama.gguf");

        settings
            .store_profile(ProfileScope::Model, Some(&path), None, profile(32_768))
            .unwrap();
        assert_eq!(settings.profile_for(Some(&path), None).context_tokens, 32_768);

        settings.clear_profile(ProfileScope::Model, Some(&path), None).unwrap();
        assert_eq!(settings.profile_for(Some(&path), None).context_tokens, 8_192);
        assert_eq!(settings.scope_for(Some(&path), None), ProfileScope::Global);
    }

    #[test]
    fn clearing_the_global_default_restores_the_app_default() {
        let mut settings = Settings::default();
        settings.performance = profile(32_768);
        settings.clear_profile(ProfileScope::Global, None, None).unwrap();
        assert_eq!(settings.performance, InferenceProfile::desktop_default());
    }

    #[test]
    fn a_profile_without_a_model_or_project_is_refused() {
        let mut settings = Settings::default();
        assert!(settings
            .store_profile(ProfileScope::Model, None, None, profile(8_192))
            .is_err());
        assert!(settings
            .store_profile(ProfileScope::Project, Some(&model("/m.gguf")), None, profile(8_192))
            .is_err());
    }

    #[test]
    fn a_scope_starts_from_the_next_one_down() {
        let mut settings = Settings::default();
        settings.performance = profile(8_192);
        let path = model("/models/llama.gguf");

        assert_eq!(
            settings.base_for(ProfileScope::Global, Some(&path), None).context_tokens,
            8_192
        );
        assert_eq!(
            settings.base_for(ProfileScope::Model, Some(&path), None).context_tokens,
            8_192
        );

        settings
            .store_profile(ProfileScope::Model, Some(&path), None, profile(32_768))
            .unwrap();
        assert_eq!(
            settings.base_for(ProfileScope::Model, Some(&path), None).context_tokens,
            32_768
        );
        // A project inherits the model's settings when it has none of its own.
        assert_eq!(
            settings.base_for(ProfileScope::Project, Some(&path), None).context_tokens,
            32_768
        );
    }

    #[test]
    fn an_old_state_file_still_loads() {
        // A file written before the per-model and per-project profiles
        // existed must not lose the settings it does have.
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
        assert!(settings.model_profiles.is_empty());
        assert!(settings.project_profiles.is_empty());
    }

    #[test]
    fn the_stored_profiles_survive_a_round_trip() {
        let mut settings = Settings::default();
        settings
            .store_profile(
                ProfileScope::Model,
                Some(&model("/models/llama.gguf")),
                None,
                profile(32_768),
            )
            .unwrap();
        settings
            .store_profile(
                ProfileScope::Project,
                Some(&model("/models/llama.gguf")),
                Some("project-1"),
                profile(65_536),
            )
            .unwrap();

        let raw = serde_json::to_string(&settings).unwrap();
        let restored: Settings = serde_json::from_str(&raw).unwrap();
        assert_eq!(restored.model_profiles.len(), 1);
        assert_eq!(restored.project_profiles.len(), 1);
    }
}
