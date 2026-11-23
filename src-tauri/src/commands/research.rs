//! Deep research commands (plan/run/stop/export) plus document analysis
//! commands (summarize, extract tables, compare, flag inconsistencies).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::activity::log as activity;
use crate::agent::mapreduce::map_reduce_summarize;
use crate::error::{AppError, AppResult};
use crate::ingest::parsers::parse_file;
use crate::models::registry as models;
use crate::projects::layout::{ProjectPaths, SUMMARY_DIR};
use crate::runtime::client::{self, SamplingParams};
use crate::runtime::profile::InferenceProfile;
use crate::research::llm_adapter::RuntimeLlmClient;
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
    /// Optional filename to save under `outputs/` (`outputs/summary/` for the
    /// summarize action), e.g. "summary.md".
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

    let serving = crate::commands::runtime::ensure(&state, &settings, None)?;

    if let Err(error) = client::wait_until_ready(&serving).await {
        if let Ok(mut supervisor) = state.supervisor() {
            supervisor.mark_failed(error.to_string());
        }
        return Err(error);
    }

    let model_label = models::label(&model);

    let result_content = match request.action.as_str() {
        "summarize" => {
            let profile = settings.performance;
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
            let task = crate::agent::prompts::Task::ExtractTables;
            let messages = vec![
                client::message("system", crate::agent::prompts::system_prompt(&task)),
                client::message("user", crate::agent::prompts::user_message(&task, &[("document", &doc_text)])),
            ];
            let sampling = SamplingParams {
                temperature: task.temperature(),
                ..Default::default()
            };
            let output = client::complete(&serving, &model_label, &messages, sampling).await?;
            crate::agent::mapreduce::clean_contract_violations(&output)
        }
        "flag_inconsistencies" => {
            let task = crate::agent::prompts::Task::Inconsistencies;
            let messages = vec![
                client::message("system", crate::agent::prompts::system_prompt(&task)),
                client::message("user", crate::agent::prompts::user_message(&task, &[("document", &doc_text)])),
            ];
            let sampling = SamplingParams {
                temperature: task.temperature(),
                ..Default::default()
            };
            let output = client::complete(&serving, &model_label, &messages, sampling).await?;
            crate::agent::mapreduce::clean_contract_violations(&output)
        }
        "compare" => {
            let second_name = request.compare_with.as_deref().unwrap_or("B");
            let second_path = crate::sandbox::paths::ensure_within(&root, std::path::Path::new(second_name))?;
            let second_text = parse_file(&second_path)
                .map_err(|e| AppError::msg(format!("Failed to read comparison file {second_name}: {e}")))?;

            let task = crate::agent::prompts::Task::Compare;
            let messages = vec![
                client::message("system", crate::agent::prompts::system_prompt(&task)),
                client::message(
                    "user",
                    crate::agent::prompts::user_message(
                        &task,
                        &[("A", &doc_text), ("B", &second_text)],
                    ),
                ),
            ];
            let sampling = SamplingParams {
                temperature: task.temperature(),
                ..Default::default()
            };
            let output = client::complete(&serving, &model_label, &messages, sampling).await?;
            crate::agent::mapreduce::clean_contract_violations(&output)
        }
        other => return Err(AppError::msg(format!("Unknown document analysis action: {other}"))),
    };

    // Save to outputs/ if requested. Summaries go to outputs/summary/; every
    // other analysis stays directly in outputs/.
    let mut saved_to_file = None;
    if let Some(filename) = request.save_as {
        let clean_name = filename.trim().replace(['\\', '/'], "_");
        if !clean_name.is_empty() {
            let summary_subdir = (request.action == "summarize").then_some(SUMMARY_DIR);
            let outputs_dir = root.join("outputs");
            let target_dir = match summary_subdir {
                Some(subdir) => outputs_dir.join(subdir),
                None => outputs_dir,
            };
            let _ = std::fs::create_dir_all(&target_dir);
            let out_file = target_dir.join(&clean_name);
            if std::fs::write(&out_file, &result_content).is_ok() {
                saved_to_file = Some(match summary_subdir {
                    Some(subdir) => format!("outputs/{subdir}/{clean_name}"),
                    None => format!("outputs/{clean_name}"),
                });
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

    let serving = crate::commands::runtime::ensure(&state, &settings, None)?;

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

    // Save the summary under outputs/summary/ so it is persisted even if the
    // frontend auto-save path fails. Derive the markdown file name from the
    // source document (e.g. data/foo.pdf -> outputs/summary/foo.md).
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
            saved_to_file = Some(format!("outputs/{SUMMARY_DIR}/{md_file_name}"));
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

    let serving = crate::commands::runtime::ensure(&state, &settings, None)?;

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
#[allow(clippy::too_many_arguments)]
pub async fn run_deep_research(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    question: String,
    mode: crate::research::request::ResearchMode,
    project_id: String,
    provided_urls: Option<Vec<String>>,
    use_browser: Option<bool>,
    budget_preset: Option<String>, // "quick" | "standard" | "deep"
    plan: Option<crate::research::transcript::PlanRecord>,
) -> Result<i64, String> {
    use crate::research::net::ResearchNet;
    use crate::research::orchestrator;
    use crate::research::request::ResearchRequest;
    use crate::research::budget::Budgets;
    use tokio::sync::mpsc;

    let (record, settings) = {
        let registry = state.registry().map_err(|e| e.to_string())?;
        let record = registry
            .find(&project_id)
            .cloned()
            .ok_or_else(|| "unknown project".to_string())?;
        (record, registry.settings.clone())
    };

    // One run per project: a second run would reuse the same transcript id and
    // overwrite the same report file while the first is still working.
    if crate::agent::cancel::research_is_active(&project_id) {
        return Err("A deep-research run is already active for this project".to_string());
    }

    let model = models::selected(&state.dirs, &settings).ok_or("No model selected".to_string())?;
    let model_label = models::label(&model);

    let serving = crate::commands::runtime::ensure(&state, &settings, None)
        .map_err(|e| e.to_string())?;

    if let Err(error) = client::wait_until_ready(&serving).await {
        if let Ok(mut supervisor) = state.supervisor() {
            supervisor.mark_failed(error.to_string());
        }
        return Err(error.to_string());
    }

    let req = ResearchRequest {
        question,
        mode,
        project_root: Some(std::path::PathBuf::from(&record.path)),
        provided_urls: provided_urls.unwrap_or_default(),
        use_browser: use_browser.unwrap_or(true),
        internet_enabled: record.internet_enabled,
    };
    let net = ResearchNet::new(
        std::path::PathBuf::from(&record.path),
        record.internet_enabled,
    );

    let llm = RuntimeLlmClient::new(serving, model_label);

    let (tx, mut rx) = mpsc::channel::<crate::research::ResearchEvent>(64);
    let tx_err = tx.clone();
    let handle = app.clone();
    tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            let _ = handle.emit("research:event", ev);
        }
    });

    // Select budget preset
    let budgets = match budget_preset.as_deref() {
        Some("quick") => Budgets::quick(),
        Some("deep") => Budgets::deep(),
        _ => Budgets::standard(),
    };

    // Registers this run so `research_stop` can halt it between steps and fetches.
    let run_guard = crate::agent::cancel::ResearchRunGuard::begin(&project_id);
    let cancel_flag = run_guard.flag();

    // A plan reviewed in the UI is executed as-is (without re-planning and
    // without re-emitting `Planned`); with no plan the pipeline plans itself.
    let result = match plan {
        Some(plan_record) => {
            let steps = plan_record.steps.clone();
            orchestrator::run_with_plan(llm, req, &net, tx, budgets, Some(cancel_flag), plan_record, steps, false).await
        }
        None => orchestrator::run(llm, req, &net, tx, budgets, Some(cancel_flag)).await,
    };
    if let Err(e) = &result {
        let _ = tx_err.send(crate::research::ResearchEvent::Error { message: e.to_string() }).await;
    }
    result.map_err(|e| e.to_string())
}

