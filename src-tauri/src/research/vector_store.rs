use crate::research::error::ResearchError;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use rusqlite::{Connection, params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Vector-enabled evidence store using sqlite-vec.
pub struct VectorStore {
    conn: Arc<Mutex<Connection>>,
    embedder: Arc<TextEmbedding>,
    _embedding_dim: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Source {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub text: String,
    pub quality_score: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Chunk {
    pub id: i64,
    pub source_id: i64,
    pub text: String,
    pub embedding: Vec<f32>,
}

impl VectorStore {
    /// Open or create a vector store at the given path.
    pub fn open(path: &Path) -> Result<Self, ResearchError> {
        let conn = Connection::open(path).map_err(|e| ResearchError::Store(e.to_string()))?;
        
        // Enable sqlite-vec extension
        #[cfg(not(target_family = "wasm"))]
        {
            use sqlite_vec::sqlite3_vec_init;
            unsafe {
                sqlite3_vec_init();
            }
        }

        // Initialize embedder (using a small, fast model)
        let embedder = TextEmbedding::try_new(InitOptions::new(EmbeddingModel::AllMiniLML6V2))
            .map_err(|e| ResearchError::Internal(format!("Failed to init embedder: {e}")))?;
        let embedding_dim = 384; // AllMiniLML6V2 dimension

        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
            embedder: Arc::new(embedder),
            _embedding_dim: embedding_dim,
        };

        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<(), ResearchError> {
        let conn = self.conn.lock().unwrap();
        
        // Create tables with vector support
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sources (
                id INTEGER PRIMARY KEY,
                url TEXT NOT NULL,
                title TEXT,
                text TEXT NOT NULL,
                quality_score REAL DEFAULT 0.0,
                fetched_at TEXT DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS chunks (
                id INTEGER PRIMARY KEY,
                source_id INTEGER NOT NULL,
                text TEXT NOT NULL,
                chunk_index INTEGER DEFAULT 0
            );
            CREATE VIRTUAL TABLE IF NOT EXISTS chunk_embeddings USING vec0(
                chunk_id INTEGER PRIMARY KEY,
                embedding FLOAT[384]
            );
            CREATE TABLE IF NOT EXISTS facts (
                id INTEGER PRIMARY KEY,
                subq_id INTEGER,
                claim TEXT,
                quote TEXT,
                source_id INTEGER,
                confidence REAL
            );
            CREATE TABLE IF NOT EXISTS questions (
                id INTEGER PRIMARY KEY,
                parent_id INTEGER,
                text TEXT,
                status TEXT
            );"
        ).map_err(|e| ResearchError::Store(e.to_string()))?;

        Ok(())
    }

    /// Add a source and chunk it with embeddings.
    pub fn add_source(&self, url: &str, title: &str, text: &str, quality_score: f32) -> Result<i64, ResearchError> {
        let conn = self.conn.lock().unwrap();
        
        let tx = conn.unchecked_transaction().map_err(|e| ResearchError::Store(e.to_string()))?;
        
        // Insert source
        tx.execute(
            "INSERT INTO sources (url, title, text, quality_score) VALUES (?, ?, ?, ?)",
            params![url, title, text, quality_score],
        ).map_err(|e| ResearchError::Store(e.to_string()))?;
        
        let source_id = tx.last_insert_rowid();
        
        // Chunk the text and generate embeddings
        let chunks = chunk_text(text, 500); // 500 chars per chunk
        let chunk_texts: Vec<&str> = chunks.iter().map(|s| s.as_str()).collect();
        
        // Generate embeddings in batch
        let embeddings = self.embedder.embed(chunk_texts, None)
            .map_err(|e| ResearchError::Internal(format!("Embedding failed: {e}")))?;
        
        // Insert chunks and embeddings
        for (i, (chunk_text, embedding)) in chunks.into_iter().zip(embeddings.into_iter()).enumerate() {
            tx.execute(
                "INSERT INTO chunks (source_id, text, chunk_index) VALUES (?, ?, ?)",
                params![source_id, chunk_text, i as i64],
            ).map_err(|e| ResearchError::Store(e.to_string()))?;
            
            let chunk_id = tx.last_insert_rowid();
            
            // Insert embedding into vec0 table
            let embedding_blob: Vec<u8> = embedding.iter()
                .flat_map(|f| f.to_le_bytes())
                .collect();
            
            tx.execute(
                "INSERT INTO chunk_embeddings (chunk_id, embedding) VALUES (?, ?)",
                params![chunk_id, embedding_blob],
            ).map_err(|e| ResearchError::Store(e.to_string()))?;
        }
        
        tx.commit().map_err(|e| ResearchError::Store(e.to_string()))?;
        Ok(source_id)
    }

    /// Find top-k similar chunks for a query using vector search.
    pub fn search_chunks(&self, query: &str, top_k: usize) -> Result<Vec<(Chunk, f32)>, ResearchError> {
        let query_embedding = self.embedder.embed(vec![query], None)
            .map_err(|e| ResearchError::Internal(format!("Query embedding failed: {e}")))?
            .into_iter().next()
            .ok_or_else(|| ResearchError::Internal("No query embedding generated".into()))?;

        let conn = self.conn.lock().unwrap();
        
        // Use sqlite-vec KNN search
        let mut stmt = conn.prepare(
            "SELECT c.id, c.source_id, c.text, c.chunk_id as chunk_index,
                    vec_distance_cosine(e.embedding, ?) as distance
             FROM chunks c
             JOIN chunk_embeddings e ON c.id = e.chunk_id
             ORDER BY distance ASC
             LIMIT ?"
        ).map_err(|e| ResearchError::Store(e.to_string()))?;

        let embedding_blob: Vec<u8> = query_embedding.iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        let rows = stmt.query_map(params![embedding_blob, top_k as i64], |row| {
            Ok((
                Chunk {
                    id: row.get(0)?,
                    source_id: row.get(1)?,
                    text: row.get(2)?,
                    embedding: vec![], // Not loading full embedding back
                },
                row.get::<_, f32>(4)?, // distance
            ))
        }).map_err(|e| ResearchError::Store(e.to_string()))?;

        let mut results = Vec::new();
        for row in rows {
            let (chunk, distance) = row.map_err(|e| ResearchError::Store(e.to_string()))?;
            // Convert distance to similarity (cosine distance -> similarity)
            let similarity = 1.0 - distance;
            results.push((chunk, similarity));
        }

        Ok(results)
    }

    /// Get source by ID.
    pub fn get_source(&self, id: i64) -> Result<Option<Source>, ResearchError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, url, title, text, quality_score FROM sources WHERE id = ?"
        ).map_err(|e| ResearchError::Store(e.to_string()))?;

        let source = stmt.query_row(params![id], |row| {
            Ok(Source {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                text: row.get(3)?,
                quality_score: row.get(4)?,
            })
        }).optional().map_err(|e| ResearchError::Store(e.to_string()))?;

        Ok(source)
    }

    /// Get all sources.
    pub fn sources(&self) -> Result<Vec<Source>, ResearchError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, url, title, text, quality_score FROM sources"
        ).map_err(|e| ResearchError::Store(e.to_string()))?;

        let rows = stmt.query_map([], |row| {
            Ok(Source {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                text: row.get(3)?,
                quality_score: row.get(4)?,
            })
        }).map_err(|e| ResearchError::Store(e.to_string()))?;

        let mut sources = Vec::new();
        for row in rows {
            sources.push(row.map_err(|e| ResearchError::Store(e.to_string()))?);
        }
        Ok(sources)
    }
}

