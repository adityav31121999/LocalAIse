//! 5-stage tool execution pipeline:
//! 1. Registry lookup & internet availability check
//! 2. JSON Schema argument validation
//! 3. Permission gate
//! 4. Before-execution hooks
//! 5. Execution + After-execution hooks

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use super::registry::ToolRegistry;
use super::tool::{ToolContext, ToolMeta};
use crate::tools::ToolResult;

/// Permission decision hook invoked at Stage 3 of [`ToolPipeline`].
/// Phase 1 provides [`AllowAllGate`]; Phase 2 will wire in interactive
/// `allow` / `ask` / `deny` policies.
#[async_trait]
pub trait PermissionGate: Send + Sync {
    async fn check(
        &self,
        meta: &ToolMeta,
        args: &Value,
        ctx: &ToolContext<'_>,
    ) -> Result<(), String>;
}

/// Default Phase 1 gate that permits all registered tools (relying on
/// `internet_enabled` and `sandbox/paths.rs` boundaries).
pub struct AllowAllGate;

#[async_trait]
impl PermissionGate for AllowAllGate {
    async fn check(
        &self,
        _meta: &ToolMeta,
        _args: &Value,
        _ctx: &ToolContext<'_>,
    ) -> Result<(), String> {
        Ok(())
    }
}

/// Lifecycle hook executed before and after a tool runs.
#[async_trait]
pub trait ToolHook: Send + Sync {
    async fn before_tool(
        &self,
        _meta: &ToolMeta,
        _args: &Value,
        _ctx: &ToolContext<'_>,
    ) -> Result<(), String> {
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

/// Truncates oversized tool outputs (on a UTF-8 char boundary) to protect
/// local model context windows.
pub struct TruncateOutputHook {
    pub max_chars: usize,
}

impl Default for TruncateOutputHook {
    fn default() -> Self {
        Self { max_chars: 8_000 }
    }
}

#[async_trait]
impl ToolHook for TruncateOutputHook {
    async fn after_tool(
        &self,
        _meta: &ToolMeta,
        _args: &Value,
        result: &mut ToolResult,
        _ctx: &ToolContext<'_>,
    ) {
        if result.error.is_none() && result.output.chars().count() > self.max_chars {
            let clipped: String = result.output.chars().take(self.max_chars).collect();
            result.output = format!("{clipped}… [truncated]");
        } else if let Some(err) = &result.error {
            if err.chars().count() > self.max_chars {
                let clipped: String = err.chars().take(self.max_chars).collect();
                result.error = Some(format!("{clipped}… [truncated]"));
            }
        }
    }
}

/// Logs tool invocations and outcomes for observability.
pub struct AuditLogHook;

#[async_trait]
impl ToolHook for AuditLogHook {
    async fn before_tool(
        &self,
        meta: &ToolMeta,
        _args: &Value,
        _ctx: &ToolContext<'_>,
    ) -> Result<(), String> {
        log::debug!("[harness] invoking tool '{}' ({:?})", meta.id, meta.action_kind);
        Ok(())
    }

    async fn after_tool(
        &self,
        meta: &ToolMeta,
        _args: &Value,
        result: &mut ToolResult,
        _ctx: &ToolContext<'_>,
    ) {
        if let Some(err) = &result.error {
            log::warn!("[harness] tool '{}' failed: {}", meta.id, err);
        } else {
            log::debug!(
                "[harness] tool '{}' succeeded ({} bytes)",
                meta.id,
                result.output.len()
            );
        }
    }
}

/// Orchestrates the 5-stage tool execution lifecycle.
pub struct ToolPipeline {
    registry: ToolRegistry,
    permission_gate: Arc<dyn PermissionGate>,
    hooks: Vec<Arc<dyn ToolHook>>,
}

impl ToolPipeline {
    pub fn new(registry: ToolRegistry) -> Self {
        Self {
            registry,
            permission_gate: Arc::new(AllowAllGate),
            hooks: Vec::new(),
        }
    }

    /// Standard pipeline used by the chat agent: built-in tools, default
    /// output truncation (8,000 chars), and audit logging.
    pub fn standard(registry: ToolRegistry) -> Self {
        Self::new(registry)
            .with_hook(Arc::new(TruncateOutputHook::default()))
            .with_hook(Arc::new(AuditLogHook))
    }

    pub fn with_permission_gate(mut self, gate: Arc<dyn PermissionGate>) -> Self {
        self.permission_gate = gate;
        self
    }

    pub fn with_hook(mut self, hook: Arc<dyn ToolHook>) -> Self {
        self.hooks.push(hook);
        self
    }

    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }

    /// Executes a tool call through all 5 pipeline stages.
    pub async fn execute(&self, name: &str, args: &Value, ctx: &ToolContext<'_>) -> ToolResult {
        // Stage 1: Registry lookup & network gate
        let Some(tool) = self.registry.get(name) else {
            return ToolResult::err(name, format!("Unknown tool: {name}"));
        };
        let meta = tool.meta();
        if meta.requires_internet && !ctx.internet_enabled {
            return ToolResult::err(
                name,
                format!("Tool '{name}' is unavailable because internet access is disabled."),
            );
        }

        // Stage 2: JSON Schema argument validation
        if let Err(reason) = validate_args(&meta.parameters_schema, args) {
            return ToolResult::err(name, format!("Invalid arguments for '{name}': {reason}"));
        }

        // Stage 3: Permission gate
        if let Err(reason) = self.permission_gate.check(&meta, args, ctx).await {
            return ToolResult::err(name, reason);
        }

        // Stage 4: Before hooks
        for hook in &self.hooks {
            if let Err(reason) = hook.before_tool(&meta, args, ctx).await {
                return ToolResult::err(name, reason);
            }
        }

        if ctx.is_cancelled() {
            return ToolResult::err(name, "Cancelled by user.");
        }

        // Stage 5: Execute + After hooks
        let mut result = tool.execute(args, ctx).await;
        for hook in &self.hooks {
            hook.after_tool(&meta, args, &mut result, ctx).await;
        }
        result
    }
}

/// Validates `args` against a JSON Schema object (`required` + `properties.*.type`).
pub fn validate_args(schema: &Value, args: &Value) -> Result<(), String> {
    let Some(obj) = args.as_object() else {
        return Err("arguments must be a JSON object".to_string());
    };

    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for req in required {
            if let Some(key) = req.as_str() {
                match obj.get(key) {
                    None | Some(Value::Null) => {
                        return Err(format!("missing required parameter '{key}'"));
                    }
                    _ => {}
                }
            }
        }
    }

