use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDocumentRecord {
    pub file_name: String,
    pub total_chunks: usize,
    pub modified_at: String,
}
