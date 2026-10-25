pub mod metrics;
pub mod harness;
pub mod retrievers;
pub mod validate;

use anyhow::Result;
use crate::eval::harness::Retriever;

/// Build a retriever by name. Returns an error for unknown names.
/// This will be extended as the vertical slice lands:
/// - "bm25"     → current BM25 search
/// - "dense"    → dense vector search
/// - "hybrid"   → RRF(dense, BM25)
/// - "rerank"   → hybrid + cross-encoder rerank
pub fn build_retriever(name: &str, project_dir: &std::path::Path) -> Result<Box<dyn Retriever>> {
    match name {
        "bm25" => {
            let r = crate::eval::retrievers::Bm25Retriever::new(project_dir)?;
            Ok(Box::new(r))
        }
        "dense" => Err(anyhow::anyhow!("dense retriever not yet wired")),
        "hybrid" => Err(anyhow::anyhow!("hybrid retriever not yet wired")),
        "rerank" => Err(anyhow::anyhow!("rerank retriever not yet wired")),
        _ => Err(anyhow::anyhow!("unknown retriever: {}", name)),
    }
}