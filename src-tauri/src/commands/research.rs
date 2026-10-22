//! Document analysis commands: summarize, extract tables, compare documents,
//! flag inconsistencies, and save outputs to the project folder.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::activity::log as activity;
use crate::error::{AppError, AppResult};
use crate::ingest::parsers::parse_file;
use crate::models::registry as models;
use crate::projects::layout::ProjectPaths;
use crate::runtime::client;
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
    let target_path = root.join(&request.file_name);
    let doc_text = parse_file(&target_path)
        .map_err(|e| AppError::msg(format!("Failed to read {}: {e}", request.file_name)))?;

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
            crate::agent::mapreduce::map_reduce_summarize(&serving, &model_label, &doc_text).await?
        }
        "extract_tables" => {
            let messages = vec![
                client::message("system", "You are an expert at extracting structured tables from text."),
                client::message("user", crate::agent::prompts::extract_tables_prompt(&doc_text)),
            ];
            client::complete(&serving, &model_label, &messages).await?
        }
        "flag_inconsistencies" => {
            let messages = vec![
                client::message("system", "You are a meticulous fact and logic auditor."),
                client::message("user", crate::agent::prompts::flag_inconsistencies_prompt(&doc_text)),
            ];
            client::complete(&serving, &model_label, &messages).await?
        }
        "compare" => {
            let second_name = request.compare_with.as_deref().unwrap_or("");
            let second_path = root.join(second_name);
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
            client::complete(&serving, &model_label, &messages).await?
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
