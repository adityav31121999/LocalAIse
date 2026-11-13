use serde::Serialize;

#[derive(Serialize, Clone)]
#[serde(tag = "type")]
pub enum ResearchEvent {
    Planned { sub_questions: Vec<String> },
    Searching { query: String },
    Fetched { url: String, tier: String, chars: usize },
    FactExtracted { sub_q: usize, source_id: i64, claim: String },
    GapFound { missing: Vec<String> },
    SectionWritten { title: String, markdown: String, confidence: Option<f32>, citation_count: Option<usize> },
    Verified { 
        supported: usize, 
        unsupported: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        stage1_passes: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stage2_supported: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stage2_contradicted: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stage2_unsupported: Option<usize>,
    },
    SectionConfidence { title: String, confidence: f32, citation_count: usize },
    Notice { message: String },
    Done { report_id: i64, report_path: Option<String> },
    Error { message: String },
}