use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;
use chrono::Utc;
use crate::research::error::ResearchError;
use crate::research::chunk;
use crate::research::fact::Fact;
use crate::research::events::ResearchEvent;
use crate::research::fetch::{BrowserFetcher, Page};
use crate::research::net::ResearchNet;
use crate::research::request::*;
use crate::research::roles::{reader, verify, plan_research, SubQuestion};
use crate::research::store::EvidenceStore;
use crate::research::llm::LlmClient;
use crate::research::search::{SearchResult, SourceScore, CorroboratedFact, cluster_claims};
use crate::research::roles::verify::VerificationLabel;
use crate::research::budget::{Budgets, GapStatus, check_gaps, all_subquestions_answered};
use crate::research::report_writer::{WrittenSection, write_report_section, write_report};
use crate::research::transcript::{self, ActionLog, RecordingLlmClient, ResearchTranscript};
use crate::projects::layout::ProjectPaths;

/// Run the research pipeline: the model plans the research (steps tagged
/// `local` or `internet`), the pipeline follows the plan step by step, then
/// verifies, writes the report, and records everything into
/// `chats/research-{id}.json`. The report itself is saved separately under
/// `outputs/` as markdown.
///
/// `cancel` is polled between steps, searches and fetches; once set, the
/// pipeline stops gathering (skipping LLM verification to avoid further
/// spend) and writes a partial report from whatever it found.
pub async fn run<C: LlmClient + 'static>(
    llm: C,
    req: ResearchRequest, net: &ResearchNet,
    tx: mpsc::Sender<ResearchEvent>, budgets: Budgets,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<i64, ResearchError> {
    let started = std::time::Instant::now();
    let cancelled = |flag: &Option<Arc<AtomicBool>>| flag.as_ref().is_some_and(|f| f.load(Ordering::SeqCst));
    let out_of_time = || started.elapsed().as_secs() >= budgets.wall_clock_secs;

    let root = req.project_root.clone().ok_or(ResearchError::MissingProjectRoot)?;
    let chats_dir = ProjectPaths::new(&root).chats_dir();
    let research_id = transcript::next_research_id(&chats_dir);
    let research_num: i64 = research_id
        .strip_prefix("research-")
        .and_then(|n| n.parse().ok())
        .unwrap_or(1);

    let action_log = transcript::new_action_log();
    let current_step = transcript::new_current_step();
    let llm = RecordingLlmClient::new(llm, action_log.clone(), current_step.clone());

    let allow_internet = matches!(req.mode, ResearchMode::Online) && req.internet_enabled;
    let mode_label = match req.mode {
        ResearchMode::Offline => "offline",
        ResearchMode::Online => "online",
    }
    .to_string();

    // 1. The model makes the plan (system prompt, plan JSON, raw response recorded).
    let (plan_record, steps) = plan_research(&llm, &req.question, allow_internet).await;
    transcript::record(&action_log, "plan", None,
        serde_json::json!({ "question": req.question, "mode": mode_label }),
        serde_json::json!({ "steps": plan_record.steps }));

    let sub_questions: Vec<SubQuestion> = steps.iter()
        .map(|s| SubQuestion {
            question: s.question.clone(),
            search_queries: s.search_queries.clone(),
            answer_criteria: s.answer_criteria.clone(),
        })
        .collect();

    let _ = tx.send(ResearchEvent::Planned {
        sub_questions: sub_questions.iter().map(|sq| sq.question.clone()).collect(),
    }).await;

    let mut ctx = Ctx {
        store: EvidenceStore::new(),
        facts: Vec::new(),
        corroborated_facts: Vec::new(),
        seen_claims: std::collections::HashSet::new(),
        tx: tx.clone(),
        llm: Some(Box::new(llm)),
        action_log: action_log.clone(),
        local_docs: None,
    };

    if out_of_time() {
        let _ = tx.send(ResearchEvent::Notice {
            message: format!("Wall-clock limit reached ({}s); skipping plan execution", budgets.wall_clock_secs),
        }).await;
    }

    // 2. Follow the plan step by step.
    let mut run_cancelled = cancelled(&cancel);
    for (i, step) in steps.iter().enumerate() {
        if run_cancelled {
            let _ = tx.send(ResearchEvent::Notice {
                message: "Research cancelled by user; writing partial report".into(),
            }).await;
            break;
        }
        if out_of_time() {
            let _ = tx.send(ResearchEvent::Notice {
                message: format!("Wall-clock limit reached ({}s); stopping plan execution", budgets.wall_clock_secs),
            }).await;
            break;
        }
        if let Ok(mut slot) = current_step.lock() { *slot = Some(i); }
        let _ = tx.send(ResearchEvent::Notice {
            message: format!("Step {}/{} [{}]: {}", i + 1, steps.len(), step.source, step.question),
        }).await;

        match step.source {
            crate::research::transcript::StepSource::Internet if allow_internet => {
                run_internet_step(&req, step, i, net, &mut ctx, &budgets, &cancel, started).await?;
            }
            crate::research::transcript::StepSource::Internet => {
                let _ = tx.send(ResearchEvent::Notice {
                    message: format!("Step {} skipped: internet is disabled for this project", i + 1),
                }).await;
                transcript::record(&action_log, "search", Some(i),
                    serde_json::json!({ "step": step.question }),
                    serde_json::json!({ "skipped": "internet disabled" }));
            }
            crate::research::transcript::StepSource::Local => {
                run_local_step(&req, step, i, &mut ctx).await?;
            }
        }
        run_cancelled = cancelled(&cancel);
    }
    if let Ok(mut slot) = current_step.lock() { *slot = None; }

    // Budget-aware gap check across the whole plan (skipped when cancelled to avoid LLM spend).
    if !run_cancelled {
        if let Some(llm) = ctx.llm.as_ref().map(|b| b.as_ref()) {
            let analyses = check_gaps(llm, &sub_questions, &ctx.facts, &ctx.corroborated_facts, &budgets).await
                .unwrap_or_default();
        let missing: Vec<String> = analyses.iter()
            .filter(|a| a.status != GapStatus::Answered)
            .flat_map(|a| a.followup_queries.clone())
            .collect();
        if all_subquestions_answered(&analyses, &budgets) || missing.is_empty() {
            let _ = tx.send(ResearchEvent::Notice {
                message: "All plan steps sufficiently answered".into(),
            }).await;
        } else {
            let _ = tx.send(ResearchEvent::GapFound { missing }).await;
        }
        }
    }

    // 3. Verify what the steps found. Skip the LLM stage when cancelled so
    // no further tokens are spent; fall back to cheap stage-1 verification.
    let verdict = if run_cancelled {
        verify::verify_stage1_only(std::mem::take(&mut ctx.facts), &ctx.store)
    } else {
        verify::verify(ctx.llm.as_deref(), std::mem::take(&mut ctx.facts), &ctx.store).await
    };

    for f in &verdict.supported {
        if let Some(source) = ctx.store.sources().iter().find(|s| s.id == f.source_id) {
            let score = SourceScore::compute(&source.url, &source.text, None);
            ctx.corroborated_facts.push(CorroboratedFact {
                claim: f.claim.clone(),
                quote: f.quote.clone(),
                source_id: f.source_id,
                source_url: source.url.clone(),
                source_score: score,
                verification_label: VerificationLabel::Supported,
            });
        }
    }

    let clusters = cluster_claims(&ctx.corroborated_facts);
    let conflicting_count = clusters.iter().filter(|c| c.conflicting).count();
    let multi_source_count = clusters.iter().filter(|c| c.source_count > 1).count();

    let stage1_passes = verdict.details.iter().filter(|d| d.stage1_pass).count();
    let stage2_supported = verdict.details.iter().filter(|d| d.stage2_label == Some(verify::VerificationLabel::Supported)).count();
    let stage2_contradicted = verdict.details.iter().filter(|d| d.stage2_label == Some(verify::VerificationLabel::Contradicted)).count();
    let stage2_unsupported = verdict.details.iter().filter(|d| d.stage2_label == Some(verify::VerificationLabel::Unsupported)).count();

    transcript::record(&action_log, "verify", None,
        serde_json::json!({ "facts": verdict.supported.len() + verdict.unsupported.len() }),
        serde_json::json!({
            "supported": verdict.supported.len(),
            "unsupported": verdict.unsupported.len(),
            "stage1_passes": stage1_passes,
            "stage2_supported": stage2_supported,
            "stage2_contradicted": stage2_contradicted,
            "stage2_unsupported": stage2_unsupported,
        }));

    let _ = tx.send(ResearchEvent::Verified {
        supported: verdict.supported.len(),
        unsupported: verdict.unsupported.len(),
        stage1_passes: Some(stage1_passes),
        stage2_supported: Some(stage2_supported),
        stage2_contradicted: Some(stage2_contradicted),
        stage2_unsupported: Some(stage2_unsupported),
    }).await;

    let _ = tx.send(ResearchEvent::Notice {
        message: format!("Corroboration: {} clusters, {} multi-source, {} conflicting",
            clusters.len(), multi_source_count, conflicting_count),
    }).await;

    // 4. Write one report section per plan step, in plan order.
    let store_sources = ctx.store.sources();
    let written_sections: Vec<WrittenSection> = sub_questions.iter().enumerate().map(|(i, sq)| {
        let facts: Vec<&Fact> = verdict.supported.iter().filter(|f| f.sub_q == i).collect();
        write_report_section(&sq.question, &facts, &verdict.supported, &verdict.details, store_sources, &ctx.corroborated_facts)
    }).collect();

    for section in &written_sections {
        let _ = tx.send(ResearchEvent::SectionWritten {
            title: section.title.clone(),
            markdown: section.content.clone(),
            confidence: Some(section.confidence),
            citation_count: Some(section.citations.len()),
        }).await;

        let _ = tx.send(ResearchEvent::SectionConfidence {
            title: section.title.clone(),
            confidence: section.confidence,
            citation_count: section.citations.len(),
        }).await;
    }

    let general: Vec<&Fact> = verdict.supported.iter()
        .filter(|f| f.sub_q >= sub_questions.len()).collect();
    if !general.is_empty() {
        let section = write_report_section("Provided sources", &general, &verdict.supported, &verdict.details, store_sources, &ctx.corroborated_facts);
        let _ = tx.send(ResearchEvent::SectionWritten {
            title: "Provided sources".into(),
            markdown: section.content,
            confidence: Some(section.confidence),
            citation_count: Some(section.citations.len()),
        }).await;
    }

    // 5. Full report: markdown file in outputs/, everything in the JSON transcript.
    let full_report = write_report(written_sections.clone(), store_sources);

    let filename = format!("research-{}.md", req.question.replace(|c: char| !c.is_alphanumeric(), "_").to_lowercase());
    let outputs_dir = root.join("outputs");
    let _ = std::fs::create_dir_all(&outputs_dir);
    let output_path = outputs_dir.join(&filename);
    let _ = std::fs::write(&output_path, &full_report);
    let report_path = output_path.to_string_lossy().to_string();

    transcript::record(&action_log, "report", None,
        serde_json::json!({ "outputs_file": filename }),
        serde_json::json!({ "report_path": report_path, "chars": full_report.len() }));

    let actions = action_log.lock().map(|a| a.clone()).unwrap_or_default();
    let transcript = ResearchTranscript {
        id: research_id.clone(),
        created_at: Utc::now().to_rfc3339(),
        question: req.question.clone(),
        mode: mode_label,
        internet_enabled: req.internet_enabled,
        system_prompt: plan_record.system_prompt.clone(),
        plan: plan_record,
        actions,
        final_report: full_report,
        report_path: Some(report_path.clone()),
    };
    match transcript::save_transcript(&chats_dir, &transcript) {
        Ok(path) => {
            let _ = tx.send(ResearchEvent::Notice {
                message: format!("Research transcript saved to {}", path.display()),
            }).await;
        }
        Err(e) => {
            log::warn!("[research] failed to save transcript: {e}");
            let _ = tx.send(ResearchEvent::Notice {
                message: format!("Failed to save research transcript: {e}"),
            }).await;
        }
    }

    let _ = tx.send(ResearchEvent::Done {
        report_id: research_num,
        report_path: Some(report_path),
    }).await;
    Ok(research_num)
}

