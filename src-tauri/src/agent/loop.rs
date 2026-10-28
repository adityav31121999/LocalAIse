//! Deep research agent execution loop.
//! Respects hard step limits, local-first search, network toggle, and produces a final cited report.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::plan::ResearchPlan;
use crate::error::AppResult;
use crate::runtime::client::{self, SamplingParams};
use crate::runtime::Serving;
use crate::tools::ToolResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchReportResult {
    pub topic: String,
    pub report_markdown: String,
    pub saved_to_file: Option<String>,
    pub sources_used: Vec<String>,
}

/// Executes approved research plan steps.
pub async fn execute_research(
    project_root: &Path,
    serving: &Serving,
    model_label: &str,
    plan: &mut ResearchPlan,
    internet_enabled: bool,
) -> AppResult<ResearchReportResult> {
    plan.status = "running".to_string();
    let mut collected_context = Vec::new();
    let mut sources_used = Vec::new();

    let index_db = crate::index::db::IndexDb::new(project_root);
    let project_index = index_db.load();

    for step in &mut plan.steps {
        if step.source_type == "local" && step.query != "synthesize" {
            let matches = crate::retrieval::search::search_chunks(&project_index, &step.query, 3);
            let mut findings = Vec::new();
            for m in matches {
                findings.push(format!("From {}: {}", m.chunk.file_name, m.chunk.content));
                if !sources_used.contains(&m.chunk.file_name) {
                    sources_used.push(m.chunk.file_name);
                }
            }
            let step_findings = findings.join("\n\n");
            step.findings = Some(step_findings.clone());
            step.completed = true;
            collected_context.push(step_findings);
        } else if step.source_type == "web" && internet_enabled {
            // Real web search using DuckDuckGo Lite + fetch top results
            let search_result = crate::tools::web_search::search(project_root, &step.query, true).await;
            
            let content = match search_result {
                ToolResult { error: Some(e), .. } => {
                    format!("Web search failed: {}", e)
                }
                ToolResult { output, .. } => {
                    // The search result now contains formatted text with URLs
                    // We also need to fetch the actual content from top URLs
                    // Parse the search output to extract URLs
                    let urls = extract_urls_from_search_output(&output);
                    
                    let mut fetched_contents = Vec::new();
                    for (i, url) in urls.iter().take(3).enumerate() {
                        let fetch_result = crate::tools::web_fetch::fetch(project_root, url, true).await;
                        match fetch_result {
                            ToolResult { error: Some(e), .. } => {
                                fetched_contents.push(format!("[Source {}] {} (fetch failed: {})", i + 1, url, e));
                            }
                            ToolResult { output: fetch_output, .. } => {
                                sources_used.push(url.clone());
                                fetched_contents.push(format!("[Source {}] {}\n{}", i + 1, url, fetch_output));
                            }
                        }
                    }
                    
                    if fetched_contents.is_empty() {
                        format!("Web search results (no content fetched):\n{}", output)
                    } else {
                        format!("Web search results:\n{}\n\n---\nFetched content:\n{}", 
                            output, fetched_contents.join("\n\n---\n\n"))
                    }
                }
            };

            step.findings = Some(content.clone());
            step.completed = true;
            collected_context.push(content);
        }
    }

    // Synthesis phase: generate final research report
    let prompt = format!(
        "Synthesize a thorough, detailed, well-structured research report with citations on the topic: “{}”.\n\n\
        Collected Evidence:\n{}\n\n\
        Format your response in clean Markdown with an executive summary, core analysis, key takeaways, and a sources list.",
        plan.topic,
        collected_context.join("\n\n---\n\n")
    );

    let messages = vec![
        client::message("system", "You are an autonomous private research analyst producing grounded reports."),
        client::message("user", prompt),
    ];

    let report_markdown = client::complete(serving, model_label, &messages, SamplingParams::default()).await?;
    plan.status = "completed".to_string();

    // Save report to outputs/
    let filename = format!("research-{}.md", plan.topic.replace(|c: char| !c.is_alphanumeric(), "_").to_lowercase());
    let outputs_dir = project_root.join("outputs");
    let _ = std::fs::create_dir_all(&outputs_dir);
    let output_path = outputs_dir.join(&filename);
    let _ = std::fs::write(&output_path, &report_markdown);

    Ok(ResearchReportResult {
        topic: plan.topic.clone(),
        report_markdown,
        saved_to_file: Some(format!("outputs/{filename}")),
        sources_used,
    })
}

/// Extract URLs from the formatted search output.
/// The output format is: "N. [Title](URL)\n   Snippet"
fn extract_urls_from_search_output(output: &str) -> Vec<String> {
    let mut urls = Vec::new();
    for line in output.lines() {
        // Look for markdown link pattern: [Title](URL)
        if let Some(start) = line.find("](") {
            let url_start = start + 2;
            if let Some(end) = line[url_start..].find(')') {
                let url = &line[url_start..url_start + end];
                if url.starts_with("http") {
                    urls.push(url.to_string());
                }
            }
        }
    }
    urls
}
