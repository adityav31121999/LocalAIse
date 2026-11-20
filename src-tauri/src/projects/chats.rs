//! Chat sessions saved as plain JSON inside `<project>/chats/`.
//!
//! Chat mode sessions are saved inside `<project>/chats/chat/chat{id}.json`
//! and agent mode sessions inside `<project>/chats/agent/agent{id}.json`.
//! The system prompt being used is stored at the beginning of the JSON file.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::projects::layout::ChatMode;
use crate::retrieval::citations::Citation;
use crate::runtime::CallUsage;
use crate::util::time;

#[cfg(test)]
use crate::util::hash;

/// One segment of an agent turn, kept in the order the model produced it.
///
/// Agent mode can think, call tools, and answer several times in a single
/// turn. Saving the segments keeps that shape when the chat is reopened
/// instead of flattening everything into one reasoning string and one answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStep {
    pub id: String,
    /// `reasoning`, `content`, or `tool`.
    pub kind: String,
    /// Which model call inside the turn produced this segment.
    #[serde(default)]
    pub call: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<AgentToolStep>,
}

/// One tool call inside an agent turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolStep {
    pub id: String,
    pub name: String,
    /// A short, one-line summary for the transcript (first meaningful argument).
    pub input: String,
    /// `running`, `done`, or `error`.
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
    /// The full raw argument JSON the model sent, unclipped.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub arguments: String,
    /// The tool result the model read back, clipped only by the loop's own
    /// truncation. Recorded so the saved chat is a complete account of the call.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub output: String,
}

/// One turn as it is written to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredMessage {
    pub id: String,
    /// `user` or `assistant`.
    pub role: String,
    pub content: String,
    /// The model's reasoning trace, when it produced one.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reasoning: String,
    /// Ordered reasoning, tool, and answer segments (agent turns only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<AgentStep>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub citations: Vec<Citation>,
    /// Prompt tokens this turn read (assistant turns only).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub prompt_tokens: u32,
    /// Tokens this turn generated.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub completion_tokens: u32,
    /// Prompt plus completion across the turn's model calls.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub total_tokens: u32,
    /// Prompt plus completion of the last call: how full the context was.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub context_tokens: u32,
    /// Generated tokens per second.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub tokens_per_second: f64,
    /// Tokens of every single model call, in order (agent tool-loop calls).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub call_usages: Vec<CallUsage>,
    /// The exact system prompt sent for this turn (assistant turns only).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub system_prompt: String,
    /// The exact user message sent to the model, including any injected
    /// context block (assistant turns only). The user's raw prompt is in
    /// the preceding user message.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model_prompt: String,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

fn is_zero_f64(value: &f64) -> bool {
    *value == 0.0
}

/// A full chat session: fronts `list`, backs `read`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSession {
    /// The exact system prompt used for this chat, stored at the beginning of the JSON file.
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "system_prompt")]
    pub system_prompt: Option<String>,
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub messages: Vec<StoredMessage>,
}

/// What the session list shows without loading every message.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSummary {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
}

impl ChatSession {
    pub fn new(id: Option<String>) -> Self {
        let now = time::now_iso();
        Self {
            system_prompt: None,
            id: id.unwrap_or_default(),
            title: String::new(),
            created_at: now.clone(),
            updated_at: now,
            messages: Vec::new(),
        }
    }

    pub fn summary(&self) -> ChatSummary {
        ChatSummary {
            id: self.id.clone(),
            title: self.title.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            message_count: self.messages.len(),
        }
    }
}

impl Default for ChatSession {
    fn default() -> Self {
        Self::new(None)
    }
}

/// Sequential chat numbering: `chat-001`, `chat-002`, etc. or `agent-001`, `agent-002`, etc.
pub fn new_session_id(dir: &Path, mode: ChatMode) -> String {
    let (prefix, sub) = match mode {
        ChatMode::Agent => ("agent-", "agent"),
        ChatMode::Chat => ("chat-", "chat"),
    };
    let next_num = next_number(dir, sub, prefix);
    format!("{prefix}{:03}", next_num)
}

/// Finds the highest existing number for this prefix and returns the next one.
fn next_number(dir: &Path, sub: &str, prefix: &str) -> u32 {
    let sub_dir = dir.join(sub);
    let mut max_num = 0;

    let mut scan = |target_dir: &Path| {
        if let Ok(read_dir) = std::fs::read_dir(target_dir) {
            for entry in read_dir.flatten() {
                if let Some(stem) = entry.path().file_stem().and_then(|s| s.to_str()) {
                    let num = if let Some(stripped) = stem.strip_prefix(prefix) {
                        stripped.parse::<u32>().ok()
                    } else if let Some(stripped) = stem.strip_prefix(prefix.trim_end_matches('-')) {
                        stripped.parse::<u32>().ok()
                    } else {
                        None
                    };
                    if let Some(n) = num {
                        if n > max_num {
                            max_num = n;
                        }
                    }
                }
            }
        }
    };

    scan(&sub_dir);
    scan(dir);

    max_num + 1
}

