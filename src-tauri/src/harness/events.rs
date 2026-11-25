//! Durable SQLite-backed harness event log (`<project>/.LocalAIse/harness.db`).

use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::util::time;

fn db_err(err: rusqlite::Error) -> AppError {
    AppError::msg(format!("harness db error: {err}"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum HarnessEvent {
    TurnStarted {
        agent_id: String,
        prompt_preview: String,
    },
    ToolStarted {
        call: u32,
        id: String,
        name: String,
        input: String,
    },
    ToolFinished {
        call: u32,
        id: String,
        name: String,
        ok: bool,
        summary: String,
    },
    RuntimeRequested {
        request_id: String,
        tool: String,
        action_kind: String,
        summary: String,
    },
    TurnFinished {
        cancelled: bool,
        total_tokens: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredHarnessEvent {
    pub id: i64,
    pub session_id: String,
    pub created_at: String,
    pub event: HarnessEvent,
}

/// Append-only event store backed by SQLite in `<project>/.LocalAIse/harness.db`.
pub struct HarnessEventStore {
    conn: Connection,
}

impl HarnessEventStore {
    pub fn open(project_root: &Path) -> AppResult<Self> {
        let dir = project_root.join(".LocalAIse");
        std::fs::create_dir_all(&dir)?;
        let db_path = dir.join("harness.db");
        let conn = Connection::open(db_path).map_err(db_err)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS harness_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                kind TEXT NOT NULL,
                payload TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_harness_events_session
                ON harness_events(session_id, id);",
        )
        .map_err(db_err)?;
        Ok(Self { conn })
    }

    pub fn append(&self, session_id: &str, event: &HarnessEvent) -> AppResult<()> {
        let payload = serde_json::to_string(event)?;
        let kind = match event {
            HarnessEvent::TurnStarted { .. } => "turnStarted",
            HarnessEvent::ToolStarted { .. } => "toolStarted",
            HarnessEvent::ToolFinished { .. } => "toolFinished",
            HarnessEvent::RuntimeRequested { .. } => "runtimeRequested",
            HarnessEvent::TurnFinished { .. } => "turnFinished",
        };
        let now = time::now_iso();
        self.conn
            .execute(
                "INSERT INTO harness_events (session_id, created_at, kind, payload)
             VALUES (?1, ?2, ?3, ?4)",
                params![session_id, now, kind, payload],
            )
            .map_err(db_err)?;
        Ok(())
    }

    pub fn list(&self, session_id: &str) -> AppResult<Vec<StoredHarnessEvent>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, session_id, created_at, payload
             FROM harness_events
             WHERE session_id = ?1
             ORDER BY id ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![session_id], |row| {
                let id: i64 = row.get(0)?;
                let sid: String = row.get(1)?;
                let created_at: String = row.get(2)?;
                let payload: String = row.get(3)?;
                Ok((id, sid, created_at, payload))
            })
            .map_err(db_err)?;

        let mut out = Vec::new();
        for row in rows {
            let (id, sid, created_at, payload) = row.map_err(db_err)?;
            if let Ok(event) = serde_json::from_str::<HarnessEvent>(&payload) {
                out.push(StoredHarnessEvent {
                    id,
                    session_id: sid,
                    created_at,
                    event,
                });
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_and_reads_events_in_order() {
        let dir = std::env::temp_dir().join(format!(
            "localaise_events_test_{}",
            crate::util::hash::unique_id("t")
        ));
        let store = HarnessEventStore::open(&dir).unwrap();
        store
            .append(
                "s1",
                &HarnessEvent::TurnStarted {
                    agent_id: "default".into(),
                    prompt_preview: "hello".into(),
                },
            )
            .unwrap();
        store
            .append(
                "s1",
                &HarnessEvent::TurnFinished {
                    cancelled: false,
                    total_tokens: 42,
                },
            )
            .unwrap();

        let events = store.list("s1").unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0].event, HarnessEvent::TurnStarted { .. }));
        assert!(matches!(events[1].event, HarnessEvent::TurnFinished { total_tokens: 42, .. }));
        drop(events);
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
