//! The chat agent: a LangChain tools agent that the project chat runs when it
//! is switched to agent mode.
//!
//! The LLM is the local llama-server itself, spoken to over its
//! OpenAI-compatible endpoint, so no new model, key or network path comes in:
//! the endpoint is the localhost port with the per-session API key the
//! supervisor already issues. Tool calling is native — llama-server receives
//! the OpenAI `tools` array and asks for `tool_calls` — instead of a text
//! protocol, which works with every chat template llama.cpp ships.
//!
//! The available tools wrap the app's own sandboxed helpers (`crate::tools`):
//! every path stays inside the project folder, and the web pair refuses to run
//! unless the internet toggle is on. Tool results go back to the model as
//! tool messages; the user sees the answer stream live and each tool framed
//! with a short note. Every piece the user saw is part of the returned
//! content, so the streamed text and the saved turn always agree.

use std::path::Path;
use std::sync::Arc;

use futures::StreamExt;
use langchain_rust::agent::{AgentExecutor, AgentStreamEvent, OpenAiToolAgentBuilder};
use langchain_rust::chain::options::ChainCallOptions;
use langchain_rust::llm::{openai::OpenAI, OpenAIConfig};
use langchain_rust::memory::SimpleMemory;
use langchain_rust::prompt_args;
use langchain_rust::schemas::memory::BaseMemory;
use langchain_rust::tools::{FunctionTool, Tool};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::runtime::Serving;
use crate::tools;

/// How many tool rounds one chat turn may run before the agent has to answer.
const MAX_TOOL_STEPS: i32 = 6;

/// Tool notes and answers are clipped, so one tool call cannot flood a line
/// of the transcript.
const MAX_NOTE_CHARS: usize = 80;

/// What one agent turn produced. `content` already contains the tool notes.
pub struct AgentTurn {
    pub content: String,
    pub tools_used: Vec<String>,
}

/// One replayed turn of the saved conversation.
pub struct HistoryTurn {
    pub assistant: bool,
    pub content: String,
}

/// The chat system prompt of agent mode: the project's own prompt plus the
/// ground rules. Tool calling is native, so this only describes when to act.
pub fn instructions(base: &str, internet_enabled: bool) -> String {
    let mut text = String::from(base.trim());
    text.push_str(
        "\n\nAGENT RULES: You are in agent mode on this private, local folder. \
Before answering, you may act with the offered tools: read files, list folders \
or search the project's indexed sources when the answer needs them, and use \
the diary and calendar tools when the question touches the user's days. \
write_file only when the user asked for something to be written. Cite files by \
their relative path. Answer in plain prose; keep tool calls to one at a time.",
    );

    text.push_str(if internet_enabled {
        " The internet toggle is on, so web_search and web_fetch are available \
when the folders cannot answer the question."
    } else {
        " The internet toggle is off, so the web tools are unavailable; say so \
rather than guessing at the web."
    });

    text
}

/// Typed arguments the model sends to each tool; serde fills the defaults.
#[derive(Deserialize)]
struct PathInput {
    path: String,
}

#[derive(Deserialize)]
struct QueryInput {
    query: String,
}

#[derive(Deserialize)]
struct SubdirInput {
    #[serde(default)]
    subdir: String,
}

#[derive(Deserialize)]
struct WriteInput {
    path: String,
    content: String,
}

#[derive(Deserialize)]
struct UrlInput {
    url: String,
}

#[derive(Deserialize)]
struct NoArgs {}

