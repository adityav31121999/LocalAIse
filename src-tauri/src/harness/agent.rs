//! Named agent profiles and registry (OpenCode pattern).
//!
//! Profiles bundle system instructions, tool allowlists, step/truncation
//! budgets, and per-agent permission overrides. Custom profiles can be placed
//! in `<project>/.LocalAIse/agents/*.json`, and project-wide instructions in
//! `AGENTS.md` or `.LocalAIse/AGENTS.md` are merged automatically.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::permissions::PermissionState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfile {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// `None` allows all registered tools; `Some(vec)` restricts to those IDs.
    #[serde(default)]
    pub tool_allowlist: Option<Vec<String>>,
    #[serde(default = "default_max_steps")]
    pub max_steps: u32,
    #[serde(default = "default_max_tool_result_chars")]
    pub max_tool_result_chars: usize,
    #[serde(default)]
    pub permission_overrides: HashMap<String, PermissionState>,
}

fn default_max_steps() -> u32 {
    6
}

fn default_max_tool_result_chars() -> usize {
    8_000
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfileSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub max_steps: u32,
    pub read_only: bool,
}

impl From<&AgentProfile> for AgentProfileSummary {
    fn from(p: &AgentProfile) -> Self {
        let read_only = p
            .tool_allowlist
            .as_ref()
            .is_some_and(|list| !list.iter().any(|t| t == "write_file" || t == "append_file"));
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            description: p.description.clone(),
            max_steps: p.max_steps,
            read_only,
        }
    }
}

/// Ordered registry of built-in and project-local [`AgentProfile`]s.
#[derive(Debug, Clone, Default)]
pub struct AgentRegistry {
    profiles: Vec<AgentProfile>,
}

impl AgentRegistry {
    pub fn for_project(project_root: &Path) -> Self {
        let mut registry = Self {
            profiles: builtin_profiles(),
        };
        registry.load_project_agents(project_root);
        registry
    }

    pub fn resolve(&self, agent_id: Option<&str>) -> AgentProfile {
        let target = agent_id.unwrap_or("default");
        self.profiles
            .iter()
            .find(|p| p.id == target)
            .cloned()
            .unwrap_or_else(|| self.profiles[0].clone())
    }

    pub fn list(&self) -> Vec<AgentProfileSummary> {
        self.profiles.iter().map(AgentProfileSummary::from).collect()
    }

    fn load_project_agents(&mut self, project_root: &Path) {
        let dir = project_root.join(".LocalAIse").join("agents");
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(raw) = std::fs::read_to_string(&path) {
                if let Ok(profile) = serde_json::from_str::<AgentProfile>(&raw) {
                    if let Some(existing) = self.profiles.iter_mut().find(|p| p.id == profile.id) {
                        *existing = profile;
                    } else {
                        self.profiles.push(profile);
                    }
                }
            }
        }
    }
}

/// Loads optional `AGENTS.md` or `.LocalAIse/AGENTS.md` instructions from
/// the project root (capped at 4,000 bytes to protect local context windows).
pub fn load_project_instructions(project_root: &Path) -> Option<String> {
    const MAX_INSTRUCTION_BYTES: usize = 4_000;
    for candidate in [
        project_root.join("AGENTS.md"),
        project_root.join(".LocalAIse").join("AGENTS.md"),
    ] {
        if let Ok(raw) = std::fs::read_to_string(&candidate) {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                let bounded: String = trimmed.chars().take(MAX_INSTRUCTION_BYTES).collect();
                return Some(bounded);
            }
        }
    }
    None
}

fn builtin_profiles() -> Vec<AgentProfile> {
    let mut planner_perms = HashMap::new();
    planner_perms.insert("write_file".to_string(), PermissionState::Deny);
    planner_perms.insert("append_file".to_string(), PermissionState::Deny);

    vec![
        AgentProfile {
            id: "default".to_string(),
            name: "Default".to_string(),
            description: "General workspace assistant with full sandboxed tool access."
                .to_string(),
            system_prompt: None,
            tool_allowlist: None,
            max_steps: 6,
            max_tool_result_chars: 8_000,
            permission_overrides: HashMap::new(),
        },
        AgentProfile {
            id: "planner".to_string(),
            name: "Planner".to_string(),
            description: "Read-only exploration and planning; file writes are blocked."
                .to_string(),
            system_prompt: Some(
                "You are in PLANNER mode: inspect files and propose a clear step-by-step plan. \
                 Do not attempt to modify files."
                    .to_string(),
            ),
            tool_allowlist: Some(vec![
                "read_file".to_string(),
                "list_files".to_string(),
                "search_local".to_string(),
                "search_diary".to_string(),
                "list_events".to_string(),
            ]),
            max_steps: 6,
            max_tool_result_chars: 8_000,
            permission_overrides: planner_perms,
        },
        AgentProfile {
            id: "researcher".to_string(),
            name: "Researcher".to_string(),
            description: "Searches local project sources and the web (when enabled)."
                .to_string(),
            system_prompt: Some(
                "You are in RESEARCHER mode: gather and corroborate facts from project files \
                 and web sources before synthesizing an answer."
                    .to_string(),
            ),
            tool_allowlist: Some(vec![
                "read_file".to_string(),
                "list_files".to_string(),
                "search_local".to_string(),
                "web_search".to_string(),
                "web_fetch".to_string(),
            ]),
            max_steps: 8,
            max_tool_result_chars: 8_000,
            permission_overrides: HashMap::new(),
        },
        AgentProfile {
            id: "coder".to_string(),
            name: "Coder".to_string(),
            description: "Extended step and output budget for multi-file edits and analysis."
                .to_string(),
            system_prompt: None,
            tool_allowlist: None,
            max_steps: 10,
            max_tool_result_chars: 12_000,
            permission_overrides: HashMap::new(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_builtin_and_falls_back_to_default() {
        let reg = AgentRegistry::for_project(Path::new("."));
        assert_eq!(reg.resolve(None).id, "default");
        assert_eq!(reg.resolve(Some("planner")).id, "planner");
        assert_eq!(reg.resolve(Some("unknown-id")).id, "default");
        assert!(reg.list().iter().any(|p| p.id == "planner" && p.read_only));
    }
}
