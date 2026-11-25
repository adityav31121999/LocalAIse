//! Child-session subagent delegation tool (`delegate_task`).
//!
//! Spawns a depth-capped child agent turn bound to a target [`AgentProfile`]
//! and returns its final answer as a standard [`ToolResult`].

use async_trait::async_trait;
use serde_json::{json, Value};

use super::tool::{ToolContext, ToolDefinition, ToolMeta};
use crate::agent::agent_chat;
use crate::sandbox::confirm::ActionKind;
use crate::tools::ToolResult;

pub const MAX_SUBAGENT_DEPTH: u32 = 2;

pub struct DelegateTaskTool;

#[async_trait]
impl ToolDefinition for DelegateTaskTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            id: "delegate_task",
            description: "Delegates a focused subtask to a specialized child agent profile (e.g. \"planner\", \"researcher\", \"coder\") and returns its final summary.",
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "Target agent profile id: \"planner\", \"researcher\", \"coder\", or \"default\""
                    },
                    "task": {
                        "type": "string",
                        "description": "Clear, self-contained task description for the child agent"
                    }
                },
                "required": ["agent_id", "task"]
            }),
            action_kind: ActionKind::Read,
            requires_internet: false,
        }
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext<'_>) -> ToolResult {
        if ctx.depth >= MAX_SUBAGENT_DEPTH {
            return ToolResult::err(
                "delegate_task",
                format!(
                    "Subagent depth limit ({MAX_SUBAGENT_DEPTH}) reached; complete the task directly."
                ),
            );
        }

        let Some(serving) = ctx.serving else {
            return ToolResult::err(
                "delegate_task",
                "Subagent execution is unavailable in this context (no active model runtime).",
            );
        };

        let agent_id = args["agent_id"].as_str().unwrap_or("default");
        let task = args["task"].as_str().unwrap_or("");
        let model_label = ctx.model_label.unwrap_or("local");
        let sampling = ctx.sampling.clone().unwrap_or_default();

        let system = agent_chat::instructions(
            "You are a focused subagent executing a delegated subtask. Return a concise, factual summary of your findings.",
            ctx.internet_enabled,
            false,
        );

        let fut = Box::pin(agent_chat::run_with_depth(
            serving,
            model_label,
            system,
            Vec::new(),
            task.to_string(),
            Vec::new(),
            ctx.project_root,
            ctx.personal_space,
            ctx.internet_enabled,
            sampling,
            0,
            Some(agent_id),
            None,
            ctx.depth + 1,
            ctx.should_stop,
            |_call, _delta| {},
            |_call, _delta| {},
            |_event| {},
            |_req| {},
        ));

        match fut.await {
            Ok(turn) => {
                let summary = if turn.content.trim().is_empty() {
                    "(subagent produced no text output)".to_string()
                } else {
                    turn.content
                };
                ToolResult::ok(
                    "delegate_task",
                    format!(
                        "[subagent:{agent_id} tools_used={:?}]\n{summary}",
                        turn.tools_used
                    ),
                )
            }
            Err(err) => ToolResult::err("delegate_task", format!("Subagent '{agent_id}' failed: {err}")),
        }
    }
}
