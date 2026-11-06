//! Chat against the local model.
//!
//! Strictly llama-server: a question is only answered by a server this app
//! started, from a model the user already has on disk. There is no mock answer
//! and no cloud fallback — if the runtime cannot serve, the user sees why.
//! Tokens stream to the UI as the model produces them.
//!
//! Every turn belongs to a session saved as JSON in the project's `chats/`
//! folder, so a conversation survives a restart. A brand-new chat is answered
//! from a completely fresh cache.

use std::fs;
use std::path::Path;

use tauri::{AppHandle, Emitter, State};

use crate::agent::agent_chat;
use crate::activity::log as activity;
use crate::error::{AppError, AppResult};
use crate::ingest::pipeline::ingest_file;
use crate::index::db::IndexDb;
use crate::models::registry as models;
use crate::projects::chats::{self, AgentStep, ChatSession, StoredMessage};
use crate::projects::layout::{ChatMode, ProjectPaths};
use crate::retrieval::citations::Citation;
use crate::retrieval::search::search_chunks;
use crate::runtime::client::{self, SamplingParams};
use crate::util::{hash, time};
use crate::AppState;

// Default constants are defined in ProjectChatSettings::default()

/// A chat turn: the streamed answer plus the citations it was built on.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStreamResult {
    /// The session the turn was saved in. A new chat gets its id here.
    pub session_id: String,
    pub title: String,
    pub content: String,
    /// The model's reasoning trace, empty when it answered without thinking.
    pub reasoning: String,
    /// Ordered reasoning, tool, and answer segments (agent mode only).
    pub steps: Vec<AgentStep>,
    pub citations: Vec<Citation>,
    /// Prompt tokens this turn read, summed over every model call.
    pub prompt_tokens: u32,
    /// Tokens this turn generated, summed over every model call.
    pub completion_tokens: u32,
    /// Prompt plus completion, summed over every model call.
    pub total_tokens: u32,
    /// Prompt plus completion of the last call: how full the context is.
    pub context_tokens: u32,
    /// Generated tokens per second, when the server reported timings.
    pub tokens_per_second: f64,
    /// True when the user stopped the turn before the model finished.
    pub cancelled: bool,
}

/// Threshold for deciding whether to read a file fully or index it for RAG.
/// Files smaller than this are read in full; larger files are indexed.
const ATTACHMENT_FULL_READ_THRESHOLD: u64 = 10_000; // 10 KB

// Maximum characters per chunk when feeding a large attached file to the
// model as consecutive inputs. Roughly 4000 chars ~ 1000 tokens, which keeps
// each chunk safely inside most models' per-message input limits.
const FILE_CHUNK_CHARS: usize = 4000;

/// Splits text into consecutive chunks of at most `max_chars` characters,
/// splitting on whitespace where possible so words are not broken mid-chunk.
fn split_into_chunks(text: &str, max_chars: usize) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut start = 0;
    let char_indices: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    let total = char_indices.len();
    while start < total {
        // Find the farthest char index within max_chars from start.
        let mut end = (start + max_chars).min(total);
        // If we're not at the end of the text, back off to the last whitespace
        // so we don't split a word in half.
        if end < total {
            while end > start {
                let prev_byte = char_indices[end - 1];
                let prev_char = text[prev_byte..].chars().next().unwrap();
                if prev_char.is_ascii_whitespace() {
                    break;
                }
                end -= 1;
            }
            if end == start {
                end = (start + max_chars).min(total);
            }
        }
        let start_byte = char_indices[start];
        let end_byte = if end < total { char_indices[end] } else { text.len() };
        let chunk = text[start_byte..end_byte].to_string();
        if !chunk.trim().is_empty() {
            chunks.push(chunk);
        }
        start = end;
        // Skip leading whitespace of the next chunk.
        while start < total {
            let byte = char_indices[start];
            let ch = text[byte..].chars().next().unwrap();
            if !ch.is_ascii_whitespace() {
                break;
            }
            start += 1;
        }
    }
    chunks
}

