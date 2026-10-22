//! Append-only record of what the agent did in a project folder.
//!
//! The file is JSON lines, one event per line, so it stays readable and can be
//! tailed without the app. Network rows are written by the network gate; for
//! now the chat runtime writes the local rows.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::projects::layout::ProjectPaths;
use crate::util::{hash, time};

/// How many rows the activity panel can show.
const MAX_ROWS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivityKind {
    /// Stayed inside the machine.
    Local,
    /// Left the machine. Only possible with the project's internet toggle on.
    Network,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEvent {
    pub id: String,
    pub at: String,
    pub kind: ActivityKind,
    /// The tool or subsystem that acted, e.g. `chat_send` or `web_fetch`.
    pub tool: String,
    pub summary: String,
}

pub fn record(project: &ProjectPaths, kind: ActivityKind, tool: &str, summary: &str) -> AppResult<()> {
    std::fs::create_dir_all(project.state_dir())?;

    let event = ActivityEvent {
        id: hash::unique_id("act"),
        at: time::now_iso(),
        kind,
        tool: tool.to_string(),
        summary: summary.to_string(),
    };

    let line = format!("{}\n", serde_json::to_string(&event)?);
    let file = project.activity_file();

    use std::io::Write;
    let mut handle = std::fs::OpenOptions::new().create(true).append(true).open(&file)?;
    handle.write_all(line.as_bytes())?;

    Ok(())
}

/// Newest first. Unparsable lines are skipped rather than failing the read.
pub fn list(project: &ProjectPaths) -> AppResult<Vec<ActivityEvent>> {
    let file = project.activity_file();
    let raw = match std::fs::read_to_string(&file) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };

    let mut events: Vec<ActivityEvent> = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<ActivityEvent>(line).ok())
        .collect();

    events.sort_by(|a, b| b.at.cmp(&a.at));
    events.truncate(MAX_ROWS);
    Ok(events)
}

/// Deletes the log for a project that is being removed from the registry.
pub fn clear(log_file: &Path) -> AppResult<()> {
    if log_file.exists() {
        std::fs::remove_file(log_file)?;
    }
    Ok(())
}