/// Builds the OpenAI tools agent over the local llama-server with the app's
/// sandboxed tools wrapped as langchain tools.
fn build_tools(
    project_root: &Path,
    personal_space: &Path,
    internet_enabled: bool,
) -> Vec<Arc<dyn Tool>> {
    let root = project_root.to_path_buf();

    let read_file = FunctionTool::new(
        "read_file",
        "Reads one file of the project and returns its text. Paths are relative \
to the project root, e.g. \"data/notes.md\".",
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File path relative to the project root" }
            },
            "required": ["path"]
        }),
        move |input: PathInput| {
            let root = root.clone();
            async move { Ok::<_, String>(tools::files::read_file(&root, &input.path).content().to_string()) }
        },
    );

    let root = project_root.to_path_buf();
    let list_files = FunctionTool::new(
        "list_files",
        "Lists the files of a folder of the project, one per line, so an empty \
subdir lists the project root itself.",
        json!({
            "type": "object",
            "properties": {
                "subdir": { "type": "string", "description": "Folder relative to the project root; empty lists the root" }
            }
        }),
        move |input: SubdirInput| {
            let root = root.clone();
            async move { Ok::<_, String>(tools::files::list_files(&root, &input.subdir).content().to_string()) }
        },
    );

    let root = project_root.to_path_buf();
    let write_file = FunctionTool::new(
        "write_file",
        "Creates or overwrites one file of the project. Only use it when the \
user asked for something to be written.",
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File path relative to the project root" },
                "content": { "type": "string", "description": "Full text to write" }
            },
            "required": ["path", "content"]
        }),
        move |input: WriteInput| {
            let root = root.clone();
            async move {
                Ok::<_, String>(
                    tools::files::write_file(&root, &input.path, &input.content)
                        .content()
                        .to_string(),
                )
            }
        },
    );

    let root = project_root.to_path_buf();
    let search_local = FunctionTool::new(
        "search_local",
        "Searches the project's indexed sources and returns the best matching \
passages with their file names.",
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "What to look for" }
            },
            "required": ["query"]
        }),
        move |input: QueryInput| {
            let root = root.clone();
            async move { Ok::<_, String>(tools::search_local::search_local(&root, &input.query).content().to_string()) }
        },
    );

    let personal = personal_space.to_path_buf();
    let search_diary = FunctionTool::new(
        "search_diary",
        "Searches the user's personal diary pages and returns matching snippets.",
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "What to look for" }
            },
            "required": ["query"]
        }),
        move |input: QueryInput| {
            let personal = personal.clone();
            async move { Ok::<_, String>(tools::diary::search_diary(&personal, &input.query).content().to_string()) }
        },
    );

    let personal = personal_space.to_path_buf();
    let list_events = FunctionTool::new(
        "list_events",
        "Lists the user's upcoming calendar events.",
        json!({ "type": "object", "properties": {} }),
        move |_input: NoArgs| {
            let personal = personal.clone();
            async move {
                Ok::<_, String>(tools::calendar::list_events_tool(&personal).content().to_string())
            }
        },
    );

    let mut list: Vec<Arc<dyn Tool>> = vec![
        Arc::new(read_file),
        Arc::new(list_files),
        Arc::new(write_file),
        Arc::new(search_local),
        Arc::new(search_diary),
        Arc::new(list_events),
    ];

    if internet_enabled {
        let root = project_root.to_path_buf();
        let web_search = FunctionTool::new(
            "web_search",
            "Searches the public web; use when the project's own sources cannot \
answer the question.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Web search query" }
                },
                "required": ["query"]
            }),
            move |input: QueryInput| {
                let root = root.clone();
                async move {
                    Ok::<_, String>(
                        tools::web_search::search(&root, &input.query, true)
                            .await
                            .content()
                            .to_string(),
                    )
                }
            },
        );

        let root = project_root.to_path_buf();
        let web_fetch = FunctionTool::new(
            "web_fetch",
            "Reads one web page and returns its text.",
            json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "The page's absolute URL" }
                },
                "required": ["url"]
            }),
            move |input: UrlInput| {
                let root = root.clone();
                async move {
                    Ok::<_, String>(
                        tools::web_fetch::fetch(&root, &input.url, true)
                            .await
                            .content()
                            .to_string(),
                    )
                }
            },
        );

        list.push(Arc::new(web_search));
        list.push(Arc::new(web_fetch));
    }

    list
}

