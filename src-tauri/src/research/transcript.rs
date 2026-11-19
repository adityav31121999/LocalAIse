//! Full audit trail for a deep-research run.
//!
//! Every research run produces a `research-{id}.json` file in the project's
//! `chats/` directory containing everything the pipeline did: the system
//! prompts sent to the model, the plan the model produced, every tool action
//! (searches, fetches, local lookups) and every LLM response, plus the final
//! report text. The report is also written separately as a markdown file in
//! `outputs/`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::research::llm::{LlmClient, LlmMessage, LlmParams, LlmResponse};
use crate::research::error::ResearchResult;

/// Which source a plan step draws from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepSource {
    /// Project documents.
    Local,
    /// Web search.
    Internet,
}

impl std::fmt::Display for StepSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StepSource::Local => write!(f, "local"),
            StepSource::Internet => write!(f, "internet"),
        }
    }
}

/// One step of the model-produced research plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    pub id: usize,
    pub question: String,
    pub source: StepSource,
    #[serde(default)]
    pub search_queries: Vec<String>,
    #[serde(default)]
    pub answer_criteria: String,
}

/// The plan as the model returned it (parsed), plus the raw prompt/response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRecord {
    pub system_prompt: String,
    pub user_prompt: String,
    pub raw_response: String,
    pub steps: Vec<PlanStep>,
}

/// One recorded action or model call, in the order it happened.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptAction {
    pub ts: String,
    /// `llm_call` | `local_search` | `search` | `fetch` | `plan` | `verify` | `report`
    pub kind: String,
    /// Index into `plan.steps` this action belongs to, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<usize>,
    pub input: serde_json::Value,
    pub output: serde_json::Value,
}

/// The complete record of one research run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchTranscript {
    /// Sequential id, e.g. `research-001`.
    pub id: String,
    pub created_at: String,
    pub question: String,
    pub mode: String,
    pub internet_enabled: bool,
    /// System prompt of the planner (the primary system prompt for the run).
    /// All other system prompts are recorded per `llm_call` action.
    pub system_prompt: String,
    pub plan: PlanRecord,
    pub actions: Vec<TranscriptAction>,
    /// The full final report markdown (also saved separately in outputs/).
    pub final_report: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_path: Option<String>,
}

/// Shared, ordered log of actions. Cloned into the recording LLM client and
/// the pipeline context so entries interleave correctly.
pub type ActionLog = Arc<Mutex<Vec<TranscriptAction>>>;

/// Shared cell holding the index of the plan step currently executing, so
/// LLM calls made inside a step are attributed to it.
pub type CurrentStep = Arc<Mutex<Option<usize>>>;

pub fn new_action_log() -> ActionLog {
    Arc::new(Mutex::new(Vec::new()))
}

pub fn new_current_step() -> CurrentStep {
    Arc::new(Mutex::new(None))
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

/// Append an action to the log.
pub fn record(log: &ActionLog, kind: &str, step: Option<usize>, input: serde_json::Value, output: serde_json::Value) {
    if let Ok(mut entries) = log.lock() {
        entries.push(TranscriptAction {
            ts: now_iso(),
            kind: kind.to_string(),
            step,
            input,
            output,
        });
    }
}

/// LLM client wrapper that records every call (system prompt, messages,
/// raw response, token usage) into the shared action log.
pub struct RecordingLlmClient<C: LlmClient> {
    inner: C,
    log: ActionLog,
    current_step: CurrentStep,
}

impl<C: LlmClient> RecordingLlmClient<C> {
    pub fn new(inner: C, log: ActionLog, current_step: CurrentStep) -> Self {
        Self { inner, log, current_step }
    }
}

#[async_trait]
impl<C: LlmClient + 'static> LlmClient for RecordingLlmClient<C> {
    async fn complete(&self, messages: Vec<LlmMessage>, params: LlmParams) -> ResearchResult<LlmResponse> {
        let system_prompt = messages
            .iter()
            .find(|m| m.role == "system")
            .map(|m| m.content.clone())
            .unwrap_or_default();
        let step = self.current_step.lock().ok().and_then(|s| *s);
        let input = serde_json::json!({
            "system_prompt": system_prompt,
            "messages": messages,
            "params": {
                "temperature": params.temperature,
                "top_p": params.top_p,
                "max_tokens": params.max_tokens,
                "json_schema": params.json_schema,
            },
        });
        let result = self.inner.complete(messages, params).await;
        match &result {
            Ok(resp) => record(&self.log, "llm_call", step, input, serde_json::json!({
                "content": resp.content,
                "prompt_tokens": resp.prompt_tokens,
                "completion_tokens": resp.completion_tokens,
                "model": self.inner.model_id(),
            })),
            Err(e) => record(&self.log, "llm_call", step, input, serde_json::json!({
                "error": e.to_string(),
            })),
        }
        result
    }

    fn model_id(&self) -> &str {
        self.inner.model_id()
    }

    fn input_cost_per_m(&self) -> f64 {
        self.inner.input_cost_per_m()
    }

    fn output_cost_per_m(&self) -> f64 {
        self.inner.output_cost_per_m()
    }
}

/// Next sequential research id for a chats directory: `research-001`, …
pub fn next_research_id(chats_dir: &Path) -> String {
    let max = std::fs::read_dir(chats_dir)
        .map(|read_dir| {
            read_dir
                .flatten()
                .filter_map(|entry| entry.path().file_stem()?.to_str().map(|s| s.to_string()))
                .filter_map(|name| {
                    name.strip_prefix("research-")
                        .and_then(|n| n.parse::<u32>().ok())
                })
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0);
    format!("research-{:03}", max + 1)
}

/// The `chats/research-{id}.json` path for a project.
pub fn transcript_path(chats_dir: &Path, id: &str) -> PathBuf {
    chats_dir.join(format!("{id}.json"))
}

/// Write the transcript JSON into the project's chats directory.
pub fn save_transcript(chats_dir: &Path, transcript: &ResearchTranscript) -> Result<PathBuf, String> {
    std::fs::create_dir_all(chats_dir).map_err(|e| e.to_string())?;
    let path = transcript_path(chats_dir, &transcript.id);
    let raw = serde_json::to_string_pretty(transcript).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Structured inputs for `export_research_report`: the written sections and the
/// sources they cite, captured at the end of a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPayload {
    pub sections: Vec<crate::research::report_writer::WrittenSection>,
    pub sources: Vec<crate::research::store::Source>,
}

/// The `chats/research-{id}.export.json` path for a project.
pub fn export_payload_path(chats_dir: &Path, id: &str) -> PathBuf {
    chats_dir.join(format!("{id}.export.json"))
}

/// Persist the structured export inputs for a finished run. Kept separate from
/// the transcript because the transcript stores the report as flat markdown.
pub fn save_export_payload(
    chats_dir: &Path,
    id: &str,
    sections: &[crate::research::report_writer::WrittenSection],
    sources: &[crate::research::store::Source],
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(chats_dir).map_err(|e| e.to_string())?;
    let path = export_payload_path(chats_dir, id);
    let payload = ExportPayload {
        sections: sections.to_vec(),
        sources: sources.to_vec(),
    };
    let raw = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Load a previously saved export payload.
pub fn load_export_payload(chats_dir: &Path, id: &str) -> Result<ExportPayload, String> {
    let path = export_payload_path(chats_dir, id);
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("Export data for {id} not found at {}: {e}", path.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("Export data for {id} is corrupt: {e}"))
}