/// Produce the deep-research plan (ordered steps tagged `local`/`internet`)
/// without executing it. The frontend shows it for review, then passes the
/// approved plan back to `run_deep_research`.
#[tauri::command]
pub async fn plan_deep_research(
    state: State<'_, AppState>,
    question: String,
    mode: crate::research::request::ResearchMode,
    project_id: String,
) -> Result<crate::research::transcript::PlanRecord, String> {
    use crate::research::request::ResearchMode;

    let (record, settings) = {
        let registry = state.registry().map_err(|e| e.to_string())?;
        let record = registry
            .find(&project_id)
            .cloned()
            .ok_or_else(|| "unknown project".to_string())?;
        (record, registry.settings.clone())
    };

    let model = models::selected(&state.dirs, &settings).ok_or("No model selected".to_string())?;
    let model_label = models::label(&model);

    let serving = crate::commands::runtime::ensure(&state, &settings, None)
        .map_err(|e| e.to_string())?;

    if let Err(error) = client::wait_until_ready(&serving).await {
        if let Ok(mut supervisor) = state.supervisor() {
            supervisor.mark_failed(error.to_string());
        }
        return Err(error.to_string());
    }

    let llm = RuntimeLlmClient::new(serving, model_label);
    let allow_internet = matches!(mode, ResearchMode::Online) && record.internet_enabled;
    let (plan_record, _steps) = crate::research::roles::plan_research(&llm, &question, allow_internet).await;
    Ok(plan_record)
}

