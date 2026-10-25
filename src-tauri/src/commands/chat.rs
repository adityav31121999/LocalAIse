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

use tauri::{AppHandle, Emitter, State};

use crate::agent::agent_chat;
use crate::activity::log as activity;
use crate::error::{AppError, AppResult};
use crate::models::registry as models;
use crate::projects::chats::{self, ChatSession, StoredMessage};
use crate::projects::layout::{ChatMode, ProjectPaths};
use crate::retrieval::citations::Citation;
use crate::runtime::client;
use crate::util::{hash, time};
use crate::AppState;

/// How many source chunks a cited answer uses.
const RETRIEVAL_LIMIT: usize = 4;

/// How many earlier turns are replayed to the model, so long chats keep a
/// bounded prompt.
const HISTORY_LIMIT: usize = 12;

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
    pub citations: Vec<Citation>,
}

#[tauri::command]
pub async fn chat_send(
    app: AppHandle,
    state: State<'_, AppState>,
    project_id: String,
    // `None` starts a new chat; a saved id continues that conversation.
    session_id: Option<String>,
    prompt: String,
) -> AppResult<ChatStreamResult> {
    let prompt = prompt.trim().to_string();
    if prompt.is_empty() {
        return Err(AppError::msg("Type a question first."));
    }

    let (record, settings) = {
        let registry = state.registry()?;
        let record = registry
            .find(&project_id)
            .cloned()
            .ok_or_else(|| AppError::UnknownProject(project_id.clone()))?;
        (record, registry.settings.clone())
    };

    let model = models::selected(&state.dirs, &settings).ok_or(AppError::NoModelSelected)?;

    // The runtime resolves the device, loads the projector that belongs to
    // the weights and starts the server with the validated flags. A project
    // with its own settings gets those.
    let serving = crate::commands::runtime::ensure(&state, &settings, Some(&project_id))?;

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

    // 2. Retrieve relevant source context from indexed project sources.
    let index_db = crate::index::db::IndexDb::new(&record.path);
    let project_index = index_db.load();
    let scored_chunks = crate::retrieval::search::search_chunks(&project_index, &prompt, RETRIEVAL_LIMIT);
    let citations = crate::retrieval::search::format_citations(&scored_chunks);

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
        agent_chat::instructions(&system, record.internet_enabled)
    } else {
        system
    };

    // 3. Replay the tail of the conversation, then ask the new question.
    let mut messages = vec![client::message("system", system.clone())];
    let history_start = session.messages.len().saturating_sub(HISTORY_LIMIT);
    for turn in &session.messages[history_start..] {
        let role = if turn.role == "assistant" { "assistant" } else { "user" };
        messages.push(client::message(role, turn.content.clone()));
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
                let mut cpu_profile = cpu_settings.profile_for(Some(&model), Some(&project_id)).clone();
                cpu_profile.device = crate::runtime::profile::ComputeDevice::Cpu;

                let cpu_server = crate::runtime::backend::resolve_server(&state.dirs, &cpu_settings)
                    .ok_or(AppError::msg("no CPU-capable llama-server found"))?;
                let projector = models::projector_for(&model);

                let cpu_serving = {
                    let mut supervisor = state.supervisor()?;
                    supervisor.ensure(&crate::runtime::supervisor::LaunchRequest {
                        model: model.as_path(),
                        projector: projector.as_deref(),
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
    let emit = |reasoning: bool, delta: &str| {
        emitter
            .emit(
                "chat://token",
                serde_json::json!({
                    "projectId": project_id,
                    "delta": delta,
                    "reasoning": reasoning,
                }),
            )
            .ok();
    };

    // Agent mode runs the LangChain tools agent over the same local server;
    // chat mode is one streamed call.
    let (content, reasoning) = if record.chat_mode == ChatMode::Agent {
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
        let turn = agent_chat::run(
            &serving,
            &model_label,
            system.clone(),
            history,
            user_prompt.clone(),
            &record.path,
            &personal_space,
            record.internet_enabled,
            |delta| emit(false, delta),
        )
        .await?;
        (turn.content, String::new())
    } else {
        let completion = client::complete_stream(
            &serving,
            &model_label,
            &messages,
            // A brand-new chat never reuses another conversation's cache.
            !fresh,
            |delta| emit(false, delta),
            |delta| emit(true, delta),
        )
        .await?;
        (completion.content, completion.reasoning)
    };

    // 4. Save the turn: one JSON file per session in the project's chats folder.
    let now = time::now_iso();
    session.messages.push(StoredMessage {
        id: hash::unique_id("msg"),
        role: "user".to_string(),
        content: prompt.clone(),
        reasoning: String::new(),
        created_at: now.clone(),
        citations: Vec::new(),
    });
    session.messages.push(StoredMessage {
        id: hash::unique_id("msg"),
        role: "assistant".to_string(),
        content: content.clone(),
        reasoning: reasoning.clone(),
        created_at: now,
        citations: citations.clone(),
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
        citations,
    })
}

/// The chat system prompt the user falls back to when they have not set their
/// own for this project.
fn system_prompt(project_name: &str) -> String {
    format!(
        "You are localAIse, a private assistant that works only inside the folder “{project_name}”. \
         The files in that folder never leave this machine. Answer in clear, plain prose, and say \
         when you are not sure instead of guessing."
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
