//! Document analysis commands: summarize, extract tables, compare documents,
//! flag inconsistencies, and save outputs to the project folder.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::activity::log as activity;
use crate::agent::mapreduce::map_reduce_summarize;
use crate::error::{AppError, AppResult};
use crate::ingest::parsers::parse_file;
use crate::models::registry as models;
use crate::projects::layout::ProjectPaths;
use crate::runtime::client::{self, SamplingParams};
use crate::runtime::profile::InferenceProfile;
use crate::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisResult {
    pub content: String,
    pub saved_to_file: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisRequest {
    pub project_id: String,
    /// Action: "summarize" | "extract_tables" | "flag_inconsistencies" | "compare"
    pub action: String,
    /// Primary file relative path
    pub file_name: String,
    /// Secondary file relative path (only for "compare")
    pub compare_with: Option<String>,
    /// Optional filename to save under outputs/ (e.g. "summary.md")
    pub save_as: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummarizationRequest {
    pub project_id: String,
    pub file_path: String,
    pub prompt: Option<String>,
    pub profile: InferenceProfile,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummarizationResult {
    pub summary: String,
    pub tokens_used: TokenUsage,
    pub saved_to_file: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input: u32,
    pub output: u32,
}

#[tauri::command]
pub async fn analyze_document(
    state: State<'_, AppState>,
    request: AnalysisRequest,
) -> AppResult<AnalysisResult> {
    let (record, settings) = {
        let registry = state.registry()?;
        let record = registry
            .find(&request.project_id)
            .cloned()
            .ok_or_else(|| AppError::UnknownProject(request.project_id.clone()))?;
        (record, registry.settings.clone())
    };

    let root = PathBuf::from(&record.path);
    let target_path = crate::sandbox::paths::ensure_within(&root, std::path::Path::new(&request.file_name))?;

    // Extract document text (PDF extraction can take several seconds; run in
    // a blocking task so we don't stall the async runtime, and log progress).
    let ext = target_path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    let is_pdf = ext == "pdf";

    if is_pdf {
        log::info!("[analyze_document] extracting text from PDF: {}", request.file_name);
    }
    let doc_text = tokio::task::spawn_blocking(move || parse_file(&target_path))
        .await
        .map_err(|e| AppError::msg(format!("PDF extraction task failed: {e}")))?
        .map_err(|e| AppError::msg(format!("Failed to read {}: {e}", request.file_name)))?;
    if is_pdf {
        log::info!("[analyze_document] finished extracting PDF text ({} chars)", doc_text.len());
    }

    let model = models::selected(&state.dirs, &settings).ok_or(AppError::NoModelSelected)?;

    let serving = crate::commands::runtime::ensure(&state, &settings, Some(&request.project_id))?;

    if let Err(error) = client::wait_until_ready(&serving).await {
        if let Ok(mut supervisor) = state.supervisor() {
            supervisor.mark_failed(error.to_string());
        }
        return Err(error);
    }

    let model_label = models::label(&model);

    let result_content = match request.action.as_str() {
        "summarize" => {
            let profile = settings.profile_for(Some(&model), Some(&request.project_id));
            let (summary, _usage) = crate::agent::mapreduce::map_reduce_summarize(
                &serving,
                &model_label,
                profile.context_tokens,
                &doc_text,
            )
            .await?;
            summary
        }
        "extract_tables" => {
            let messages = vec![
                client::message("system", "You are an expert at extracting structured tables from text."),
                client::message("user", crate::agent::prompts::extract_tables_prompt(&doc_text)),
            ];
            client::complete(&serving, &model_label, &messages, SamplingParams::default()).await?
        }
        "flag_inconsistencies" => {
            let messages = vec![
                client::message("system", "You are a meticulous fact and logic auditor."),
                client::message("user", crate::agent::prompts::flag_inconsistencies_prompt(&doc_text)),
            ];
            client::complete(&serving, &model_label, &messages, SamplingParams::default()).await?
        }
        "compare" => {
            let second_name = request.compare_with.as_deref().unwrap_or("");
            let second_path = crate::sandbox::paths::ensure_within(&root, std::path::Path::new(second_name))?;
            let second_text = parse_file(&second_path)
                .map_err(|e| AppError::msg(format!("Failed to read comparison file {second_name}: {e}")))?;

            let messages = vec![
                client::message("system", "You compare documents and clearly outline all differences."),
                client::message(
                    "user",
                    crate::agent::prompts::compare_documents_prompt(
                        &request.file_name,
                        &doc_text,
                        second_name,
                        &second_text,
                    ),
                ),
            ];
            client::complete(&serving, &model_label, &messages, SamplingParams::default()).await?
        }
        other => return Err(AppError::msg(format!("Unknown document analysis action: {other}"))),
    };

    // Save to outputs/ if requested
    let mut saved_to_file = None;
    if let Some(filename) = request.save_as {
        let clean_name = filename.trim().replace(['\\', '/'], "_");
        if !clean_name.is_empty() {
            let outputs_dir = root.join("outputs");
            let _ = std::fs::create_dir_all(&outputs_dir);
            let out_file = outputs_dir.join(&clean_name);
            if std::fs::write(&out_file, &result_content).is_ok() {
                saved_to_file = Some(format!("outputs/{clean_name}"));
            }
        }
    }

    let _ = activity::record(
        &ProjectPaths::new(record.path),
        activity::ActivityKind::Local,
        "analyze_document",
        &format!("{}: {}", request.action, request.file_name),
    );

    Ok(AnalysisResult {
        content: result_content,
        saved_to_file,
    })
}

#[tauri::command]
pub async fn summarize_document(
    app: AppHandle,
    state: State<'_, AppState>,
    request: SummarizationRequest,
) -> AppResult<SummarizationResult> {
    let (record, settings) = {
        let registry = state.registry()?;
        let record = registry
            .find(&request.project_id)
            .cloned()
            .ok_or_else(|| AppError::UnknownProject(request.project_id.clone()))?;
        (record, registry.settings.clone())
    };

    let root = PathBuf::from(&record.path);
    let target_path = crate::sandbox::paths::ensure_within(&root, std::path::Path::new(&request.file_path))?;

    // Extract document text (PDF extraction can take several seconds; run in
    // a blocking task so we don't stall the async runtime, and log progress).
    let ext = target_path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    let is_pdf = ext == "pdf";

    if is_pdf {
        log::info!("[summarize_document] extracting text from PDF: {}", request.file_path);
    }
    let doc_text = tokio::task::spawn_blocking(move || parse_file(&target_path))
        .await
        .map_err(|e| AppError::msg(format!("PDF extraction task failed: {e}")))?
        .map_err(|e| AppError::msg(format!("Failed to read {}: {e}", request.file_path)))?;
    if is_pdf {
        log::info!("[summarize_document] finished extracting PDF text ({} chars)", doc_text.len());
        // Notify frontend that extraction is done, summarization starting
        let _ = app.emit("summarize:extraction_done", ());
    }

    let model = models::selected(&state.dirs, &settings).ok_or(AppError::NoModelSelected)?;

    let serving = crate::commands::runtime::ensure(&state, &settings, Some(&request.project_id))?;

    if let Err(error) = client::wait_until_ready(&serving).await {
        if let Ok(mut supervisor) = state.supervisor() {
            supervisor.mark_failed(error.to_string());
        }
        return Err(error);
    }

    let model_label = models::label(&model);

    // Always use map-reduce: chunk the document, summarize each chunk,
    // then synthesize. Works for any document length without context window
    // issues. Token usage is accumulated across all internal model calls.
    log::info!(
        "[summarize_document] summarizing via map-reduce ({} chars)",
        doc_text.len()
    );
    let (summary, usage) = map_reduce_summarize(
        &serving,
        &model_label,
        request.profile.context_tokens,
        &doc_text,
    )
    .await?;

    log::info!("[summarize_document] summary length: {} chars", summary.len());

    // Print token counts to the terminal logs so summarisation (including PDFs)
    // is observable server-side.
    log::info!(
        "[summarize_document] {} — prompt_tokens={}, completion_tokens={}, total_tokens={}",
        request.file_path,
        usage.prompt_tokens,
        usage.completion_tokens,
        usage.total_tokens
    );

    // Save the summary to the project's summary/ folder so it is persisted
    // even if the frontend auto-save path fails. Derive the markdown file
    // name from the source document (e.g. data/foo.pdf -> summary/foo.md).
    let mut saved_to_file = None;
    let source_name = Path::new(&request.file_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&request.file_path);
    let dot_index = source_name.rfind('.');
    let stem = dot_index.map(|i| &source_name[..i]).unwrap_or(source_name);
    let md_file_name = format!("{stem}.md");
    let summary_dir = ProjectPaths::new(&record.path).summary_dir();
    if std::fs::create_dir_all(&summary_dir).is_ok() {
        let summary_path = summary_dir.join(&md_file_name);
        if std::fs::write(&summary_path, &summary).is_ok() {
            saved_to_file = Some(format!("summary/{md_file_name}"));
            log::info!(
                "[summarize_document] saved summary to {}",
                summary_path.display()
            );
        } else {
            log::warn!(
                "[summarize_document] failed to write summary to {}",
                summary_path.display()
            );
        }
    } else {
        log::warn!(
            "[summarize_document] failed to create summary directory {}",
            summary_dir.display()
        );
    }

    let _ = activity::record(
        &ProjectPaths::new(record.path),
        activity::ActivityKind::Local,
        "summarize_document",
        &request.file_path,
    );

    Ok(SummarizationResult {
        summary,
        tokens_used: TokenUsage {
            input: usage.prompt_tokens,
            output: usage.completion_tokens,
        },
        saved_to_file,
    })
}

#[tauri::command]
pub fn create_research_plan(
    state: State<'_, AppState>,
    project_id: String,
    topic: String,
) -> AppResult<crate::agent::plan::ResearchPlan> {
    let registry = state.registry()?;
    let record = registry
        .find(&project_id)
        .ok_or_else(|| AppError::UnknownProject(project_id.clone()))?;

    Ok(crate::agent::plan::create_starter_plan(&topic, record.internet_enabled))
}

#[tauri::command]
pub async fn execute_research_plan(
    state: State<'_, AppState>,
    project_id: String,
    mut plan: crate::agent::plan::ResearchPlan,
) -> AppResult<crate::agent::r#loop::ResearchReportResult> {
    let (record, settings) = {
        let registry = state.registry()?;
        let record = registry
            .find(&project_id)
            .cloned()
            .ok_or_else(|| AppError::UnknownProject(project_id.clone()))?;
        (record, registry.settings.clone())
    };

    let root = PathBuf::from(&record.path);
    let model = models::selected(&state.dirs, &settings).ok_or(AppError::NoModelSelected)?;

    let serving = crate::commands::runtime::ensure(&state, &settings, Some(&project_id))?;

    if let Err(error) = client::wait_until_ready(&serving).await {
        if let Ok(mut supervisor) = state.supervisor() {
            supervisor.mark_failed(error.to_string());
        }
        return Err(error);
    }

    let model_label = models::label(&model);

    crate::agent::r#loop::execute_research(
        &root,
        &serving,
        &model_label,
        &mut plan,
        record.internet_enabled,
    )
    .await
}

#[tauri::command]
pub async fn run_deep_research(
    app: tauri::AppHandle,
    question: String,
) -> Result<i64, String> {
    use crate::research::orchestrator;
    use tokio::sync::mpsc;
    let (tx, mut rx) = mpsc::channel::<crate::research::ResearchEvent>(64);
    let handle = app.clone();
    tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            let _ = handle.emit("research:event", ev);
        }
    });
    orchestrator::run_once(&question, tx).await
}
