//! MCP (Model Context Protocol) client bridge for stdio and HTTP servers.
//!
//! Reads optional `<project_root>/.LocalAIse/mcp.json`, discovers tools via
//! JSON-RPC `tools/list`, and wraps each tool as a [`ToolDefinition`].

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use super::registry::ToolRegistry;
use super::tool::{ToolContext, ToolDefinition, ToolMeta};
use crate::sandbox::confirm::ActionKind;
use crate::tools::ToolResult;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub url: Option<String>,
    pub bearer_token: Option<String>,
    #[serde(default)]
    pub requires_internet: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpConfigFile {
    #[serde(default)]
    pub servers: HashMap<String, McpServerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpToolInfo {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_schema")]
    input_schema: Value,
}

fn default_schema() -> Value {
    json!({ "type": "object", "properties": {} })
}

/// Loads `<project_root>/.LocalAIse/mcp.json` and registers discovered tools
/// into `registry`. Failures on individual servers are logged and skipped.
pub async fn register_project_mcp_tools(
    project_root: &Path,
    internet_enabled: bool,
    registry: &mut ToolRegistry,
) {
    let cfg_path = project_root.join(".LocalAIse").join("mcp.json");
    let Ok(raw) = std::fs::read_to_string(cfg_path) else {
        return;
    };
    let Ok(cfg) = serde_json::from_str::<McpConfigFile>(&raw) else {
        log::warn!("[mcp] failed to parse .LocalAIse/mcp.json");
        return;
    };

    for (server_name, server_cfg) in cfg.servers {
        if server_cfg.requires_internet && !internet_enabled {
            log::info!("[mcp] skipping server '{}' (internet disabled)", server_name);
            continue;
        }
        match discover_tools(&server_cfg).await {
            Ok(tools) => {
                for tool in tools {
                    let full_id: &'static str = Box::leak(
                        format!("mcp_{}_{}", sanitize(&server_name), sanitize(&tool.name))
                            .into_boxed_str(),
                    );
                    let desc: &'static str = Box::leak(tool.description.into_boxed_str());
                    registry.register(Arc::new(McpToolDefinition {
                        id: full_id,
                        description: desc,
                        parameters_schema: tool.input_schema,
                        raw_tool_name: tool.name,
                        server: server_cfg.clone(),
                    }));
                }
            }
            Err(err) => {
                log::warn!("[mcp] failed to list tools from '{}': {}", server_name, err);
            }
        }
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

async fn discover_tools(server: &McpServerConfig) -> Result<Vec<McpToolInfo>, String> {
    let list_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });
    let resp = rpc_call(server, &list_req, true).await?;
    let tools_val = resp
        .get("result")
        .and_then(|r| r.get("tools"))
        .cloned()
        .unwrap_or_else(|| json!([]));
    serde_json::from_value::<Vec<McpToolInfo>>(tools_val)
        .map_err(|e| format!("invalid tools/list response: {e}"))
}

async fn rpc_call(
    server: &McpServerConfig,
    payload: &Value,
    send_init: bool,
) -> Result<Value, String> {
    if let Some(url) = &server.url {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|e| e.to_string())?;
        let mut req = client.post(url).json(payload);
        if let Some(token) = &server.bearer_token {
            req = req.bearer_auth(token);
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        let body: Value = resp.json().await.map_err(|e| e.to_string())?;
        if let Some(err) = body.get("error") {
            return Err(err.to_string());
        }
        return Ok(body);
    }

    let Some(cmd) = &server.command else {
        return Err("MCP server config missing both 'command' and 'url'".to_string());
    };
    let server_owned = server.clone();
    let payload_owned = payload.clone();
    let cmd_owned = cmd.clone();

    tokio::task::spawn_blocking(move || {
        let mut child = Command::new(&cmd_owned)
            .args(&server_owned.args)
            .envs(&server_owned.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("failed to spawn '{cmd_owned}': {e}"))?;

        let mut stdin = child.stdin.take().ok_or("missing child stdin")?;
        let stdout = child.stdout.take().ok_or("missing child stdout")?;
        let mut reader = BufReader::new(stdout);

        if send_init {
            let init = json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": { "name": "localaise", "version": "0.1.3" }
                }
            });
            writeln!(stdin, "{init}").map_err(|e| e.to_string())?;
            let mut _init_line = String::new();
            let _ = reader.read_line(&mut _init_line);

            let initialized = json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            });
            writeln!(stdin, "{initialized}").map_err(|e| e.to_string())?;
        }

        writeln!(stdin, "{payload_owned}").map_err(|e| e.to_string())?;
        stdin.flush().map_err(|e| e.to_string())?;
        drop(stdin);

        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
        let _ = child.wait();

        let body: Value = serde_json::from_str(line.trim())
            .map_err(|e| format!("invalid JSON-RPC response: {e}"))?;
        if let Some(err) = body.get("error") {
            return Err(err.to_string());
        }
        Ok(body)
    })
    .await
    .map_err(|e| format!("MCP task join error: {e}"))?
}

struct McpToolDefinition {
    id: &'static str,
    description: &'static str,
    parameters_schema: Value,
    raw_tool_name: String,
    server: McpServerConfig,
}

#[async_trait]
impl ToolDefinition for McpToolDefinition {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            id: self.id,
            description: self.description,
            parameters_schema: self.parameters_schema.clone(),
            action_kind: if self.server.requires_internet {
                ActionKind::Network
            } else {
                ActionKind::Modify
            },
            requires_internet: self.server.requires_internet,
        }
    }

    async fn execute(&self, args: &Value, _ctx: &ToolContext<'_>) -> ToolResult {
        let call_req = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": self.raw_tool_name,
                "arguments": args
            }
        });
        match rpc_call(&self.server, &call_req, true).await {
            Ok(resp) => {
                let text = resp
                    .get("result")
                    .and_then(|r| r.get("content"))
                    .and_then(Value::as_array)
                    .map(|parts| {
                        parts
                            .iter()
                            .filter_map(|p| p.get("text").and_then(Value::as_str))
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| resp["result"].to_string());
                ToolResult::ok(self.id, text)
            }
            Err(err) => ToolResult::err(self.id, format!("MCP call failed: {err}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mcp_config_file() {
        let raw = r#"{
            "servers": {
                "notes": { "command": "npx", "args": ["-y", "notes-mcp"] },
                "webapi": { "url": "https://example.com/mcp", "requiresInternet": true }
            }
        }"#;
        let parsed: McpConfigFile = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.servers.len(), 2);
        assert!(!parsed.servers["notes"].requires_internet);
        assert!(parsed.servers["webapi"].requires_internet);
    }
}
