use serde::Serialize;

#[derive(Serialize, Clone)]
#[serde(tag = "type")]
pub enum ResearchEvent {
    Planned { sub_questions: Vec<String> },
    Searching { query: String },
    Fetched { url: String, tier: String, chars: usize },
    FactExtracted { sub_q: usize, source_id: i64, claim: String },
    GapFound { missing: Vec<String> },
    SectionWritten { title: String, markdown: String },
    Verified { supported: usize, unsupported: usize },
    Done { report_id: i64 },
    Error { message: String },
}