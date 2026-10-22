use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorDoc {
    pub chunk_index: usize,
    pub file_name: String,
    pub content: String,
    pub page: Option<u32>,
}

/// Simulated lightweight embedding interface or keyword tokenization.
pub fn generate_embedding_stub(_text: &str) -> Vec<f32> {
    // Placeholder 128-d vector for compatibility with vector indexing signatures
    vec![0.0; 128]
}
