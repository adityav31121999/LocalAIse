use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Source { 
    pub id: i64, 
    pub url: String, 
    pub title: String, 
    pub text: String,
    pub quality_score: f32,
}

impl Default for Source {
    fn default() -> Self {
        Self {
            id: 0,
            url: String::new(),
            title: String::new(),
            text: String::new(),
            quality_score: 0.0,
        }
    }
}

pub struct EvidenceStore { sources: Vec<Source>, next: i64 }

impl Default for EvidenceStore { fn default() -> Self { Self::new() } }

impl EvidenceStore {
    pub fn new() -> Self { Self { sources: vec![], next: 1 } }
    
    pub fn add_source(&mut self, url: &str, title: &str, text: &str) -> i64 {
        self.add_source_with_quality(url, title, text, 0.0)
    }
    
    pub fn add_source_with_quality(&mut self, url: &str, title: &str, text: &str, quality_score: f32) -> i64 {
        let id = self.next; self.next += 1;
        self.sources.push(Source { id, url: url.to_string(), title: title.to_string(), text: text.to_string(), quality_score });
        id
    }
    
    pub fn sources(&self) -> &[Source] { &self.sources }
    pub fn find(&self, needle: &str) -> Vec<&Source> {
        let n = needle.to_lowercase();
        self.sources.iter().filter(|s| s.text.to_lowercase().contains(&n)).collect()
    }
}