struct Ctx {
    store: EvidenceStore,
    facts: Vec<Fact>,
    corroborated_facts: Vec<CorroboratedFact>,
    seen_claims: std::collections::HashSet<String>,
    tx: mpsc::Sender<ResearchEvent>,
    llm: Option<Box<dyn LlmClient + Send>>,
    action_log: ActionLog,
    /// Project documents, loaded lazily on the first local step.
    local_docs: Option<Vec<crate::research::local::LocalDoc>>,
}

impl Ctx {
    async fn ingest(&mut self, sub_q: usize, source_id: i64, markdown: &str) {
        for c in chunk::chunk_markdown(markdown, 800) {
            let llm_ref = self.llm.as_ref().map(|b| b.as_ref());
            let extracted = if let Some(llm) = llm_ref {
                reader::extract(llm, &c, source_id).await
            } else {
                reader::fallback_extract(&c, source_id)
            };

            for raw in extracted {
                if !reader::accept_fact(&raw, &c) { continue; }
                let claim = raw.claim.split_whitespace().collect::<Vec<_>>().join(" ");
                if !self.seen_claims.insert(reader::norm(&claim)) { continue; }
                let _ = self.tx.send(ResearchEvent::FactExtracted {
                    sub_q, source_id, claim: claim.clone(),
                }).await;
                self.facts.push(Fact { sub_q, source_id, claim, quote: raw.quote });
            }
        }
    }
}