/// Extracts readable text from an attached file.
///
/// Uses the format-aware ingestion parser first so PDFs, DOCX, XLSX and other
/// supported documents become plain text; code and other plain files fall back
/// to a direct UTF-8 read. Returns `None` when the file cannot be read at all.
fn read_attached_text(file_path: &Path) -> Option<String> {
    if let Ok(text) = crate::ingest::parsers::parse_file(file_path) {
        return Some(text);
    }
    fs::read_to_string(file_path).ok()
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn chat_send(
    app: AppHandle,
    state: State<'_, AppState>,
    project_id: String,
    // `None` starts a new chat; a saved id continues that conversation.
    session_id: Option<String>,
    prompt: String,
    // Optional file paths attached to this message (relative to project root).
    attached_files: Option<Vec<String>>,
    // Optional sampling parameters; if not provided, defaults are used.
    temperature: Option<f32>,
    top_p: Option<f32>,
    top_k: Option<i32>,
) -> AppResult<ChatStreamResult> {
    let prompt = prompt.trim().to_string();
    if prompt.is_empty() {
        return Err(AppError::msg("Type a question first."));
    }

    // Registers this turn so `chat_stop` can reach its model and tool loop.
    // The guard clears the registration when the command returns or errors.
    let run = crate::agent::cancel::RunGuard::begin(&project_id);

    // Build sampling params from provided values, using defaults when not specified.
    let sampling = SamplingParams {
        temperature: temperature.unwrap_or(0.2).clamp(0.0, 2.0),
        top_p: top_p.map(|v| v.clamp(0.0, 1.0)),
        top_k: top_k.map(|v| v.max(0)),
        max_tokens: None,
        reasoning_effort: None,
        chat_template_kwargs: None,
    };

    let (record, settings) = {
        let registry = state.registry()?;
        let record = registry
            .find(&project_id)
            .cloned()
            .ok_or_else(|| AppError::UnknownProject(project_id.clone()))?;
        (record, registry.settings.clone())
    };

    let model = models::selected(&state.dirs, &settings).ok_or(AppError::NoModelSelected)?;

    // Image is the only supported multimodal modality. The projector is not
    // loaded at server startup — it is only passed when this turn attaches
    // an image, so text-only chats run without --mmproj.
    let needs_projector = attached_files
        .as_ref()
        .is_some_and(|files| {
            models::has_image_attachment(
                &files
                    .iter()
                    .map(|f| record.path.join(f))
                    .collect::<Vec<_>>(),
            )
        });
    let projector_path = models::projector_for(&model);
    let projector = if needs_projector {
        projector_path.as_deref()
    } else {
        None
    };

    // The runtime resolves the device and starts the server. The projector
    // is passed through only when an image needs to be embedded.
    let serving = crate::commands::runtime::ensure(&state, &settings, projector)?;

    // 1. Load or start the session that this turn belongs to.
    let paths = ProjectPaths::new(record.path.clone());
    let chat_dir = paths.chats_dir();
    let fresh = session_id.is_none();
    let mut session = match &session_id {
        Some(id) => chats::load(&chat_dir, id)?,
        None => ChatSession::new(Some(chats::new_session_id(&chat_dir))),
    };
    if session.title.trim().is_empty() {
        session.title = chats::first_line_title(Some(&prompt));
    }

    // Read project-specific limits (fall back to defaults)
    let retrieval_limit = record.chat_settings.retrieval_limit;
    let history_limit = record.chat_settings.history_limit;
    let max_tool_steps = record.chat_settings.max_tool_steps;

    // 2. Process attached files: read small files fully, index large files for RAG.
    // Files that exceed the per-message input limit are split into chunks and fed
    // to the model as consecutive inputs before the actual question.
    let mut attached_context = String::new();
    let mut attached_citations: Vec<Citation> = Vec::new();
    let mut file_chunks: Vec<(String, String)> = Vec::new();
    if let Some(files) = attached_files {
        let project_root = &record.path;
        let index_db = IndexDb::new(project_root);
        
        for file_rel in files.iter() {
            let file_path = project_root.join(file_rel);
            
            // Get file size to decide strategy
            let file_size = fs::metadata(&file_path).map(|m| m.len()).unwrap_or(0);
            
            if file_size <= ATTACHMENT_FULL_READ_THRESHOLD {
                // Small file: read full content and add directly to context
                if let Some(content) = read_attached_text(&file_path) {
                    if !content.trim().is_empty() {
                        attached_context.push_str(&format!(
                            "\n\n--- Attached file: {} ---\n{}",
                            file_rel, content
                        ));
                        attached_citations.push(Citation {
                            index: attached_citations.len() + 1,
                            file_name: file_rel.clone(),
                            page: None,
                            passage: Some(if content.len() > 200 {
                                format!("{}…", &content[..200])
                            } else {
                                content.clone()
                            }),
                        });
                        // If the content itself exceeds the per-message limit,
                        // chunk it as consecutive inputs as well.
                        if content.len() > FILE_CHUNK_CHARS {
                            for chunk in split_into_chunks(&content, FILE_CHUNK_CHARS) {
                                file_chunks.push((file_rel.clone(), chunk));
                            }
                        }
                    }
                }
            } else {
                // Large file: ingest into index for RAG retrieval
                let file_rel_path = Path::new(file_rel);
                match ingest_file(project_root, file_rel_path) {
                    Ok(chunks) => {
                        // Get file modification time for index stamp
                        let file_stamp = fs::metadata(&file_path)
                            .and_then(|m| m.modified())
                            .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs().to_string())
                            .unwrap_or_default();
                        
                        if let Err(e) = index_db.insert_chunks(file_rel, &file_stamp, chunks) {
                            eprintln!("Failed to index attached file {}: {}", file_rel, e);
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to ingest attached file {}: {}", file_rel, e);
                    }
                }
                // Read the full content and chunk it as consecutive inputs so
                // the model sees the whole file even when it is too large for
                // a single message.
                if let Some(content) = read_attached_text(&file_path) {
                    if !content.trim().is_empty() {
                        for chunk in split_into_chunks(&content, FILE_CHUNK_CHARS) {
                            file_chunks.push((file_rel.clone(), chunk));
                        }
                    }
                }
            }
        }
    }

    // 3. Retrieve relevant source context from indexed project sources (includes newly indexed attached files).
    let index_db = IndexDb::new(&record.path);
    let project_index = index_db.load();
    let scored_chunks = search_chunks(&project_index, &prompt, retrieval_limit);
    let mut citations = crate::retrieval::search::format_citations(&scored_chunks);
    // Merge attached file citations (adjust indices to continue after RAG citations)
    for mut c in attached_citations {
        c.index = citations.len() + 1;
        citations.push(c);
    }

    let mut context_block = String::new();
    if !scored_chunks.is_empty() {
        context_block.push_str("\n\nContext from project sources:\n");
        for (i, sc) in scored_chunks.iter().enumerate() {
            context_block.push_str(&format!(
                "[{}] (Source: {}):\n{}\n\n",
                i + 1,
                sc.chunk.file_name,
                sc.chunk.content
            ));
        }
    }
    // Append attached file context (full content for small files)
    if !attached_context.is_empty() {
        context_block.push_str("\n\nContext from attached files:\n");
        context_block.push_str(&attached_context);
    }

    let user_prompt = if context_block.is_empty() {
        prompt.clone()
    } else {
        format!(
            "{prompt}\n{context_block}\nUse the provided sources above to answer and cite them where applicable."
        )
    };

    let system = match record.system_prompt.as_deref().map(str::trim).filter(|text| !text.is_empty()) {
        Some(custom) => custom.to_string(),
        None => system_prompt(&record.name),
    };

    // In agent mode the built-in prompt gains the tool rules; the custom
    // prompt the user set still leads the model's voice and behaviour.
    let system = if record.chat_mode == ChatMode::Agent {
        agent_chat::instructions(&system, record.internet_enabled, !scored_chunks.is_empty())
    } else {
        system
    };

    // 3. Replay the tail of the conversation, then feed attached file chunks
    // as consecutive inputs, and finally ask the new question.
    let mut messages = vec![client::message("system", system.clone())];
    let history_start = session.messages.len().saturating_sub(history_limit);
    for turn in &session.messages[history_start..] {
        let role = if turn.role == "assistant" { "assistant" } else { "user" };
        messages.push(client::message(role, turn.content.clone()));
    }
    // Each chunk of an oversized attached file is sent as its own user message
    // so the model processes the file in consecutive inputs that fit within
    // the per-message context limit.
    for (file_name, chunk) in &file_chunks {
        messages.push(client::message(
            "user",
            format!(
                "--- Attached file: {} (continued) ---\n{}",
                file_name, chunk
            ),
        ));
    }
    messages.push(client::message("user", user_prompt.clone()));

    let serving = match client::wait_until_ready(&serving).await {
        Ok(()) => serving,
        Err(error) => {
            // Check if this was a GPU run. If so, attempt fallback to CPU once.
            let is_gpu_run = state
                .supervisor()
                .ok()
                .and_then(|sup| sup.running_profile().map(|p| p.device.gpu_layers() > 0))
                .unwrap_or(false);

            if is_gpu_run {
                if let Ok(mut supervisor) = state.supervisor() {
                    supervisor.stop();
                    supervisor.set_fallback_notice(Some(
                        "GPU runtime failed health check. Fell back to CPU backend.".to_string(),
                    ));
                }

                // Force a CPU setup and launch
                let cpu_settings = settings.clone();
                let mut cpu_profile = cpu_settings.performance.clone();
                cpu_profile.device = crate::runtime::profile::ComputeDevice::Cpu;

                let cpu_server = crate::runtime::backend::resolve_server(&state.dirs, &cpu_settings)
                    .ok_or(AppError::msg("no CPU-capable llama-server found"))?;

                let cpu_serving = {
                    let mut supervisor = state.supervisor()?;
                    supervisor.ensure(&crate::runtime::supervisor::LaunchRequest {
                        model: model.as_path(),
                        projector,
                        server: cpu_server.as_path(),
                        profile: &cpu_profile,
                    })?
                };

                if let Err(retry_err) = client::wait_until_ready(&cpu_serving).await {
                    if let Ok(mut supervisor) = state.supervisor() {
                        supervisor.mark_failed(retry_err.to_string());
                    }
                    return Err(retry_err);
                }

                cpu_serving
            } else {
                if let Ok(mut supervisor) = state.supervisor() {
                    supervisor.mark_failed(error.to_string());
                }
                return Err(error);
            }
        }
    };

    let model_label = models::label(&model);
    let emitter = app.clone();
    let emit = |reasoning: bool, call: Option<u32>, delta: &str| {
        let mut payload = serde_json::json!({
            "projectId": project_id,
            "delta": delta,
            "reasoning": reasoning,
        });
        // Agent turns tag each delta with its model call so the UI can keep
        // one thinking block per call. Chat mode has a single call.
        if let Some(call) = call {
            payload["call"] = serde_json::json!(call);
        }
        emitter.emit("chat://token", payload).ok();
    };
    let emit_agent_event = |event: &agent_chat::AgentEvent| {
        emitter
            .emit(
                "chat://agent",
                serde_json::json!({
                    "projectId": project_id,
                    "event": event,
                }),
            )
            .ok();
    };

    // Agent mode runs the tool loop over the same local server; chat mode is
    // one streamed call. Both report the tokens and the speed they spent.
    let (mut content, reasoning, steps, prompt_tokens, completion_tokens, total_tokens, context_tokens, tokens_per_second, cancelled) =
        if record.chat_mode == ChatMode::Agent {
            let personal_space = {
                let registry = state.registry()?;
                registry.personal_space_or_default(&state.dirs)
            };
            let history = session.messages[history_start..]
                .iter()
                .map(|turn| agent_chat::HistoryTurn {
                    assistant: turn.role == "assistant",
                    content: turn.content.clone(),
                })
                .collect();
            // Build file chunk messages for agent mode (large attached files
            // split into consecutive user messages).
            let file_chunk_messages: Vec<String> = file_chunks
                .iter()
                .map(|(file_name, chunk)| {
                    format!("--- Attached file: {} (continued) ---\n{}", file_name, chunk)
                })
                .collect();

            let turn = agent_chat::run(
                &serving,
                &model_label,
                system.clone(),
                history,
                user_prompt.clone(),
                file_chunk_messages,
                &record.path,
                &personal_space,
                record.internet_enabled,
                sampling,
                max_tool_steps,
                || run.cancelled(),
                |call, delta| emit(false, Some(call), delta),
                |call, delta| emit(true, Some(call), delta),
                |event| emit_agent_event(&event),
            )
            .await?;
            (
                turn.content,
                turn.reasoning,
                turn.steps,
                turn.usage.prompt_tokens,
                turn.usage.completion_tokens,
                turn.usage.total_tokens,
                turn.context_tokens,
                turn.timings.predicted_per_second,
                turn.cancelled,
            )
        } else {
            let completion = client::complete_stream(
                &serving,
                &model_label,
                &messages,
                // A brand-new chat never reuses another conversation's cache.
                !fresh,
                sampling,
                false, // allow_empty: reject empty answers (default behavior)
                || run.cancelled(),
                |delta| emit(false, None, delta),
                |delta| emit(true, None, delta),
            )
            .await?;
            let usage = completion.usage.unwrap_or_default();
            let timings = completion.timings.unwrap_or_default();
            (
                completion.content,
                completion.reasoning,
                Vec::new(),
                usage.prompt_tokens,
                usage.completion_tokens,
                usage.total_tokens,
                usage.prompt_tokens + usage.completion_tokens,
                timings.predicted_per_second,
                run.cancelled(),
            )
        };

    // A stop before the model said anything still gets a visible, saved turn
    // instead of an empty bubble.
    if cancelled && content.trim().is_empty() && steps.is_empty() {
        content = "Stopped.".to_string();
    }

    // 4. Save the turn: one JSON file per session in the project's chats folder.
    let now = time::now_iso();
    session.messages.push(StoredMessage {
        id: hash::unique_id("msg"),
        role: "user".to_string(),
        content: prompt.clone(),
        reasoning: String::new(),
        steps: Vec::new(),
        created_at: now.clone(),
        citations: Vec::new(),
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
        context_tokens: 0,
        tokens_per_second: 0.0,
    });
    session.messages.push(StoredMessage {
        id: hash::unique_id("msg"),
        role: "assistant".to_string(),
        content: content.clone(),
        reasoning: reasoning.clone(),
        steps: steps.clone(),
        created_at: now,
        citations: citations.clone(),
        prompt_tokens,
        completion_tokens,
        total_tokens,
        context_tokens,
        tokens_per_second,
    });
    let summary = chats::save(&chat_dir, &mut session)?;

    let _ = activity::record(
        &paths,
        activity::ActivityKind::Local,
        "chat_send",
        &summarize(&prompt),
    );

    Ok(ChatStreamResult {
        session_id: summary.id,
        title: summary.title,
        content,
        reasoning,
        steps,
        citations,
        prompt_tokens,
        completion_tokens,
        total_tokens,
        context_tokens,
        tokens_per_second,
        cancelled,
    })
}

/// Stops the chat turn currently running for a project.
///
/// The loop checks the flag between model calls and tool calls, so the turn
/// ends after the step in flight and keeps what it produced so far.
#[tauri::command]
pub fn chat_stop(project_id: String) {
    crate::agent::cancel::cancel(&project_id);
}

/// The chat system prompt the user falls back to when they have not set their
/// own for this project.
///
/// It also teaches the one convention every agent-mode tool call depends on:
/// how files are named in this app. Agent mode prepends its rules under this
/// base, so the two modes never disagree about paths.
fn system_prompt(project_name: &str) -> String {
    format!(
        "You are localAIse, a private assistant that works only inside the \
project folder “{project_name}”. The files in that folder never leave this machine.\n\n\
HOW FILES ARE NAMED HERE:\n\
1. Every file is named by its path relative to the project root, such as \
\"data/notes.md\" or \"outputs/report.md\". These name the format; which files \
actually exist is a separate question.\n\
2. The user mentions files with a leading @, such as @outputs/file.txt. The @ is \
only this app's chat marker: the real path is exactly what follows it, \
outputs/file.txt. This mapping is fixed — @something always means just \
\"something\", in every message, with no exceptions.\n\
3. When you name a file — in an answer, a citation, or anything else — use the \
plain relative path. Never add @, quotes, backslashes, or absolute paths.\n\n\
Answer in clear, plain prose, and say when you are not sure instead of guessing."
    )
}

/// Keeps the activity row short enough to read at a glance.
fn summarize(prompt: &str) -> String {
    const LIMIT: usize = 80;
    let first_line = prompt.lines().next().unwrap_or(prompt).trim();
    if first_line.chars().count() <= LIMIT {
        return first_line.to_string();
    }
    let clipped: String = first_line.chars().take(LIMIT).collect();
    format!("{clipped}…")
}
