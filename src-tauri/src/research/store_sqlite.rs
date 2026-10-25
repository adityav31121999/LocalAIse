use rusqlite::Connection;

pub struct SqliteStore { pub conn: Connection }

impl SqliteStore {
    pub fn open(path: &str) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sources (id INTEGER PRIMARY KEY, url TEXT, title TEXT, fetched_at TEXT, content_hash TEXT, quality_score REAL);
             CREATE TABLE IF NOT EXISTS chunks (id INTEGER PRIMARY KEY, source_id INTEGER, text TEXT);
             CREATE TABLE IF NOT EXISTS facts (id INTEGER PRIMARY KEY, subq_id INTEGER, claim TEXT, quote TEXT, source_id INTEGER, confidence REAL);
             CREATE TABLE IF NOT EXISTS questions (id INTEGER PRIMARY KEY, parent_id INTEGER, text TEXT, status TEXT);",
        ).map_err(|e| e.to_string())?;
        Ok(Self { conn })
    }
}