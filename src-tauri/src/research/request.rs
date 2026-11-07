use std::path::PathBuf;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchMode { Offline, Online }

#[derive(Debug, Clone)]
pub struct ResearchRequest {
    pub question: String,
    pub mode: ResearchMode,
    pub project_root: Option<PathBuf>,
    pub provided_urls: Vec<String>,
    pub use_browser: bool,
    pub internet_enabled: bool,
}