/// The chat agent turn. Runs as long as the model keeps calling tools, bound
/// by `MAX_TOOL_STEPS`.
///
/// `deliver` receives exactly the text the user should see, in order — streamed
/// answer text plus one short note per tool call.
pub async fn run(
    serving: &Serving,
    model_label: &str,
    system: String,
    history: Vec<HistoryTurn>,
    user_prompt: String,
    project_root: &Path,
    personal_space: &Path,
    internet_enabled: bool,
    mut deliver: impl FnMut(&str),
) -> AppResult<AgentTurn> {
    let mut memory = SimpleMemory::new();
    for turn in &history {
        let result = if turn.assistant {
            memory.add_ai_message(&turn.content).await
        } else {
            memory.add_user_message(&turn.content).await
        };
        result.map_err(|error| {
            AppError::msg(format!("could not feed chat history to the agent: {error}"))
        })?;
    }

    let llm = OpenAI::default()
        .with_config(
            OpenAIConfig::default()
                .with_api_base(&format!("http://127.0.0.1:{}/v1", serving.port))
                .with_api_key(serving.token.as_str()),
        )
        .with_model(model_label);

    let tools = build_tools(project_root, personal_space, internet_enabled);
    let agent = OpenAiToolAgentBuilder::new()
        .prefix(system)
        .tools(&tools)
        // `new()` sends no cap and no temperature; the server's own settings
        // decide the length, the same as plain chat mode does.
        .options(ChainCallOptions::new())
        .build(llm)
        .map_err(|error| AppError::msg(format!("could not start the agent: {error}")))?;

    let executor = AgentExecutor::from_agent(agent)
        .with_max_iterations(MAX_TOOL_STEPS)
        .with_memory(memory.into());

    let mut content = String::new();
    let mut tools_used: Vec<String> = Vec::new();
    let mut finish: Option<String> = None;

    let mut events = executor.stream_events(prompt_args! {
        "input" => user_prompt,
    });
    while let Some(event) = events.next().await {
        match event {
            Ok(AgentStreamEvent::Text(chunk)) => {
                deliver(&chunk);
                content.push_str(&chunk);
            }
            Ok(AgentStreamEvent::ToolStart { tool, input }) => {
                if !tools_used.contains(&tool) {
                    tools_used.push(tool.clone());
                }
                let note = format!("> 🔧 `{tool}` — {}\n\n", short_input(&input));
                deliver(&note);
                content.push_str(&note);
            }
            // The model already sees the tool output; Usage is not shown.
            Ok(AgentStreamEvent::ToolEnd { .. }) | Ok(AgentStreamEvent::Usage(_)) => {}
            Ok(AgentStreamEvent::Finish { output }) => finish = Some(output),
            // The enum is deliberately open to new event kinds; ignore them.
            Err(error) => {
                return Err(AppError::msg(format!("the agent could not finish: {error}")))
            }
            Ok(_) => {}
        }
    }

    // A plan may hold text back when it looks like a tool-call JSON dump and
    // turns out to be the answer; make sure it reaches the saved turn too.
    if let Some(output) = finish {
        let tail = content.trim_end().trim_start_matches('>');
        if tail.is_empty() {
            content = output;
        } else if !tail.ends_with(output.trim()) {
            content.push_str(&output);
        }
    }

    Ok(AgentTurn { content, tools_used })
}

/// The visible summary of one tool call: the first meaningful argument, short.
fn short_input(input: &str) -> String {
    let parsed: Value = serde_json::from_str(input).unwrap_or(Value::Null);
    let argument = ["path", "query", "url", "subdir"]
        .iter()
        .find_map(|key| parsed.get(key).and_then(Value::as_str))
        .or_else(|| Some(input))
        .unwrap_or(input);
    let argument = argument.lines().next().unwrap_or(argument).trim();
    if argument.chars().count() <= MAX_NOTE_CHARS {
        return argument.to_string();
    }
    let clipped: String = argument.chars().take(MAX_NOTE_CHARS).collect();
    format!("{clipped}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_rules_arrive_after_the_project_prompt() {
        let text = instructions("You are localAIse for “Budget”.", false);
        assert!(text.starts_with("You are localAIse"));
        assert!(text.contains("AGENT RULES"));
        assert!(text.contains("internet toggle is off"));
        assert!(!text.contains("web tools are available when the folders"));
        let with_web = instructions("base", true);
        assert!(with_web.contains("web_search and web_fetch are available"));
    }

    #[test]
    fn tool_input_notes_pick_the_meaningful_argument() {
        assert_eq!(
            short_input(r#"{"path": "data/notes.md"}"#),
            "data/notes.md"
        );
        assert_eq!(short_input(r#"{"query": "spending"}"#), "spending");
        assert_eq!(short_input(r#"not json at all"#), "not json at all");
    }

    #[test]
    fn tool_input_notes_stay_short() {
        let long = "a".repeat(300);
        let note = short_input(&format!(r#"{{"path": "{long}"}}"#));
        assert!(note.chars().count() <= MAX_NOTE_CHARS + 1);
        assert!(note.ends_with('…'));
    }

    #[test]
    fn tool_definitions_match_the_dispatcher_names() {
        let tools = build_tools(Path::new("C:/tmp"), Path::new("C:/personal"), false);
        let names: Vec<String> = tools.iter().map(|tool| tool.name()).collect();
        assert_eq!(
            names,
            vec![
                "read_file".to_string(),
                "list_files".to_string(),
                "write_file".to_string(),
                "search_local".to_string(),
                "search_diary".to_string(),
                "list_events".to_string(),
            ]
        );

        let tools = build_tools(Path::new("C:/tmp"), Path::new("C:/personal"), true);
        let names: Vec<String> = tools.iter().map(|tool| tool.name()).collect();
        assert!(names.contains(&"web_search".to_string()));
        assert!(names.contains(&"web_fetch".to_string()));
    }
}
