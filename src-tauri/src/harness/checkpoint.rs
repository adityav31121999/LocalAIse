//! Workspace file checkpointing and rollback for non-git and git folders.
//!
//! Before any `Modify` or `Delete` tool runs during a turn, [`CheckpointHook`]
//! snapshots the pre-turn state of the target file into
//! `<project>/.LocalAIse/checkpoints/<checkpoint_id>/`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::pipeline::ToolHook;
use super::tool::{ToolContext, ToolMeta};
use crate::error::AppResult;
use crate::sandbox::confirm::ActionKind;
use crate::sandbox::paths;
use crate::tools::ToolResult;
use crate::util::{hash, time};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointEntry {
    pub relative_path: String,
    pub existed_before: bool,
    pub backup_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointManifest {
    pub id: String,
    pub session_id: String,
    pub created_at: String,
    pub entries: Vec<CheckpointEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RollbackSummary {
    pub checkpoint_id: String,
    pub restored_files: Vec<String>,
    pub removed_files: Vec<String>,
}

/// Manages pre-turn file snapshots under `<project>/.LocalAIse/checkpoints/`.
pub struct CheckpointManager {
    id: String,
    project_root: PathBuf,
    dir: PathBuf,
    recorded_paths: Mutex<HashSet<String>>,
    manifest: Mutex<CheckpointManifest>,
}

impl CheckpointManager {
    pub fn start(project_root: &Path, session_id: &str) -> Self {
        let id = hash::unique_id("chk");
        let canon_root = std::fs::canonicalize(project_root).unwrap_or_else(|_| project_root.to_path_buf());
        let dir = canon_root.join(".LocalAIse").join("checkpoints").join(&id);
        let manifest = CheckpointManifest {
            id: id.clone(),
            session_id: session_id.to_string(),
            created_at: time::now_iso(),
            entries: Vec::new(),
        };
        Self {
            id,
            project_root: canon_root,
            dir,
            recorded_paths: Mutex::new(HashSet::new()),
            manifest: Mutex::new(manifest),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// Snapshots `raw_rel_path` (if inside `project_root`) before modification.
    /// Each path is snapshotted at most once per turn.
    pub fn snapshot_before_write(&self, raw_rel_path: &str) -> AppResult<()> {
        let trimmed = raw_rel_path.trim().trim_start_matches('@');
        if trimmed.is_empty() {
            return Ok(());
        }

        // Resolve where write_file/append_file will actually land: existing
        // project path, or `outputs/<name>` for new files.
        let direct = paths::ensure_within(&self.project_root, Path::new(trimmed));
        let target = match direct {
            Ok(p) if p.exists() => p,
            _ => {
                let out_rel = if trimmed.starts_with("outputs/") || trimmed.starts_with("outputs\\")
                {
                    PathBuf::from(trimmed)
                } else {
                    PathBuf::from("outputs").join(trimmed)
                };
                paths::ensure_within(&self.project_root, &out_rel)?
            }
        };

        let rel = target
            .strip_prefix(&self.project_root)
            .unwrap_or(&target)
            .to_string_lossy()
            .replace('\\', "/");

        {
            let mut seen = self.recorded_paths.lock().unwrap();
            if !seen.insert(rel.clone()) {
                return Ok(());
            }
        }

        std::fs::create_dir_all(&self.dir)?;
        let entry = if target.exists() {
            let backup_name = format!("{}.bak", hash::unique_id("f"));
            let backup_path = self.dir.join(&backup_name);
            std::fs::copy(&target, &backup_path)?;
            CheckpointEntry {
                relative_path: rel,
                existed_before: true,
                backup_file: Some(backup_name),
            }
        } else {
            CheckpointEntry {
                relative_path: rel,
                existed_before: false,
                backup_file: None,
            }
        };

        let mut manifest = self.manifest.lock().unwrap();
        manifest.entries.push(entry);
        let manifest_json = serde_json::to_string_pretty(&*manifest)?;
        std::fs::write(self.dir.join("manifest.json"), manifest_json)?;
        Ok(())
    }

    pub fn list(project_root: &Path) -> AppResult<Vec<CheckpointManifest>> {
        let root = project_root.join(".LocalAIse").join("checkpoints");
        let Ok(entries) = std::fs::read_dir(root) else {
            return Ok(Vec::new());
        };
        let mut manifests = Vec::new();
        for entry in entries.flatten() {
            let manifest_path = entry.path().join("manifest.json");
            if let Ok(raw) = std::fs::read_to_string(manifest_path) {
                if let Ok(m) = serde_json::from_str::<CheckpointManifest>(&raw) {
                    manifests.push(m);
                }
            }
        }
        manifests.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(manifests)
    }

    pub fn rollback(project_root: &Path, checkpoint_id: &str) -> AppResult<RollbackSummary> {
        let dir = project_root
            .join(".LocalAIse")
            .join("checkpoints")
            .join(checkpoint_id);
        let raw = std::fs::read_to_string(dir.join("manifest.json"))?;
        let manifest: CheckpointManifest = serde_json::from_str(&raw)?;

        let mut restored_files = Vec::new();
        let mut removed_files = Vec::new();

        for entry in &manifest.entries {
            let target = paths::ensure_within(project_root, Path::new(&entry.relative_path))?;
            if entry.existed_before {
                if let Some(backup) = &entry.backup_file {
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::copy(dir.join(backup), &target)?;
                    restored_files.push(entry.relative_path.clone());
                }
            } else if target.exists() {
                std::fs::remove_file(&target)?;
                removed_files.push(entry.relative_path.clone());
            }
        }

        let _ = &manifest.session_id;
        Ok(RollbackSummary {
            checkpoint_id: checkpoint_id.to_string(),
            restored_files,
            removed_files,
        })
    }
}

/// [`ToolHook`] that snapshots target files before `Modify` / `Delete` tools run.
pub struct CheckpointHook {
    manager: Arc<CheckpointManager>,
}

impl CheckpointHook {
    pub fn new(manager: Arc<CheckpointManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl ToolHook for CheckpointHook {
    async fn before_tool(
        &self,
        meta: &ToolMeta,
        args: &Value,
        _ctx: &ToolContext<'_>,
    ) -> Result<(), String> {
        if matches!(meta.action_kind, ActionKind::Modify | ActionKind::Delete) {
            if let Some(path) = args.get("path").and_then(Value::as_str) {
                if let Err(err) = self.manager.snapshot_before_write(path) {
                    log::warn!("[checkpoint] snapshot failed for '{}': {}", path, err);
                }
            }
        }
        Ok(())
    }

    async fn after_tool(
        &self,
        _meta: &ToolMeta,
        _args: &Value,
        _result: &mut ToolResult,
        _ctx: &ToolContext<'_>,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_and_rolls_back_modified_and_created_files() {
        let dir = std::env::temp_dir().join(format!(
            "localaise_chk_test_{}",
            hash::unique_id("t")
        ));
        std::fs::create_dir_all(dir.join("outputs")).unwrap();
        let existing = dir.join("outputs").join("existing.txt");
        std::fs::write(&existing, "original").unwrap();

        let mgr = CheckpointManager::start(&dir, "s1");
        mgr.snapshot_before_write("outputs/existing.txt").unwrap();
        mgr.snapshot_before_write("outputs/created.txt").unwrap();

        // Simulate tool writes
        std::fs::write(&existing, "mutated").unwrap();
        let created = dir.join("outputs").join("created.txt");
        std::fs::write(&created, "new file").unwrap();

        let summary = CheckpointManager::rollback(&dir, mgr.id()).unwrap();
        assert_eq!(summary.restored_files, vec!["outputs/existing.txt"]);
        assert_eq!(summary.removed_files, vec!["outputs/created.txt"]);
        assert_eq!(std::fs::read_to_string(&existing).unwrap(), "original");
        assert!(!created.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
