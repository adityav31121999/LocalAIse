use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub chunk_index: usize,
    pub file_name: String,
    pub content: String,
    pub page: Option<u32>,
}

/// Splits text into overlapping character chunks preserving word boundaries.
pub fn chunk_text(file_name: &str, text: &str, target_chunk_size: usize, overlap: usize) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let text = text.trim();
    if text.is_empty() {
        return chunks;
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return chunks;
    }

    let mut start_idx = 0;
    let mut chunk_index = 0;

    while start_idx < words.len() {
        let mut current_words = Vec::new();
        let mut current_len = 0;
        let mut next_idx = start_idx;

        while next_idx < words.len() && (current_len < target_chunk_size || current_words.is_empty()) {
            current_words.push(words[next_idx]);
            current_len += words[next_idx].len() + 1;
            next_idx += 1;
        }

        chunks.push(Chunk {
            chunk_index,
            file_name: file_name.to_string(),
            content: current_words.join(" "),
            page: None,
        });
        chunk_index += 1;

        if next_idx >= words.len() {
            break;
        }

        // Calculate step considering overlap
        let overlap_words = (overlap / 6).max(1);
        start_idx = if next_idx > overlap_words && (next_idx - overlap_words) > start_idx {
            next_idx - overlap_words
        } else {
            next_idx
        };
    }

    chunks
}
