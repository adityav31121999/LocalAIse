use std::collections::HashSet;

use crate::index::db::ProjectIndex;
use crate::ingest::chunk::Chunk;
use crate::retrieval::citations::Citation;

#[derive(Debug, Clone)]
pub struct ScoredChunk {
    pub chunk: Chunk,
    pub score: f32,
}

/// Tokenizes text into lowercase normalized alphanumeric keywords.
pub fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() > 1)
        .map(|s| s.to_string())
        .collect()
}

/// Top-k BM25 / keyword relevance search across indexed chunks.
pub fn search_chunks(index: &ProjectIndex, query: &str, limit: usize) -> Vec<ScoredChunk> {
    let query_tokens: Vec<String> = tokenize(query);
    if query_tokens.is_empty() {
        return Vec::new();
    }

    let query_set: HashSet<&String> = query_tokens.iter().collect();
    let mut scored: Vec<ScoredChunk> = Vec::new();

    for chunk in &index.chunks {
        let chunk_tokens = tokenize(&chunk.content);
        if chunk_tokens.is_empty() {
            continue;
        }

        let mut matches = 0;
        let mut term_freq = 0;
        for t in &chunk_tokens {
            if query_set.contains(t) {
                matches += 1;
                term_freq += 1;
            }
        }

        if matches > 0 {
            // Simplified BM25 / TF-IDF score
            let tf = term_freq as f32 / chunk_tokens.len() as f32;
            let coverage = matches as f32 / query_tokens.len() as f32;
            let score = coverage * 2.0 + tf * 10.0;

            scored.push(ScoredChunk {
                chunk: chunk.clone(),
                score,
            });
        }
    }

    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    scored
}

/// Converts scored chunks into structured citations for the UI.
pub fn format_citations(chunks: &[ScoredChunk]) -> Vec<Citation> {
    chunks
        .iter()
        .enumerate()
        .map(|(i, sc)| {
            let passage = if sc.chunk.content.len() > 200 {
                format!("{}…", &sc.chunk.content[..200])
            } else {
                sc.chunk.content.clone()
            };
            Citation {
                index: i + 1,
                file_name: sc.chunk.file_name.clone(),
                page: sc.chunk.page,
                passage: Some(passage),
            }
        })
        .collect()
}
