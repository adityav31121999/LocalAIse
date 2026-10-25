use super::metrics::Hit;
use super::harness::Retriever;
use crate::index::db::{IndexDb, ProjectIndex};
use crate::retrieval::search::search_chunks;
use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;

/// BM25 retriever that uses the existing JSON index.
pub struct Bm25Retriever {
    index: Arc<ProjectIndex>,
}

impl Bm25Retriever {
    pub fn new(project_dir: &Path) -> anyhow::Result<Self> {
        let db = IndexDb::new(project_dir);
        let index = db.load();
        Ok(Self {
            index: Arc::new(index),
        })
    }
}

#[async_trait]
impl Retriever for Bm25Retriever {
    fn name(&self) -> &str {
        "bm25"
    }

    async fn retrieve(&self, query: &str, limit: usize) -> anyhow::Result<Vec<Hit>> {
        let scored = search_chunks(&self.index, query, limit);
        let hits = scored
            .into_iter()
            .map(|sc| Hit {
                source: sc.chunk.file_name,
                text: sc.chunk.content,
                score: sc.score,
            })
            .collect();
        Ok(hits)
    }
}

/// Wrapper to allow passing a pre-loaded index (useful for tests).
pub struct Bm25RetrieverWithIndex {
    index: Arc<ProjectIndex>,
}

impl Bm25RetrieverWithIndex {
    pub fn new(index: ProjectIndex) -> Self {
        Self {
            index: Arc::new(index),
        }
    }
}

#[async_trait]
impl Retriever for Bm25RetrieverWithIndex {
    fn name(&self) -> &str {
        "bm25"
    }

    async fn retrieve(&self, query: &str, limit: usize) -> anyhow::Result<Vec<Hit>> {
        let scored = search_chunks(&self.index, query, limit);
        let hits = scored
            .into_iter()
            .map(|sc| Hit {
                source: sc.chunk.file_name,
                text: sc.chunk.content,
                score: sc.score,
            })
            .collect();
        Ok(hits)
    }
}