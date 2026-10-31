//! The chat agent: a tool-using loop that the project chat runs when it is
//! switched to agent mode.
//!
//! The LLM is the local llama-server itself, spoken to over its
//! OpenAI-compatible endpoint, so no new model, key or network path comes in:
//! the endpoint is the localhost port with the per-session API key the
//! supervisor already issues. Tool calling is native — llama-server receives
//! the OpenAI `tools` array and asks for `tool_calls` — instead of a text
//! protocol, which works with every chat template llama.cpp ships.
//!
//! The loop talks to the server through [`client`] rather than a third-party
//! agent crate, because the client already understands llama-server's
//! `reasoning_content` field. That keeps the model's thinking visible in agent
//! mode, exactly as it is in plain chat.
//!
//! The available tools wrap the app's own sandboxed helpers (`crate::tools`):
//! every path stays inside the project folder, and the web pair refuses to run
//! unless the internet toggle is on. Tool results go back to the model as
//! tool messages; the user sees the answer stream live and each tool framed
//! with a short note. Every piece the user saw is part of the returned
//! content, so the streamed text and the saved turn always agree.

use std::path::Path;

use serde_json::{json, Value};

use crate::error::AppResult;
use crate::runtime::client::{self, SamplingParams, Timings, Usage};
use crate::runtime::Serving;
use crate::tools;

/// Tool notes and answers are clipped, so one tool call cannot flood a line
/// of the transcript.
const MAX_NOTE_CHARS: usize = 80;

/// What one agent turn produced. `content` already contains the tool notes.
pub struct AgentTurn {
    pub content: String,
    /// The model's thinking across the turn, empty when it did not think.
    pub reasoning: String,
    pub tools_used: Vec<String>,
    /// Tokens spent across every model call of the turn.
    pub usage: Usage,
    /// Generation speed across every model call of the turn.
    pub timings: Timings,
    /// Prompt plus completion of the last call: what the context held.
    pub context_tokens: u32,
}

/// One replayed turn of the saved conversation.
pub struct HistoryTurn {
    pub assistant: bool,
    pub content: String,
}

/// The chat system prompt of agent mode: the project's own prompt plus the
/// ground rules. Tool calling is native, so this only describes when to act.
pub fn instructions(base: &str, internet_enabled: bool, has_rag_context: bool) -> String {
    let mut text = String::from(base.trim());
    text.push_str(
        "\n\nAGENT RULES: You are in agent mode on this private, local folder. \
Before answering, you may act with the offered tools: read files, list folders \
or search the project's indexed sources when the answer needs them, and use \
the diary and calendar tools when the question touches the user's days. \
write_file only when the user asked for something to be written. Cite files by \
their relative path. Answer in plain prose; keep tool calls to one at a time.\n\n\
IMPORTANT: For greetings, simple questions, and casual conversation, do NOT use \
any tools. Answer directly without searching, reading files, or calling any \
tools. Only use tools when the question genuinely requires accessing the \
project's files, searching the web, or checking your diary/calendar.",
    );

    if has_rag_context {
        text.push_str(
            "\n\nRelevant context from the project's indexed sources has been \
prepended to this conversation. Use that information first before calling \
search_local. Only call search_local if you need additional information not \
covered by the provided context."
        );
    }

    text.push_str(if internet_enabled {
        " The internet toggle is on, so web_search and web_fetch are available \
when the folders cannot answer the question."
    } else {
        " The internet toggle is off, so the web tools are unavailable; say so \
rather than guessing at the web."
    });

    text
}

/// The OpenAI `tools` array the model is offered. The names and arguments must
/// stay in step with [`crate::tools::call`], which dispatches them.
fn tool_specs(internet_enabled: bool) -> Vec<Value> {
    let mut tools = vec![
        json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Reads one file of the project and returns its text. Paths are relative to the project root, e.g. \"data/notes.md\".",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path relative to the project root" }
                    },
                    "required": ["path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "list_files",
                "description": "Lists the files of a folder of the project, one per line, so an empty subdir lists the project root itself.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "subdir": { "type": "string", "description": "Folder relative to the project root; empty lists the root" }
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "write_file",
                "description": "Creates or overwrites one file of the project. Only use it when the user asked for something to be written.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path relative to the project root" },
                        "content": { "type": "string", "description": "Full text to write" }
                    },
                    "required": ["path", "content"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "search_local",
                "description": "Searches the project's indexed sources and returns the best matching passages with their file names.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "What to look for" }
                    },
                    "required": ["query"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "search_diary",
                "description": "Searches the user's personal diary pages and returns matching snippets.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "What to look for" }
                    },
                    "required": ["query"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "list_events",
                "description": "Lists the user's upcoming calendar events.",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
    ];

    if internet_enabled {
        tools.push(json!({
            "type": "function",
            "function": {
                "name": "web_search",
                "description": "Searches the public web; use when the project's own sources cannot answer the question.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Web search query" }
                    },
                    "required": ["query"]
                }
            }
        }));
        tools.push(json!({
            "type": "function",
            "function": {
                "name": "web_fetch",
                "description": "Reads one web page and returns its text.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "The page's absolute URL" }
                    },
                    "required": ["url"]
                }
            }
        }));
    }

    tools
}