/// Asks the project's current deep-research run to stop. The pipeline halts
/// after the step/fetch in flight and writes a partial report with what it
/// found, without spending further tokens on verification.
#[tauri::command]
pub fn research_stop(project_id: String) {
    crate::agent::cancel::research_cancel(&project_id);
}

/// Re-render a finished run's report in the requested format. Reads the
/// structured export payload captured at the end of the run
/// (`chats/research-NNN.export.json`), so no pipeline work is redone.
/// Writes `outputs/deep-research/research-NNN-export.<ext>` and returns its
/// absolute path.
#[tauri::command]
pub fn export_research_report(
    state: State<'_, AppState>,
    project_id: String,
    report_id: i64,
    format: String,
    citation_style: Option<String>,
) -> Result<String, String> {
    use crate::research::export::{CitationStyle, ExportFormat, ExportOptions, export_report};

    let record = {
        let registry = state.registry().map_err(|e| e.to_string())?;
        registry
            .find(&project_id)
            .cloned()
            .ok_or_else(|| "unknown project".to_string())?
    };

    let format = format.to_ascii_lowercase();
    let format = match format.as_str() {
        "markdown" | "md" => ExportFormat::Markdown,
        "json" => ExportFormat::Json,
        "html" => ExportFormat::Html,
        "notion" => ExportFormat::Notion,
        "text" | "txt" => ExportFormat::Text,
        "csljson" | "csl-json" | "csl_json" => ExportFormat::CslJson,
        other => return Err(format!("Unknown export format: {other}")),
    };
    let citation_style = match citation_style.as_deref().map(str::to_ascii_lowercase).as_deref() {
        None | Some("numeric") => CitationStyle::Numeric,
        Some("apa") => CitationStyle::Apa,
        Some("ieee") => CitationStyle::Ieee,
        Some("chicago") => CitationStyle::Chicago,
        Some("mla") => CitationStyle::Mla,
        Some("harvard") => CitationStyle::Harvard,
        Some(other) => return Err(format!("Unknown citation style: {other}")),
    };

    let chats_dir = ProjectPaths::new(&record.path).chats_dir();
    let research_id = format!("research-{report_id:03}");
    let payload = crate::research::transcript::load_export_payload(&chats_dir, &research_id)?;

    let options = ExportOptions {
        format,
        citation_style,
        include_methodology: true,
        include_sources: true,
        include_confidence: true,
        include_limitations: true,
        include_conflicts: true,
    };
    let rendered = export_report(&payload.sections, &payload.sources, options)
        .map_err(|e| e.to_string())?;

    let ext = match format {
        ExportFormat::Markdown => "md",
        ExportFormat::Text => "txt",
        ExportFormat::Html => "html",
        ExportFormat::Json | ExportFormat::Notion | ExportFormat::CslJson => "json",
    };
    let deep_research_dir = ProjectPaths::new(&record.path).deep_research_dir();
    std::fs::create_dir_all(&deep_research_dir).map_err(|e| e.to_string())?;
    let out_path = deep_research_dir.join(format!("{research_id}-export.{ext}"));
    std::fs::write(&out_path, rendered).map_err(|e| e.to_string())?;

    let _ = activity::record(
        &ProjectPaths::new(record.path),
        activity::ActivityKind::Local,
        "export_research_report",
        &format!("{research_id} as {format:?}"),
    );
    Ok(out_path.to_string_lossy().to_string())
}

/// Reveal a path in the OS file manager (e.g. the report just written).
/// Fire-and-forget: the command succeeds as soon as the reveal is spawned.
#[tauri::command]
pub fn reveal_path(path: String) -> Result<(), String> {
    let target = std::path::PathBuf::from(&path);
    if !target.exists() {
        return Err(format!("Path does not exist: {path}"));
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", target.display()))
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&target)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let dir = target.parent().unwrap_or(target.as_path());
        std::process::Command::new("xdg-open")
            .arg(dir)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

