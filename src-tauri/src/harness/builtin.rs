//! Built-in tool definitions wrapping `crate::tools`.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::registry::ToolRegistry;
use super::subagent::DelegateTaskTool;
use super::tool::{ToolContext, ToolDefinition, ToolMeta};
use crate::sandbox::confirm::ActionKind;
use crate::tools::{self, ToolResult};

pub fn register_builtins(registry: &mut ToolRegistry) {
    registry.register(Arc::new(ReadFileTool));
    registry.register(Arc::new(ListFilesTool));
    registry.register(Arc::new(WriteFileTool));
    registry.register(Arc::new(AppendFileTool));
    registry.register(Arc::new(SearchLocalTool));
    registry.register(Arc::new(SearchDiaryTool));
    registry.register(Arc::new(ListEventsTool));
    registry.register(Arc::new(DelegateTaskTool));
    registry.register(Arc::new(WebSearchTool));
    registry.register(Arc::new(WebFetchTool));
}

struct ReadFileTool;

#[async_trait]
impl ToolDefinition for ReadFileTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            id: "read_file",
            description: "Reads one file of the project and returns its text. Pass `path` as a plain path relative to the project root, e.g. \"data/notes.md\" — never with a leading @.",
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to the project root, without any @ or quotes — e.g. \"outputs/summary/report.md\""
                    }
                },
                "required": ["path"]
            }),
            action_kind: ActionKind::Read,
            requires_internet: false,
        }
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext<'_>) -> ToolResult {
        let path = args["path"].as_str().unwrap_or("");
        tools::files::read_file(ctx.project_root, path)
    }
}

struct ListFilesTool;

#[async_trait]
impl ToolDefinition for ListFilesTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            id: "list_files",
            description: "Lists the files of a folder of the project, one per line, so an empty subdir lists the project root itself. Pass `subdir` as a plain relative folder name like \"outputs\", never with a leading @.",
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "subdir": {
                        "type": "string",
                        "description": "Folder relative to the project root, without any @ — e.g. \"outputs/summary\"; empty lists the root"
                    }
                }
            }),
            action_kind: ActionKind::Read,
            requires_internet: false,
        }
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext<'_>) -> ToolResult {
        let subdir = args["subdir"].as_str().unwrap_or("");
        tools::files::list_files(ctx.project_root, subdir)
    }
}

struct WriteFileTool;

#[async_trait]
impl ToolDefinition for WriteFileTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            id: "write_file",
            description: "Creates or overwrites one file of the project. If the file already exists at the given path it is written exactly there (e.g. \"diary/2026-11-07.md\"); brand-new files are saved inside the outputs/ folder (\"file.txt\" writes outputs/file.txt). Pass `path` relative to the project root — never with a leading @. Only use it when the user asked for something to be written.",
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Target file path relative to the project root, without any @ or quotes — e.g. \"file.txt\""
                    },
                    "content": {
                        "type": "string",
                        "description": "Full text to write"
                    }
                },
                "required": ["path", "content"]
            }),
            action_kind: ActionKind::Modify,
            requires_internet: false,
        }
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext<'_>) -> ToolResult {
        let path = args["path"].as_str().unwrap_or("");
        let content = args["content"].as_str().unwrap_or("");
        tools::files::write_file(ctx.project_root, path, content)
    }
}

struct AppendFileTool;

#[async_trait]
impl ToolDefinition for AppendFileTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            id: "append_file",
            description: "Appends text to the end of an existing file (creates it under outputs/ if it does not exist yet). A file that already exists at the given path is appended exactly there (e.g. \"diary/2026-11-07.md\" — no copy under outputs/). Use this when the user asks to add or append text to a file.",
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Target file path relative to the project root, without any @ or quotes — e.g. \"file.txt\""
                    },
                    "content": {
                        "type": "string",
                        "description": "Text to append to the end of the file"
                    }
                },
                "required": ["path", "content"]
            }),
            action_kind: ActionKind::Modify,
            requires_internet: false,
        }
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext<'_>) -> ToolResult {
        let path = args["path"].as_str().unwrap_or("");
        let content = args["content"].as_str().unwrap_or("");
        tools::files::append_file(ctx.project_root, path, content)
    }
}

struct SearchLocalTool;

#[async_trait]
impl ToolDefinition for SearchLocalTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            id: "search_local",
            description: "Searches the project's indexed documents and returns matching passages.",
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "What to look for"
                    }
                },
                "required": ["query"]
            }),
            action_kind: ActionKind::Read,
            requires_internet: false,
        }
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext<'_>) -> ToolResult {
        let query = args["query"].as_str().unwrap_or("");
        tools::search_local::search_local(ctx.project_root, query)
    }
}

struct SearchDiaryTool;

#[async_trait]
impl ToolDefinition for SearchDiaryTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            id: "search_diary",
            description: "Searches the user's personal diary pages and returns matching snippets.",
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "What to look for"
                    }
                },
                "required": ["query"]
            }),
            action_kind: ActionKind::Read,
            requires_internet: false,
        }
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext<'_>) -> ToolResult {
        let query = args["query"].as_str().unwrap_or("");
        tools::diary::search_diary(ctx.personal_space, query)
    }
}

struct ListEventsTool;

#[async_trait]
impl ToolDefinition for ListEventsTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            id: "list_events",
            description: "Lists the user's upcoming calendar events.",
            parameters_schema: json!({
                "type": "object",
                "properties": {}
            }),
            action_kind: ActionKind::Read,
            requires_internet: false,
        }
    }

    async fn execute(&self, _args: &Value, ctx: &ToolContext<'_>) -> ToolResult {
        tools::calendar::list_events_tool(ctx.personal_space)
    }
}

struct WebSearchTool;

#[async_trait]
impl ToolDefinition for WebSearchTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            id: "web_search",
            description: "Searches the public web; use when the project's own sources cannot answer the question.",
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Web search query"
                    }
                },
                "required": ["query"]
            }),
            action_kind: ActionKind::Network,
            requires_internet: true,
        }
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext<'_>) -> ToolResult {
        let query = args["query"].as_str().unwrap_or("");
        tools::web_search::search(ctx.project_root, query, ctx.internet_enabled).await
    }
}

struct WebFetchTool;

#[async_trait]
impl ToolDefinition for WebFetchTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            id: "web_fetch",
            description: "Reads one web page and returns its text.",
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The page's absolute URL"
                    }
                },
                "required": ["url"]
            }),
            action_kind: ActionKind::Network,
            requires_internet: true,
        }
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext<'_>) -> ToolResult {
        let url = args["url"].as_str().unwrap_or("");
        tools::web_fetch::fetch(ctx.project_root, url, ctx.internet_enabled).await
    }
}