/// The chat agent turn. Runs as long as the model keeps calling tools, bound
/// by `max_tool_steps` (0 falls back to the app default).
///
/// `deliver` receives exactly the text the user should see, in order — streamed
/// answer text plus one short note per tool call. `deliver_reasoning` receives
/// the model's thinking deltas so the transcript can show them.
pub async fn run(
    serving: &Serving,
    model_label: &str,
    system: String,
    history: Vec<HistoryTurn>,
    user_prompt: String,
    project_root: &Path,
    personal_space: &Path,
    internet_enabled: bool,
    sampling: SamplingParams,
    max_tool_steps: u32,
    mut deliver: impl FnMut(&str),
    mut deliver_reasoning: impl FnMut(&str),
) -> AppResult<AgentTurn> {
    let mut messages: Vec<Value> = Vec::new();
    messages.push(json!({ "role": "system", "content": system }));
    for turn in &history {
        messages.push(json!({
            "role": if turn.assistant { "assistant" } else { "user" },
            "content": turn.content,
        }));
    }
    messages.push(json!({ "role": "user", "content": user_prompt }));

    let tools = Value::Array(tool_specs(internet_enabled));
    let max_steps = if max_tool_steps == 0 { 6 } else { max_tool_steps };

    let mut content = String::new();
    let mut reasoning = String::new();
    let mut tools_used: Vec<String> = Vec::new();
    let mut usage = Usage::default();
    let mut timings = Timings::default();
    let mut context_tokens = 0u32;
    let mut step = 0u32;

    loop {
        let turn = client::complete_stream_with_tools(
            serving,
            model_label,
            &messages,
            &tools,
            sampling,
            |delta| {
                deliver(delta);
                content.push_str(delta);
            },
            |delta| {
                deliver_reasoning(delta);
                reasoning.push_str(delta);
            },
        )
        .await?;

        // Every model call counts towards the turn; the last one's prompt plus
        // completion is how much of the context window is now in use.
        if let Some(counts) = turn.usage {
            usage.add(counts);
            context_tokens = counts.prompt_tokens + counts.completion_tokens;
        }
        if let Some(speed) = turn.timings {
            timings.add(speed);
        }

        // No tool calls: the streamed text is the answer, and the turn is done.
        if turn.tool_calls.is_empty() {
            break;
        }

        // Replay the assistant's preamble and the calls it asked for, so the
        // model sees its own request before the results.
        let calls_json: Vec<Value> = turn
            .tool_calls
            .iter()
            .map(|call| {
                json!({
                    "id": call.id,
                    "type": "function",
                    "function": { "name": call.name, "arguments": call.arguments }
                })
            })
            .collect();
        messages.push(json!({
            "role": "assistant",
            "content": turn.content,
            "tool_calls": calls_json,
        }));

        for call in &turn.tool_calls {
            if !tools_used.contains(&call.name) {
                tools_used.push(call.name.clone());
            }
            let note = format!("> 🔧 `{}` — {}\n\n", call.name, short_input(&call.arguments));
            deliver(&note);
            content.push_str(&note);

            let args = serde_json::from_str::<Value>(&call.arguments)
                .ok()
                .filter(Value::is_object)
                .unwrap_or_else(|| json!({}));
            let result =
                tools::call(&call.name, &args, project_root, personal_space, internet_enabled).await;
            messages.push(json!({
                "role": "tool",
                "tool_call_id": call.id,
                "content": result.content(),
            }));
        }

        step += 1;
        if step >= max_steps {
            break;
        }
    }

    Ok(AgentTurn {
        content,
        reasoning,
        tools_used,
        usage,
        timings,
        context_tokens,
    })
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
        let text = instructions("You are localAIse for “Budget”.", false, false);
        assert!(text.starts_with("You are localAIse"));
        assert!(text.contains("AGENT RULES"));
        assert!(text.contains("do NOT use any tools"));
        assert!(text.contains("internet toggle is off"));
        assert!(!text.contains("web tools are available when the folders"));
        let with_web = instructions("base", true, false);
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
        let names: Vec<String> = tool_specs(false)
            .iter()
            .map(|tool| tool["function"]["name"].as_str().unwrap().to_string())
            .collect();
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

        let names: Vec<String> = tool_specs(true)
            .iter()
            .map(|tool| tool["function"]["name"].as_str().unwrap().to_string())
            .collect();
        assert!(names.contains(&"web_search".to_string()));
        assert!(names.contains(&"web_fetch".to_string()));
    }
}
