//! Context-budget-aware deferred tool loading (`tool_search`, T3 Code pattern).
//!
//! When a session has many registered tools (e.g., built-ins + MCP servers),
//! only core tools and `tool_search` are sent initially to protect the local
//! model's context window. Calling `tool_search` unlocks matching tools for
//! subsequent steps in the turn.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{json, Value};

use super::tool::{ToolContext, ToolDefinition, ToolMeta};
use crate::sandbox::confirm::ActionKind;
use crate::tools::ToolResult;

/// Threshold above which `ToolRegistry::openai_specs_deferred` switches to
/// core tools + `tool_search`.
pub const DEFERRED_TOOL_THRESHOLD: usize = 12;

/// Core tools always exposed even in deferred mode.
pub const CORE_DEFERRED_TOOLS: &[&str] = &[
    "read_file",
    "list_files",
    "search_local",
    "delegate_task",
    "tool_search",
];

#[derive(Debug, Clone)]
pub struct ToolSearchCandidate {
    pub id: String,
    pub description: String,
}

pub struct ToolSearchTool {
    catalog: Vec<ToolSearchCandidate>,
    unlocked: Arc<Mutex<HashSet<String>>>,
}

impl ToolSearchTool {
    pub fn new(
        catalog: Vec<ToolSearchCandidate>,
        unlocked: Arc<Mutex<HashSet<String>>>,
    ) -> Self {
        Self { catalog, unlocked }
    }
}

#[async_trait]
impl ToolDefinition for ToolSearchTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            id: "tool_search",
            description: "Searches available tools by keyword and unlocks matching tools for the remainder of this session.",
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Keywords describing the capability you need"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum tools to unlock (default 6, max 10)"
                    }
                },
                "required": ["query"]
            }),
            action_kind: ActionKind::Read,
            requires_internet: false,
        }
    }

    async fn execute(&self, args: &Value, _ctx: &ToolContext<'_>) -> ToolResult {
        let query = args["query"].as_str().unwrap_or("").to_lowercase();
        let limit = args["limit"].as_u64().unwrap_or(6).clamp(1, 10) as usize;
        let terms: Vec<&str> = query.split_whitespace().filter(|t| !t.is_empty()).collect();

        let mut scored: Vec<(usize, &ToolSearchCandidate)> = self
            .catalog
            .iter()
            .filter(|c| c.id != "tool_search")
            .filter_map(|c| {
                let hay = format!("{} {}", c.id.to_lowercase(), c.description.to_lowercase());
                let score = if terms.is_empty() {
                    1
                } else {
                    terms.iter().filter(|t| hay.contains(*t)).count()
                };
                if score > 0 {
                    Some((score, c))
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
        scored.truncate(limit);

        if scored.is_empty() {
            return ToolResult::ok("tool_search", "No matching tools found.");
        }

        let mut lines = Vec::new();
        if let Ok(mut set) = self.unlocked.lock() {
            for (_, cand) in &scored {
                set.insert(cand.id.clone());
                lines.push(format!("- {}: {}", cand.id, cand.description));
            }
        }
        ToolResult::ok(
            "tool_search",
            format!("Unlocked {} tool(s) for your next step:\n{}", lines.len(), lines.join("\n")),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[tokio::test]
    async fn tool_search_unlocks_matching_tools() {
        let unlocked = Arc::new(Mutex::new(HashSet::new()));
        let tool = ToolSearchTool::new(
            vec![
                ToolSearchCandidate {
                    id: "mcp_github_list_issues".into(),
                    description: "List GitHub repository issues".into(),
                },
                ToolSearchCandidate {
                    id: "mcp_slack_send".into(),
                    description: "Send a Slack message".into(),
                },
            ],
            Arc::clone(&unlocked),
        );
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
        let res = tool.execute(&json!({"query": "github issues"}), &ctx).await;
        assert!(res.error.is_none());
        assert!(res.output.contains("mcp_github_list_issues"));
        assert!(unlocked.lock().unwrap().contains("mcp_github_list_issues"));
        assert!(!unlocked.lock().unwrap().contains("mcp_slack_send"));
    }
}
