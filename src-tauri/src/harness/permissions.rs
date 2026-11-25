//! 3-state permission policy (`Allow` / `Ask` / `Deny`), doom-loop detection,
//! and interactive [`RuntimeRequest`] pause-and-resume approval channel.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::oneshot;

use super::pipeline::PermissionGate;
use super::tool::{ToolContext, ToolMeta};
use crate::sandbox::confirm::{self, ActionKind};
use crate::util::hash;

/// Threshold of identical `(tool, args)` invocations in a single turn before
/// escalating to an interactive confirmation prompt.
const DOOM_LOOP_THRESHOLD: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionState {
    Allow,
    Ask,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ApprovalDecision {
    ApproveOnce,
    ApproveAlways,
    Reject { reason: Option<String> },
}

/// Interactive approval prompt emitted to the frontend when a tool requires
/// confirmation.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeRequest {
    pub id: String,
    pub tool: String,
    pub action_kind: String,
    pub summary: String,
    pub arguments: String,
}

/// Project-level overrides loaded from `<project>/.LocalAIse/harness.json`
/// if present.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HarnessConfigFile {
    #[serde(default)]
    permissions: HashMap<String, PermissionState>,
}

/// Evaluates `Allow` / `Ask` / `Deny` rules across session overrides,
/// project config, and default `ActionKind` posture.
#[derive(Debug, Clone)]
pub struct PermissionPolicy {
    project_overrides: HashMap<String, PermissionState>,
    agent_overrides: HashMap<String, PermissionState>,
    session_allow_always: Arc<Mutex<HashSet<String>>>,
    call_counts: Arc<Mutex<HashMap<(String, String), usize>>>,
}