/// Rejects ids that could escape the chats folder.
fn safe_id(id: &str) -> AppResult<&str> {
    let valid = !id.is_empty()
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_');
    if valid {
        Ok(id)
    } else {
        Err(AppError::msg("that chat id is not valid."))
    }
}

/// Path where a chat file with `id` should be placed:
/// - Agent sessions go into `<chats_dir>/agent/<id>.json`
/// - Chat sessions go into `<chats_dir>/chat/<id>.json`
pub fn file_for(dir: &Path, id: &str) -> PathBuf {
    let sub = if id.starts_with("agent") {
        "agent"
    } else {
        "chat"
    };
    dir.join(sub).join(format!("{id}.json"))
}

/// Finds an existing chat file by looking in its dedicated folder (`chats/agent/` or `chats/chat/`),
/// the opposite subfolder, or the legacy root `chats/` directory.
pub fn find_chat_file(dir: &Path, id: &str) -> Option<PathBuf> {
    let sub = if id.starts_with("agent") { "agent" } else { "chat" };
    let primary = dir.join(sub).join(format!("{id}.json"));
    if primary.is_file() {
        return Some(primary);
    }
    let other_sub = if sub == "agent" { "chat" } else { "agent" };
    let secondary = dir.join(other_sub).join(format!("{id}.json"));
    if secondary.is_file() {
        return Some(secondary);
    }
    let legacy = dir.join(format!("{id}.json"));
    if legacy.is_file() {
        return Some(legacy);
    }
    // Also check if id was passed without prefix e.g. "001" or "-001"
    for s in &["agent", "chat"] {
        for sep in &["-", ""] {
            let candidate = dir.join(s).join(format!("{s}{sep}{id}.json"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Newest session first.
pub fn list(dir: &Path) -> Vec<ChatSummary> {
    let mut sessions: Vec<ChatSummary> = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();

    // 1. Scan subdirectories: chats/chat and chats/agent
    for sub in &["chat", "agent"] {
        let sub_dir = dir.join(sub);
        if let Ok(read_dir) = std::fs::read_dir(&sub_dir) {
            for entry in read_dir.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                    if let Ok(raw) = std::fs::read_to_string(&path) {
                        if let Ok(session) = serde_json::from_str::<ChatSession>(&raw) {
                            if seen_ids.insert(session.id.clone()) {
                                sessions.push(session.summary());
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Scan legacy root chats/ folder (for backward compatibility)
    if let Ok(read_dir) = std::fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                if let Ok(raw) = std::fs::read_to_string(&path) {
                    if let Ok(session) = serde_json::from_str::<ChatSession>(&raw) {
                        if seen_ids.insert(session.id.clone()) {
                            sessions.push(session.summary());
                        }
                    }
                }
            }
        }
    }

    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    sessions
}

pub fn load(dir: &Path, id: &str) -> AppResult<ChatSession> {
    let id = safe_id(id)?;
    let path = find_chat_file(dir, id).unwrap_or_else(|| file_for(dir, id));
    let raw = std::fs::read_to_string(&path)
        .map_err(|_| AppError::msg("that chat no longer exists in the chats folder."))?;
    serde_json::from_str(&raw)
        .map_err(|error| AppError::msg(format!("that chat file cannot be read: {error}")))
}

/// Writes the session back, stamping `updatedAt`.
pub fn save(dir: &Path, session: &mut ChatSession) -> AppResult<ChatSummary> {
    let id = safe_id(&session.id)?.to_string();
    let path = file_for(dir, &id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    session.id = id.clone();
    session.updated_at = time::now_iso();
    if session.title.trim().is_empty() {
        session.title = first_line_title(session.messages.first().map(|message| message.content.as_str()));
    }

    let raw = serde_json::to_string_pretty(session)?;
    std::fs::write(&path, raw)?;

    // If an older copy was in the legacy root chats/ folder, clean it up
    let legacy = dir.join(format!("{id}.json"));
    if legacy.is_file() && legacy != path {
        let _ = std::fs::remove_file(legacy);
    }

    Ok(session.summary())
}

pub fn delete(dir: &Path, id: &str) -> AppResult<()> {
    let id = safe_id(id)?;
    if let Some(path) = find_chat_file(dir, id) {
        let _ = std::fs::remove_file(path);
    }
    let default_path = file_for(dir, id);
    if default_path.is_file() {
        let _ = std::fs::remove_file(default_path);
    }
    let legacy = dir.join(format!("{id}.json"));
    if legacy.is_file() {
        let _ = std::fs::remove_file(legacy);
    }
    Ok(())
}

/// A short, filesystem-friendly label for a chat, used as its title.
pub fn first_line_title(prompt: Option<&str>) -> String {
    const LIMIT: usize = 60;
    let Some(prompt) = prompt else {
        return "New chat".to_string();
    };
    let first_line = prompt.lines().next().unwrap_or(prompt).trim();
    if first_line.is_empty() {
        return "New chat".to_string();
    }
    if first_line.chars().count() <= LIMIT {
        return first_line.to_string();
    }
    let clipped: String = first_line.chars().take(LIMIT).collect();
    format!("{clipped}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "LocalAIse-chats-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn message(role: &str, content: &str) -> StoredMessage {
        StoredMessage {
            id: hash::unique_id("msg"),
            role: role.to_string(),
            content: content.to_string(),
            reasoning: String::new(),
            steps: Vec::new(),
            created_at: time::now_iso(),
            citations: Vec::new(),
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            context_tokens: 0,
            tokens_per_second: 0.0,
            call_usages: Vec::new(),
            system_prompt: String::new(),
            model_prompt: String::new(),
        }
    }

    #[test]
    fn save_write_list_load_and_delete_one_file_per_session() {
        let dir = temp_dir("roundtrip");

        let mut session = ChatSession::new(Some(new_session_id(&dir, ChatMode::Chat)));
        session.system_prompt = Some("You are a helpful assistant.".to_string());
        session.messages.push(message("user", "Where did spending rise?"));
        session.messages.push(message("assistant", "Hardware rose the most."));

        let summary = save(&dir, &mut session).unwrap();
        assert_eq!(summary.message_count, 2);
        assert_eq!(summary.title, "Where did spending rise?");

        // Exactly one JSON file inside chats/chat/
        let chat_dir = dir.join("chat");
        let files: Vec<_> = std::fs::read_dir(&chat_dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .collect();
        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0].file_name().unwrap().to_string_lossy(),
            format!("{}.json", session.id)
        );

        let raw = std::fs::read_to_string(&files[0]).unwrap();
        // Verify systemPrompt is at the beginning of the json file
        assert!(
            raw.trim_start().starts_with("{\n  \"systemPrompt\":"),
            "systemPrompt should be at beginning of JSON: {raw}"
        );

        let listed = list(&dir);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, session.id);

        let loaded = load(&dir, &session.id).unwrap();
        assert_eq!(loaded.system_prompt.as_deref(), Some("You are a helpful assistant."));
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[1].content, "Hardware rose the most.");

        delete(&dir, &session.id).unwrap();
        assert!(list(&dir).is_empty());
        assert!(load(&dir, &session.id).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_agent_mode_in_agent_dir_with_system_prompt() {
        let dir = temp_dir("agent-roundtrip");

        let mut session = ChatSession::new(Some(new_session_id(&dir, ChatMode::Agent)));
        assert_eq!(session.id, "agent-001");
        session.system_prompt = Some("You are an agent with tools.".to_string());
        session.messages.push(message("user", "List files"));
        session.messages.push(message("assistant", "Found 3 files."));

        let summary = save(&dir, &mut session).unwrap();
        assert_eq!(summary.id, "agent-001");

        // Saved in chats/agent/agent-001.json
        let agent_file = dir.join("agent").join("agent-001.json");
        assert!(agent_file.is_file());

        let raw = std::fs::read_to_string(&agent_file).unwrap();
        assert!(raw.trim_start().starts_with("{\n  \"systemPrompt\":"));

        let loaded = load(&dir, "agent-001").unwrap();
        assert_eq!(loaded.system_prompt.as_deref(), Some("You are an agent with tools."));

        // Second agent session gets agent-002
        let next_id = new_session_id(&dir, ChatMode::Agent);
        assert_eq!(next_id, "agent-002");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_chats_are_loaded_and_listed() {
        let dir = temp_dir("legacy");

        // Put a legacy chat file in root dir
        let legacy_file = dir.join("chat-001.json");
        let mut session = ChatSession::new(Some("chat-001".to_string()));
        session.system_prompt = Some("Legacy prompt".to_string());
        session.messages.push(message("user", "Old message"));
        std::fs::write(&legacy_file, serde_json::to_string_pretty(&session).unwrap()).unwrap();

        let listed = list(&dir);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "chat-001");

        let loaded = load(&dir, "chat-001").unwrap();
        assert_eq!(loaded.messages.len(), 1);

        // Saving it migrates it to chats/chat/chat-001.json and removes legacy file
        let mut loaded_mut = loaded;
        save(&dir, &mut loaded_mut).unwrap();
        assert!(dir.join("chat").join("chat-001.json").is_file());
        assert!(!legacy_file.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ids_cannot_escape_the_chats_folder() {
        let dir = temp_dir("ids");
        assert!(load(&dir, "../secrets").is_err());
        assert!(delete(&dir, "..\\secrets").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_long_first_prompt_becomes_a_short_title() {
        let title = first_line_title(Some(&"x".repeat(200)));
        assert_eq!(title.chars().count(), 61);
        assert!(title.ends_with('…'));
        assert_eq!(first_line_title(None), "New chat");
    }
}
