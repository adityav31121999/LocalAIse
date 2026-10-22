use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ingest::chunk::Chunk;

pub const INDEX_FILE: &str = "index.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectIndex {
    pub chunks: Vec<Chunk>,
    /// File path -> modification timestamp or hash
    pub indexed_files: HashMap<String, String>,
}

pub struct IndexDb {
    index_path: PathBuf,
}

impl IndexDb {
    pub fn new(project_dir: &Path) -> Self {
        let localaise_dir = project_dir.join(".LocalAIse");
        let _ = fs::create_dir_all(&localaise_dir);
        Self {
            index_path: localaise_dir.join(INDEX_FILE),
        }
    }

    pub fn load(&self) -> ProjectIndex {
        if let Ok(content) = fs::read_to_string(&self.index_path) {
            if let Ok(index) = serde_json::from_str(&content) {
                return index;
            }
        }
        ProjectIndex::default()
    }

    pub fn save(&self, index: &ProjectIndex) -> Result<(), String> {
        let content = serde_json::to_string(index).map_err(|e| e.to_string())?;
        fs::write(&self.index_path, content).map_err(|e| e.to_string())
    }

    pub fn insert_chunks(&self, file_name: &str, file_stamp: &str, new_chunks: Vec<Chunk>) -> Result<(), String> {
        let mut index = self.load();
        // Remove existing chunks for this file
        index.chunks.retain(|c| c.file_name != file_name);
        index.chunks.extend(new_chunks);
        index.indexed_files.insert(file_name.to_string(), file_stamp.to_string());
        self.save(&index)
    }

    pub fn remove_file(&self, file_name: &str) -> Result<(), String> {
        let mut index = self.load();
        index.chunks.retain(|c| c.file_name != file_name);
        index.indexed_files.remove(file_name);
        self.save(&index)
    }
}
