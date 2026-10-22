//! Reminders: local notification management.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reminder {
    pub id: String,
    pub title: String,
    pub due_at: String,
    pub completed: bool,
}

pub fn schedule_notification(title: &str, body: &str) {
    // In Tauri environment, notification triggers or logs to console
    println!("[Reminder Alert] {title}: {body}");
}