/// Execute one `local` plan step against the project document corpus.
async fn run_local_step(
    req: &ResearchRequest, step: &crate::research::transcript::PlanStep, step_idx: usize, ctx: &mut Ctx,
) -> Result<(), ResearchError> {
    let root = req.project_root.as_ref().ok_or(ResearchError::MissingProjectRoot)?;

    if ctx.local_docs.is_none() {
        ctx.local_docs = Some(crate::research::local::load_corpus(root).map_err(ResearchError::Internal)?);
    }

    let mut ids: HashMap<String, i64> = HashMap::new();
    // Collect (path, title, text) triples first so the borrow on ctx ends before ingest.
    let picked: Vec<(String, String, String)> = {
        let docs = ctx.local_docs.as_ref().expect("local corpus loaded");
        crate::research::local::rank(docs, &step.search_queries, 8)
            .into_iter()
            .map(|d| (d.path_string(), d.title.clone(), d.text.clone()))
            .collect()
    };
    for (path, title, text) in picked {
        let id = match ids.get(&path) {
            Some(&id) => id,
            None => {
                let score = SourceScore::compute(&path, &text, None).composite;
                let id = ctx.store.add_source_with_quality(&path, &title, &text, score);
                ids.insert(path.clone(), id);
                transcript::record(&ctx.action_log, "local_search", Some(step_idx),
                    serde_json::json!({ "path": path, "query": step.question }),
                    serde_json::json!({ "source_id": id, "chars": text.len() }));
                let _ = ctx.tx.send(ResearchEvent::Fetched {
                    url: path, tier: "local".into(), chars: text.len(),
                }).await;
                id
            }
        };
        ctx.ingest(step_idx, id, &text).await;
    }
    Ok(())
}

