//! Chat sessions saved as plain JSON inside `<project>/chats/`.
//!
//! One file per session, named after the moment it started, so the folder
//! stays readable without this program and a session can be inspected,
//! copied or deleted in any file explorer.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::retrieval::citations::Citation;
use crate::util::time;

#[cfg(test)]
use crate::util::hash;

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
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub citations: Vec<Citation>,
}

/// A full chat session: fronts `list`, backs `read`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSession {
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

/// Sequential chat numbering: `chat-001`, `chat-002`, etc.
pub fn new_session_id(dir: &Path) -> String {
    let next_num = next_chat_number(dir);
    format!("chat-{:03}", next_num)
}

/// Finds the highest existing chat number and returns the next one.
fn next_chat_number(dir: &Path) -> u32 {
    if let Ok(read_dir) = std::fs::read_dir(dir) {
        read_dir
            .flatten()
            .filter_map(|entry| entry.path().file_stem()?.to_str().map(|s| s.to_string()))
            .filter_map(|name| {
                name.strip_prefix("chat-")
                    .and_then(|n| n.parse::<u32>().ok())
            })
            .max()
            .map_or(1, |max| max + 1)
    } else {
        1
    }
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

fn file_for(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.json"))
}

/// Newest session first.
pub fn list(dir: &Path) -> Vec<ChatSummary> {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut sessions: Vec<ChatSummary> = read_dir
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "json"))
        .filter_map(|path| {
            let raw = std::fs::read_to_string(&path).ok()?;
            serde_json::from_str::<ChatSession>(&raw).ok()
        })
        .map(|session| session.summary())
        .collect();

    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    sessions
}

pub fn load(dir: &Path, id: &str) -> AppResult<ChatSession> {
    let id = safe_id(id)?;
    let path = file_for(dir, id);
    let raw = std::fs::read_to_string(&path)
        .map_err(|_| AppError::msg("that chat no longer exists in the chats folder."))?;
    serde_json::from_str(&raw)
        .map_err(|error| AppError::msg(format!("that chat file cannot be read: {error}")))
}

/// Writes the session back, stamping `updatedAt`.
pub fn save(dir: &Path, session: &mut ChatSession) -> AppResult<ChatSummary> {
    let id = safe_id(&session.id)?.to_string();
    std::fs::create_dir_all(dir)?;

    session.id = id.clone();
    session.updated_at = time::now_iso();
    if session.title.trim().is_empty() {
        session.title = first_line_title(session.messages.first().map(|message| message.content.as_str()));
    }

    let raw = serde_json::to_string_pretty(session)?;
    std::fs::write(file_for(dir, &id), raw)?;
    Ok(session.summary())
}

pub fn delete(dir: &Path, id: &str) -> AppResult<()> {
    let id = safe_id(id)?;
    let path = file_for(dir, id);
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
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
            created_at: time::now_iso(),
            citations: Vec::new(),
        }
    }

    #[test]
    fn save_write_list_load_and_delete_one_file_per_session() {
        let dir = temp_dir("roundtrip");

        let mut session = ChatSession::new(Some(new_session_id(&dir)));
        session.messages.push(message("user", "Where did spending rise?"));
        session.messages.push(message("assistant", "Hardware rose the most."));

        let summary = save(&dir, &mut session).unwrap();
        assert_eq!(summary.message_count, 2);
        assert_eq!(summary.title, "Where did spending rise?");

        // Exactly one JSON file, named after the session id.
        let files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .collect();
        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0].file_name().unwrap().to_string_lossy(),
            format!("{}.json", session.id)
        );

        let listed = list(&dir);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, session.id);

        let loaded = load(&dir, &session.id).unwrap();
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[1].content, "Hardware rose the most.");

        delete(&dir, &session.id).unwrap();
        assert!(list(&dir).is_empty());
        assert!(load(&dir, &session.id).is_err());

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
