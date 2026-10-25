use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Source { pub id: i64, pub url: String, pub title: String, pub text: String }

#[derive(Default)]
pub struct EvidenceStore { sources: Vec<Source>, next: i64 }

impl EvidenceStore {
    pub fn new() -> Self { Self { sources: vec![], next: 1 } }
    pub fn add_source(&mut self, url: &str, title: &str, text: &str) -> i64 {
        let id = self.next; self.next += 1;
        self.sources.push(Source { id, url: url.to_string(), title: title.to_string(), text: text.to_string() });
        id
    }
    pub fn sources(&self) -> &[Source] { &self.sources }
    pub fn find(&self, needle: &str) -> Vec<&Source> {
        let n = needle.to_lowercase();
        self.sources.iter().filter(|s| s.text.to_lowercase().contains(&n)).collect()
    }
}