impl Default for PermissionPolicy {
    fn default() -> Self {
        Self {
            project_overrides: HashMap::new(),
            agent_overrides: HashMap::new(),
            session_allow_always: Arc::new(Mutex::new(HashSet::new())),
            call_counts: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl PermissionPolicy {
    /// Loads optional `<project_root>/.LocalAIse/harness.json` permission overrides.
    pub fn for_project(project_root: &Path) -> Self {
        Self::for_project_and_agent(project_root, HashMap::new())
    }

    /// Loads project overrides and layers per-agent permission overrides on top.
    pub fn for_project_and_agent(
        project_root: &Path,
        agent_overrides: HashMap<String, PermissionState>,
    ) -> Self {
        let cfg_path = project_root.join(".LocalAIse").join("harness.json");
        let project_overrides = std::fs::read_to_string(cfg_path)
            .ok()
            .and_then(|raw| serde_json::from_str::<HarnessConfigFile>(&raw).ok())
            .map(|cfg| cfg.permissions)
            .unwrap_or_default();

        Self {
            project_overrides,
            agent_overrides,
            ..Self::default()
        }
    }

    pub fn allow_always(&self, tool_id: &str) {
        if let Ok(mut set) = self.session_allow_always.lock() {
            set.insert(tool_id.to_string());
        }
    }

    /// Evaluates the effective permission state for a tool invocation, including
    /// doom-loop escalation when identical calls repeat `>= 3` times.
    pub fn evaluate(&self, meta: &ToolMeta, args: &Value) -> (PermissionState, bool) {
        let key = (meta.id.to_string(), args.to_string());
        let repeat_count = if let Ok(mut counts) = self.call_counts.lock() {
            let entry = counts.entry(key).or_insert(0);
            *entry += 1;
            *entry
        } else {
            1
        };
        let is_doom_loop = repeat_count >= DOOM_LOOP_THRESHOLD;

        // Explicit project or agent deny always wins.
        if let Some(&PermissionState::Deny) = self.project_overrides.get(meta.id) {
            return (PermissionState::Deny, is_doom_loop);
        }
        if let Some(&PermissionState::Deny) = self.agent_overrides.get(meta.id) {
            return (PermissionState::Deny, is_doom_loop);
        }

        if is_doom_loop {
            return (PermissionState::Ask, true);
        }

        if let Ok(set) = self.session_allow_always.lock() {
            if set.contains(meta.id) {
                return (PermissionState::Allow, false);
            }
        }

        if let Some(&state) = self.agent_overrides.get(meta.id) {
            return (state, false);
        }
        if let Some(&state) = self.project_overrides.get(meta.id) {
            return (state, false);
        }

        let default_state = if meta.action_kind.requires_confirmation() {
            PermissionState::Ask
        } else {
            PermissionState::Allow
        };
        (default_state, false)
    }
}

static PENDING_REQUESTS: OnceLock<Mutex<HashMap<String, oneshot::Sender<ApprovalDecision>>>> =
    OnceLock::new();

fn pending_map() -> &'static Mutex<HashMap<String, oneshot::Sender<ApprovalDecision>>> {
    PENDING_REQUESTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Resolves a pending [`RuntimeRequest`] from a Tauri command. Returns `false`
/// if the request already expired or was cancelled.
pub fn resolve_request(request_id: &str, decision: ApprovalDecision) -> bool {
    let sender = pending_map()
        .lock()
        .ok()
        .and_then(|mut map| map.remove(request_id));
    if let Some(tx) = sender {
        tx.send(decision).is_ok()
    } else {
        false
    }
}

fn remove_pending(request_id: &str) {
    if let Ok(mut map) = pending_map().lock() {
        map.remove(request_id);
    }
}

/// Interactive `PermissionGate` that pauses tool execution on `Ask`, emits a
/// [`RuntimeRequest`] to the UI, and resumes when the user responds or cancels.
pub struct InteractivePermissionGate {
    policy: PermissionPolicy,
    on_request: Arc<dyn Fn(RuntimeRequest) + Send + Sync>,
}

impl InteractivePermissionGate {
    pub fn new(
        policy: PermissionPolicy,
        on_request: Arc<dyn Fn(RuntimeRequest) + Send + Sync>,
    ) -> Self {
        Self { policy, on_request }
    }
}

#[async_trait]
impl PermissionGate for InteractivePermissionGate {
    async fn check(
        &self,
        meta: &ToolMeta,
        args: &Value,
        ctx: &ToolContext<'_>,
    ) -> Result<(), String> {
        let (state, is_doom_loop) = self.policy.evaluate(meta, args);
        match state {
            PermissionState::Allow => Ok(()),
            PermissionState::Deny => Err(format!(
                "Tool '{}' is denied by workspace permission policy.",
                meta.id
            )),
            PermissionState::Ask => {
                let request_id = hash::unique_id("req");
                let summary = if is_doom_loop {
                    format!(
                        "Repeated identical call to '{}' ({DOOM_LOOP_THRESHOLD}+ times)",
                        meta.id
                    )
                } else {
                    format_action_summary(meta, args)
                };

                let (tx, mut rx) = oneshot::channel();
                if let Ok(mut map) = pending_map().lock() {
                    map.insert(request_id.clone(), tx);
                }

                (self.on_request)(RuntimeRequest {
                    id: request_id.clone(),
                    tool: meta.id.to_string(),
                    action_kind: action_kind_label(&meta.action_kind).to_string(),
                    summary: summary.clone(),
                    arguments: args.to_string(),
                });

                let decision = loop {
                    if ctx.is_cancelled() {
                        remove_pending(&request_id);
                        return Err("Cancelled by user.".to_string());
                    }
                    tokio::select! {
                        res = &mut rx => {
                            break res.map_err(|_| "Approval request was closed.".to_string())?;
                        }
                        _ = tokio::time::sleep(Duration::from_millis(100)) => {}
                    }
                };

                match decision {
                    ApprovalDecision::ApproveOnce => {
                        confirm::record_approved(&meta.action_kind, &summary);
                        Ok(())
                    }
                    ApprovalDecision::ApproveAlways => {
                        self.policy.allow_always(meta.id);
                        confirm::record_approved(&meta.action_kind, &summary);
                        Ok(())
                    }
                    ApprovalDecision::Reject { reason } => Err(reason.unwrap_or_else(|| {
                        format!("User rejected '{}' ({summary}).", meta.id)
                    })),
                }
            }
        }
    }
}

fn action_kind_label(kind: &ActionKind) -> &'static str {
    match kind {
        ActionKind::Read => "read",
        ActionKind::Create => "create",
        ActionKind::Modify => "modify",
        ActionKind::Delete => "delete",
        ActionKind::Network => "network",
    }
}

fn format_action_summary(meta: &ToolMeta, args: &Value) -> String {
    if let Some(path) = args.get("path").and_then(Value::as_str) {
        format!("{} on \"{}\"", meta.id, path)
    } else if let Some(url) = args.get("url").and_then(Value::as_str) {
        format!("{} on {}", meta.id, url)
    } else {
        meta.id.to_string()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn meta_for(id: &'static str, kind: ActionKind) -> ToolMeta {
        ToolMeta {
            id,
            description: "",
            parameters_schema: json!({"type": "object"}),
            action_kind: kind,
            requires_internet: false,
        }
    }

    #[test]
    fn default_policy_asks_for_modify_and_allows_read() {
        let policy = PermissionPolicy::default();
        let read_meta = meta_for("read_file", ActionKind::Read);
        let write_meta = meta_for("write_file", ActionKind::Modify);

        assert_eq!(
            policy.evaluate(&read_meta, &json!({"path": "a.txt"})),
            (PermissionState::Allow, false)
        );
        assert_eq!(
            policy.evaluate(&write_meta, &json!({"path": "out.txt"})),
            (PermissionState::Ask, false)
        );
    }

    #[test]
    fn allow_always_overrides_default_ask() {
        let policy = PermissionPolicy::default();
        let write_meta = meta_for("write_file", ActionKind::Modify);
        policy.allow_always("write_file");
        assert_eq!(
            policy.evaluate(&write_meta, &json!({"path": "out.txt"})),
            (PermissionState::Allow, false)
        );
    }

    #[test]
    fn doom_loop_escalates_to_ask_on_third_identical_call() {
        let policy = PermissionPolicy::default();
        let read_meta = meta_for("read_file", ActionKind::Read);
        let args = json!({"path": "same.txt"});

        assert_eq!(policy.evaluate(&read_meta, &args), (PermissionState::Allow, false));
        assert_eq!(policy.evaluate(&read_meta, &args), (PermissionState::Allow, false));
        assert_eq!(policy.evaluate(&read_meta, &args), (PermissionState::Ask, true));
    }

    #[tokio::test]
    async fn interactive_gate_resolves_once_and_always() {
        let policy = PermissionPolicy::default();
        let captured = Arc::new(Mutex::new(Vec::<String>::new()));
        let captured_clone = Arc::clone(&captured);

        let gate = InteractivePermissionGate::new(
            policy,
            Arc::new(move |req| {
                let id = req.id.clone();
                captured_clone.lock().unwrap().push(id.clone());
                tokio::spawn(async move {
                    resolve_request(&id, ApprovalDecision::ApproveAlways);
                });
            }),
        );

        let write_meta = meta_for("write_file", ActionKind::Modify);
        let ctx = ToolContext {
            project_root: Path::new("."),
            personal_space: Path::new("."),
            internet_enabled: false,
            should_stop: &|| false,
            serving: None,
            model_label: None,
            sampling: None,
            depth: 0,
        };

        assert!(gate
            .check(&write_meta, &json!({"path": "a.txt"}), &ctx)
            .await
            .is_ok());
        assert_eq!(captured.lock().unwrap().len(), 1);

        // Second call should now be allowed without prompting again.
        assert!(gate
            .check(&write_meta, &json!({"path": "b.txt"}), &ctx)
            .await
            .is_ok());
        assert_eq!(captured.lock().unwrap().len(), 1);
    }
}