    if let Some(props) = schema.get("properties").and_then(Value::as_object) {
        for (key, val) in obj {
            if val.is_null() {
                continue;
            }
            if let Some(expected_type) = props
                .get(key)
                .and_then(|p| p.get("type"))
                .and_then(Value::as_str)
            {
                let valid = match expected_type {
                    "string" => val.is_string(),
                    "number" => val.is_number(),
                    "integer" => val.is_i64() || val.is_u64(),
                    "boolean" => val.is_boolean(),
                    "array" => val.is_array(),
                    "object" => val.is_object(),
                    _ => true,
                };
                if !valid {
                    return Err(format!(
                        "parameter '{key}' must be of type '{expected_type}'"
                    ));
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;

    use super::*;

    fn test_ctx(internet_enabled: bool) -> ToolContext<'static> {
        ToolContext {
            project_root: Path::new("."),
            personal_space: Path::new("."),
            internet_enabled,
            should_stop: &|| false,
            serving: None,
            model_label: None,
            sampling: None,
            depth: 0,
        }
    }

    #[tokio::test]
    async fn unknown_tool_returns_error() {
        let pipeline = ToolPipeline::standard(ToolRegistry::with_builtins());
        let res = pipeline
            .execute("nonexistent_tool", &json!({}), &test_ctx(false))
            .await;
        assert!(res.error.as_deref().unwrap().contains("Unknown tool"));
    }

    #[tokio::test]
    async fn internet_tool_blocked_when_disabled() {
        let pipeline = ToolPipeline::standard(ToolRegistry::with_builtins());
        let res = pipeline
            .execute("web_search", &json!({"query": "rust"}), &test_ctx(false))
            .await;
        assert!(res
            .error
            .as_deref()
            .unwrap()
            .contains("internet access is disabled"));
    }

    #[tokio::test]
    async fn schema_validation_rejects_missing_and_wrong_type() {
        let pipeline = ToolPipeline::standard(ToolRegistry::with_builtins());
        let missing = pipeline
            .execute("read_file", &json!({}), &test_ctx(false))
            .await;
        assert!(missing
            .error
            .as_deref()
            .unwrap()
            .contains("missing required parameter 'path'"));

        let wrong_type = pipeline
            .execute("read_file", &json!({"path": 123}), &test_ctx(false))
            .await;
        assert!(wrong_type
            .error
            .as_deref()
            .unwrap()
            .contains("must be of type 'string'"));
    }

    struct DenyGate;

    #[async_trait]
    impl PermissionGate for DenyGate {
        async fn check(
            &self,
            meta: &ToolMeta,
            _args: &Value,
            _ctx: &ToolContext<'_>,
        ) -> Result<(), String> {
            Err(format!("Denied by policy: {}", meta.id))
        }
    }

    #[tokio::test]
    async fn permission_gate_blocks_execution() {
        let pipeline = ToolPipeline::new(ToolRegistry::with_builtins())
            .with_permission_gate(Arc::new(DenyGate));
        let res = pipeline
            .execute("list_events", &json!({}), &test_ctx(false))
            .await;
        assert_eq!(res.error.as_deref(), Some("Denied by policy: list_events"));
    }

    #[tokio::test]
    async fn truncate_hook_handles_utf8_boundaries() {
        let hook = TruncateOutputHook { max_chars: 5 };
        let mut res = ToolResult::ok("read_file", "héllo world 🚀");
        let meta = ToolRegistry::with_builtins()
            .get("read_file")
            .unwrap()
            .meta();
        hook.after_tool(&meta, &json!({}), &mut res, &test_ctx(false))
            .await;
        assert_eq!(res.output, "héllo… [truncated]");
    }
}