/// Execute one `internet` plan step: search each query, fetch the best pages.
async fn run_internet_step(
    req: &ResearchRequest, step: &crate::research::transcript::PlanStep, step_idx: usize,
    net: &ResearchNet, ctx: &mut Ctx, budgets: &Budgets,
    cancel: &Option<Arc<AtomicBool>>, started: std::time::Instant,
) -> Result<(), ResearchError> {
    let browser = req.use_browser.then(|| BrowserFetcher::new(None));
    let general = step_idx;
    let is_cancelled = |cancel: &Option<Arc<AtomicBool>>| cancel.as_ref().is_some_and(|f| f.load(Ordering::SeqCst));
    let out_of_time = || started.elapsed().as_secs() >= budgets.wall_clock_secs;

    let mut queue: Vec<SearchResult> = Vec::new();
    for query in &step.search_queries {
        if is_cancelled(cancel) || out_of_time() { break; }
        let mut query = query.clone();
        // Inject current date for time-sensitive queries
        let time_sensitive_keywords = ["today", "current", "latest", "recent", "now", "this week", "this month", "stock market", "weather"];
        let is_time_sensitive = time_sensitive_keywords.iter().any(|kw| query.to_lowercase().contains(kw));
        if is_time_sensitive {
            let today = Utc::now().format("%B %d, %Y").to_string();
            if !query.to_lowercase().contains(&today.to_lowercase()) {
                query = format!("{} {}", query, today);
            }
        }
        let _ = ctx.tx.send(ResearchEvent::Searching { query: query.clone() }).await;
        match net.search(&query, 10).await {
            Ok(results) => {
                let unique_results = crate::research::search::dedupe_results(results);
                transcript::record(&ctx.action_log, "search", Some(step_idx),
                    serde_json::json!({ "query": query }),
                    serde_json::json!({ "results": unique_results.iter().map(|r| serde_json::json!({
                        "url": r.url, "title": r.title, "provider": r.provider,
                    })).collect::<Vec<_>>() }));
                queue.extend(unique_results);
            }
            Err(e) => {
                transcript::record(&ctx.action_log, "search", Some(step_idx),
                    serde_json::json!({ "query": query }),
                    serde_json::json!({ "error": e.clone() }));
                let _ = ctx.tx.send(ResearchEvent::Notice {
                    message: format!("Search failed for '{}': {}", query, e),
                }).await;
            }
        }
    }

    // Provided URLs are ingested alongside the first step's internet work.
    if step_idx == 0 {
        for u in &req.provided_urls {
            queue.push(SearchResult {
                title: "Provided".into(),
                url: u.clone(),
                snippet: "User-provided URL".into(),
                published_at: None,
                provider: "user".into(),
                rank: 0,
            });
        }
    }

    queue.sort_by(|a, b| {
        let score_a = SourceScore::compute(&a.url, &a.snippet, a.published_at).composite;
        let score_b = SourceScore::compute(&b.url, &b.snippet, b.published_at).composite;
        score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut seen = std::collections::HashSet::new();
    let mut pages = 0usize;
    for result in queue {
        if is_cancelled(cancel) {
            let _ = ctx.tx.send(ResearchEvent::Notice {
                message: "Research cancelled by user; stopping fetches".into(),
            }).await;
            break;
        }
        if out_of_time() {
            let _ = ctx.tx.send(ResearchEvent::Notice {
                message: format!("Wall-clock limit reached ({}s); stopping fetches", budgets.wall_clock_secs),
            }).await;
            break;
        }
        if pages >= budgets.max_pages { break; }
        if !seen.insert(result.url.clone()) { continue; }
        match fetch_tiered(net, browser.as_ref(), &result.url).await {
            Ok((page, tier)) => {
                pages += 1;
                let score = SourceScore::compute(&page.url, &page.markdown, None).composite;
                let id = ctx.store.add_source_with_quality(&page.url, &result.title, &page.markdown, score);
                transcript::record(&ctx.action_log, "fetch", Some(step_idx),
                    serde_json::json!({ "url": page.url, "tier": tier }),
                    serde_json::json!({ "source_id": id, "chars": page.markdown.len() }));
                let _ = ctx.tx.send(ResearchEvent::Fetched {
                    url: page.url.clone(), tier: tier.into(), chars: page.markdown.len(),
                }).await;
                ctx.ingest(general, id, &page.markdown).await;
            }
            Err(why) => {
                transcript::record(&ctx.action_log, "fetch", Some(step_idx),
                    serde_json::json!({ "url": result.url }),
                    serde_json::json!({ "error": why.clone() }));
                let _ = ctx.tx.send(ResearchEvent::Notice {
                    message: format!("Skipped {}: {}", result.url, why),
                }).await;
            }
        }
    }
    Ok(())
}

async fn fetch_tiered(
    net: &ResearchNet, browser: Option<&BrowserFetcher>, url: &str,
) -> Result<(Page, &'static str), String> {
    const THIN: usize = 400;
    // When internet is on and browser is available, prefer Chromium for richer content
    // (JS-rendered pages, dynamic content). Fall back to HTTP if browser fails.
    if let Some(b) = browser {
        match net.fetch_browser(b, url).await {
            Ok(p) if p.markdown.len() >= THIN => return Ok((p, "browser")),
            Ok(p) => {
                // Browser returned thin content, try HTTP as fallback
                if let Ok(http_p) = net.fetch_http(url).await {
                    if http_p.markdown.len() > p.markdown.len() {
                        return Ok((http_p, "http"));
                    }
                }
                return Ok((p, "browser"));
            }
            Err(_) => {
                // Browser failed, fall back to HTTP
                if let Ok(p) = net.fetch_http(url).await {
                    if p.markdown.len() >= THIN {
                        return Ok((p, "http"));
                    }
                }
            }
        }
    }
    // No browser available or browser failed - use HTTP
    match net.fetch_http(url).await {
        Ok(p) if p.markdown.len() >= THIN => Ok((p, "http")),
        Ok(p) => Ok((p, "http")),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::budget::Budgets;
    use crate::research::request::ResearchMode;
    use crate::research::llm::MockLlmClient;

    fn temp_project(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "LocalAIse-orchestrator-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("data")).unwrap();
        std::fs::write(
            dir.join("data").join("notes.md"),
            "The Helsinki project was founded in 2019. It focuses on local inference.",
        )
        .unwrap();
        dir
    }

    #[tokio::test]
    async fn offline_run_produces_transcript_and_report() {
        let root = temp_project("offline-smoke");

        // The mock returns "{}" for every call, which fails plan parsing,
        // so the planner falls back to a single local step. Deterministic.
        let llm = MockLlmClient::new("mock");
        let req = ResearchRequest {
            question: "When was the Helsinki project founded?".into(),
            mode: ResearchMode::Offline,
            project_root: Some(root.clone()),
            provided_urls: vec![],
            use_browser: false,
            internet_enabled: false,
        };
        let net = ResearchNet::new(root.clone(), false);
        let (tx, mut rx) = mpsc::channel(256);
        tokio::spawn(async move { while rx.recv().await.is_some() {} });

        let report_id = run(llm, req, &net, tx, Budgets::quick(), None).await.unwrap();
        assert_eq!(report_id, 1);

        // Transcript JSON lands in the project chats directory.
        let transcript_path = root.join("chats").join("research-001.json");
        assert!(transcript_path.exists(), "transcript should exist at {transcript_path:?}");
        let raw = std::fs::read_to_string(&transcript_path).unwrap();
        let transcript: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(transcript["id"], "research-001");
        assert!(transcript["systemPrompt"].as_str().map(|s| !s.is_empty()).unwrap_or(false));
        assert!(transcript["plan"]["steps"].as_array().map(|a| !a.is_empty()).unwrap_or(false));
        assert!(transcript["actions"].as_array().map(|a| !a.is_empty()).unwrap_or(false));
        assert!(transcript["finalReport"].as_str().map(|s| s.contains("Research Report")).unwrap_or(false));

        // The report markdown is stored separately under outputs/.
        let outputs = root.join("outputs");
        let has_md = std::fs::read_dir(&outputs)
            .map(|d| d.flatten().any(|e| e.file_name().to_string_lossy().ends_with(".md")))
            .unwrap_or(false);
        assert!(has_md, "a markdown report should exist in outputs/");

        let _ = std::fs::remove_dir_all(&root);
    }
}