/// Simple text chunking function.
fn chunk_text(text: &str, target_size: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for paragraph in text.split("\n\n") {
        if current.len() + paragraph.len() > target_size && !current.is_empty() {
            chunks.push(current.trim().to_string());
            current.clear();
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(paragraph);
    }

    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }

    // Handle very long paragraphs
    let mut final_chunks = Vec::new();
    for chunk in chunks {
        if chunk.len() <= target_size {
            final_chunks.push(chunk);
        } else {
            // Split long chunk by sentences
            let sentences: Vec<&str> = chunk.split(['.', '!', '?']).collect();
            let mut sent_chunk = String::new();
            for sent in sentences {
                let sent = sent.trim();
                if sent.is_empty() { continue; }
                if sent_chunk.len() + sent.len() + 2 > target_size && !sent_chunk.is_empty() {
                    final_chunks.push(std::mem::take(&mut sent_chunk));
                }
                if !sent_chunk.is_empty() {
                    sent_chunk.push_str(". ");
                }
                sent_chunk.push_str(sent);
            }
            if !sent_chunk.is_empty() {
                final_chunks.push(sent_chunk);
            }
        }
    }

    final_chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_chunk_text() {
        let text = "Short paragraph.\n\nAnother paragraph here.\n\nThird one.";
        let chunks = chunk_text(text, 100);
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_long_paragraph_splitting() {
        let text = "This is a very long sentence that should be split into multiple chunks because it exceeds the target size significantly and we need to handle this case properly.";
        let chunks = chunk_text(text, 50);
        assert!(chunks.len() > 1);
    }
}