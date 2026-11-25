//! Core tool trait and execution context for the agent harness.

use std::path::Path;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::runtime::client::SamplingParams;
use crate::runtime::Serving;
use crate::sandbox::confirm::ActionKind;
use crate::tools::ToolResult;

/// Metadata describing a registered tool, used for OpenAI schema generation,
/// permission classification, and network gating.
#[derive(Debug, Clone)]
pub struct ToolMeta {
    pub id: &'static str,
    pub description: &'static str,
    pub parameters_schema: Value,
    pub action_kind: ActionKind,
    pub requires_internet: bool,
}

/// Execution context injected into every tool invocation.
pub struct ToolContext<'a> {
    pub project_root: &'a Path,
    pub personal_space: &'a Path,
    pub internet_enabled: bool,
    pub should_stop: &'a (dyn Fn() -> bool + Send + Sync),
    pub serving: Option<&'a Serving>,
    pub model_label: Option<&'a str>,
    pub sampling: Option<SamplingParams>,
    pub depth: u32,
}

impl<'a> ToolContext<'a> {
    pub fn is_cancelled(&self) -> bool {
        (self.should_stop)()
    }
}

/// Unified interface implemented by all built-in and future MCP/plugin tools.
#[async_trait]
pub trait ToolDefinition: Send + Sync {
    fn meta(&self) -> ToolMeta;

    async fn execute(&self, args: &Value, ctx: &ToolContext<'_>) -> ToolResult;

    /// Serializes this tool into an OpenAI-compatible function tool specification.
    fn to_openai_spec(&self) -> Value {
        let meta = self.meta();
        json!({
            "type": "function",
            "function": {
                "name": meta.id,
                "description": meta.description,
                "parameters": meta.parameters_schema,
            }
        })
    }
}
