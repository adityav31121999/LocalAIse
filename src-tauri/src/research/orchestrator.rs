use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;
use chrono::Utc;
use crate::research::chunk;
use crate::research::fact::Fact;
use crate::research::events::ResearchEvent;
use crate::research::fetch::{BrowserFetcher, Page};
use crate::research::net::ResearchNet;
use crate::research::request::*;
use crate::research::roles::{reader, verify, plan_subquestions, SubQuestion};
use crate::research::store::EvidenceStore;
use crate::research::llm::LlmClient;
use crate::research::search::{SearchResult, SourceScore, CorroboratedFact, cluster_claims};
use crate::research::roles::verify::VerificationLabel;
use crate::research::budget::{Budgets, GapStatus, check_gaps, should_stop_adaptive, all_subquestions_answered};
use crate::research::report_writer::{WrittenSection, write_report_section, write_report};

/// Run the research pipeline with LLM-powered roles and adaptive control.
pub async fn run<C: LlmClient + 'static>(
    llm: C,
    req: ResearchRequest, net: &ResearchNet,
    tx: mpsc::Sender<ResearchEvent>, budgets: Budgets,
) -> Result<i64, String> {
    // Plan sub-questions using LLM
    let sub_question_objs = plan_subquestions(&llm, &req.question).await;
    let sub_questions: Vec<SubQuestion> = sub_question_objs.into_iter()
        .map(|q| SubQuestion::simple(q))
        .collect();
    let sub_questions = if sub_questions.is_empty() {
        vec![SubQuestion::simple(req.question.clone())]
    } else { sub_questions };

    let _ = tx.send(ResearchEvent::Planned { 
        sub_questions: sub_questions.iter().map(|sq| sq.question.clone()).collect() 
    }).await;

    let mut ctx = Ctx {
        store: EvidenceStore::new(), 
        facts: Vec::new(),
        corroborated_facts: Vec::new(),
        seen_claims: HashSet::new(), 
        tx: tx.clone(),
        llm: Some(Box::new(llm)),
    };

    match req.mode {
        ResearchMode::Offline => run_offline(&req, &sub_questions, &mut ctx, &budgets).await?,
        ResearchMode::Online if !req.internet_enabled => {
            let _ = tx.send(ResearchEvent::Notice {
                message: "Internet is disabled for this project; running offline.".into(),
            }).await;
            run_offline(&req, &sub_questions, &mut ctx, &budgets).await?;
        }
        ResearchMode::Online => run_online(&req, &sub_questions, net, &mut ctx, &budgets).await?,
    }

    // Two-stage verification with LLM
    let verdict = verify::verify(ctx.llm.as_deref(), std::mem::take(&mut ctx.facts), &ctx.store).await;

    // Build corroborated facts with source scores
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

    // Corroboration clustering
    let clusters = cluster_claims(&ctx.corroborated_facts);
    let conflicting_count = clusters.iter().filter(|c| c.conflicting).count();
    let multi_source_count = clusters.iter().filter(|c| c.source_count > 1).count();

    // Log verification stats
    let stage1_passes = verdict.details.iter().filter(|d| d.stage1_pass).count();
    let stage2_supported = verdict.details.iter().filter(|d| d.stage2_label == Some(verify::VerificationLabel::Supported)).count();
    let stage2_contradicted = verdict.details.iter().filter(|d| d.stage2_label == Some(verify::VerificationLabel::Contradicted)).count();
    let stage2_unsupported = verdict.details.iter().filter(|d| d.stage2_label == Some(verify::VerificationLabel::Unsupported)).count();
    
    let _ = tx.send(ResearchEvent::Verified {
        supported: verdict.supported.len(),
        unsupported: verdict.unsupported.len(),
        stage1_passes: Some(stage1_passes),
        stage2_supported: Some(stage2_supported),
        stage2_contradicted: Some(stage2_contradicted),
        stage2_unsupported: Some(stage2_unsupported),
    }).await;

    // Emit corroboration stats
    let _ = tx.send(ResearchEvent::Notice {
        message: format!("Corroboration: {} clusters, {} multi-source, {} conflicting", 
            clusters.len(), multi_source_count, conflicting_count),
    }).await;

    // Write enhanced sections using the new writer
    let written_sections: Vec<WrittenSection> = sub_questions.iter().enumerate().map(|(i, sq)| {
        let facts: Vec<&Fact> = verdict.supported.iter().filter(|f| f.sub_q == i).collect();
        write_report_section(&sq.question, &facts, &verdict.supported, &verdict.details, &ctx.store.sources(), &ctx.corroborated_facts)
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
        let section = write_report_section("Provided sources", &general, &verdict.supported, &verdict.details, &ctx.store.sources(), &ctx.corroborated_facts);
        let _ = tx.send(ResearchEvent::SectionWritten {
            title: "Provided sources".into(),
            markdown: section.content,
            confidence: Some(section.confidence),
            citation_count: Some(section.citations.len()),
        }).await;
    }

    // Generate full report
    let full_report = write_report(written_sections.clone(), &ctx.store.sources());
    
    // Save full report to outputs/
    let filename = format!("research-{}.md", req.question.replace(|c: char| !c.is_alphanumeric(), "_").to_lowercase());
    let outputs_dir = std::path::PathBuf::from(&req.project_root.as_ref().unwrap_or(&std::path::PathBuf::new())).join("outputs");
    let _ = std::fs::create_dir_all(&outputs_dir);
    let output_path = outputs_dir.join(&filename);
    let _ = std::fs::write(&output_path, &full_report);

    let report_id = 1;
    let _ = tx.send(ResearchEvent::Done { 
        report_id, 
        report_path: Some(output_path.to_string_lossy().to_string()) 
    }).await;
    Ok(report_id)
}

struct Ctx {
    store: EvidenceStore,
    facts: Vec<Fact>,
    corroborated_facts: Vec<CorroboratedFact>,
    seen_claims: HashSet<String>,
    tx: mpsc::Sender<ResearchEvent>,
    llm: Option<Box<dyn LlmClient + Send>>,
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

async fn run_offline(
    req: &ResearchRequest, subs: &[SubQuestion], ctx: &mut Ctx, budgets: &Budgets,
) -> Result<(), String> {
    let root = req.project_root.as_ref().ok_or("offline research needs a project root")?;
    let docs = crate::research::local::load_corpus(root)?;
    let mut ids: HashMap<String, i64> = HashMap::new();

    for (i, sq) in subs.iter().enumerate() {
        for doc in crate::research::local::rank(&docs, std::slice::from_ref(&sq.question), 8) {
            let path = doc.path_string();
            let id = match ids.get(&path) {
                Some(&id) => id,
                None => {
                    let id = ctx.store.add_source(&path, &doc.title, &doc.text);
                    ids.insert(path.clone(), id);
                    let _ = ctx.tx.send(ResearchEvent::Fetched {
                        url: path, tier: "local".into(), chars: doc.text.len(),
                    }).await;
                    id
                }
            };
            ctx.ingest(i, id, &doc.text).await;
        }
    }

    // Use LLM-based gap checker for offline too
    if let Some(llm) = ctx.llm.as_ref().map(|b| b.as_ref()) {
        let analyses = check_gaps(llm, subs, &ctx.facts, &ctx.corroborated_facts, budgets).await
            .unwrap_or_default();
        
        let missing: Vec<String> = analyses.iter()
            .filter(|a| a.status != GapStatus::Answered)
            .flat_map(|a| a.followup_queries.clone())
            .collect();
        
        if !missing.is_empty() {
            let _ = ctx.tx.send(ResearchEvent::GapFound {
                missing,
            }).await;
        }
    } else {
        // Fallback to simple gap check
        let gaps = crate::research::roles::gap::check(subs.len(), &ctx.facts);
        if !gaps.is_empty() {
            let _ = ctx.tx.send(ResearchEvent::GapFound {
                missing: gaps.iter().map(|&i| subs[i].question.clone()).collect(),
            }).await;
        }
    }
    Ok(())
}

async fn run_online(
    req: &ResearchRequest, subs: &[SubQuestion], net: &ResearchNet,
    ctx: &mut Ctx, budgets: &Budgets,
) -> Result<(), String> {
    let browser = req.use_browser.then(|| BrowserFetcher::new(None));
    let general = subs.len();
    let mut seen = HashSet::new();
    let mut pages = 0usize;
    let started = std::time::Instant::now();
    let mut low_gain_streak = 0usize;
    let mut pending: Vec<usize> = (0..subs.len()).collect();

    let mut queue: Vec<(usize, SearchResult)> =
        req.provided_urls.iter().map(|u| (general, SearchResult {
            title: "Provided".into(),
            url: u.clone(),
            snippet: "User-provided URL".into(),
            published_at: None,
            provider: "user".into(),
            rank: 0,
        })).collect();

    for round in 0..budgets.max_rounds {
        if started.elapsed().as_secs() >= budgets.wall_clock_secs { 
            let _ = ctx.tx.send(ResearchEvent::Notice {
                message: format!("Wall-clock limit reached ({}s)", budgets.wall_clock_secs),
            }).await;
            break; 
        }

        // Track cost for this round
        let mut cost_this_round = 0u64;

        for &i in &pending {
            // Use sub-question's search queries if available
            let mut query = subs[i].search_queries.first().cloned().unwrap_or_else(|| subs[i].question.clone());
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
                    // Deduplicate and score results
                    let unique_results = crate::research::search::dedupe_results(results);
                    cost_this_round += unique_results.len() as u64;
                    queue.extend(unique_results.into_iter().map(|r| (i, r)));
                }
                Err(e) => {
                    let _ = ctx.tx.send(ResearchEvent::Notice {
                        message: format!("Search failed for '{}': {}", query, e),
                    }).await;
                }
            }
        }

        // Sort queue by source score (higher quality first)
        queue.sort_by(|a, b| {
            let score_a = SourceScore::compute(&a.1.url, &a.1.snippet, a.1.published_at).composite;
            let score_b = SourceScore::compute(&b.1.url, &b.1.snippet, b.1.published_at).composite;
            score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
        });

        let facts_before = ctx.facts.len();
        let corroborated_before = ctx.corroborated_facts.len();

        for (sq, result) in std::mem::take(&mut queue) {
            if pages >= budgets.max_pages { break; }
            if !seen.insert(result.url.clone()) { continue; }
            match fetch_tiered(net, browser.as_ref(), &result.url).await {
                Ok((page, tier)) => {
                    pages += 1;
                    cost_this_round += 1;
                    let id = ctx.store.add_source(&page.url, &result.title, &page.markdown);
                    let _ = ctx.tx.send(ResearchEvent::Fetched {
                        url: page.url.clone(), tier: tier.into(), chars: page.markdown.len(),
                    }).await;
                    ctx.ingest(sq, id, &page.markdown).await;
                }
                Err(why) => {
                    let _ = ctx.tx.send(ResearchEvent::Notice {
                        message: format!("Skipped {}: {}", result.url, why),
                    }).await;
                }
            }
        }

        // LLM-based gap analysis
        let _new_facts = ctx.facts.len().saturating_sub(facts_before);
        
        if let Some(llm) = ctx.llm.as_ref().map(|b| b.as_ref()) {
            let analyses = check_gaps(llm, subs, &ctx.facts, &ctx.corroborated_facts, budgets).await
                .unwrap_or_default();
            
            // Collect follow-up queries for partial/missing sub-questions
            let mut followup_queries = Vec::new();
            for analysis in &analyses {
                if analysis.status != GapStatus::Answered {
                    followup_queries.extend(analysis.followup_queries.clone());
                }
            }
            
            // Emit gap event with follow-up queries
            if !followup_queries.is_empty() {
                let followup_for_event = followup_queries.clone();
                let _ = ctx.tx.send(ResearchEvent::GapFound {
                    missing: followup_for_event,
                }).await;
                
                // Add follow-up queries to queue for next round
                for query in followup_queries {
                    queue.push((general, SearchResult {
                        title: "Follow-up".into(),
                        url: format!("https://duckduckgo.com/html/?q={}", urlencoding::encode(&query)),
                        snippet: query.clone(),
                        published_at: None,
                        provider: "gap_followup".into(),
                        rank: 0,
                    }));
                }
            }

            // Check if all sub-questions are answered
            if all_subquestions_answered(&analyses, budgets) {
                let _ = ctx.tx.send(ResearchEvent::Notice {
                    message: "All sub-questions sufficiently answered".into(),
                }).await;
                break;
            }

            // Adaptive stopping based on marginal gain
            let new_corroborated = ctx.corroborated_facts.len().saturating_sub(corroborated_before);
            if should_stop_adaptive(round, new_corroborated, cost_this_round, budgets, &mut low_gain_streak) {
                let _ = ctx.tx.send(ResearchEvent::Notice {
                    message: format!("Stopping: marginal gain below threshold for {} consecutive rounds", low_gain_streak),
                }).await;
                break;
            }
        } else {
            // Fallback to simple gap check
            pending = crate::research::roles::gap::check(subs.len(), &ctx.facts);
            if !pending.is_empty() {
                let _ = ctx.tx.send(ResearchEvent::GapFound {
                    missing: pending.iter().map(|&i| subs[i].question.clone()).collect(),
                }).await;
            }
            if pending.is_empty() || pages >= budgets.max_pages { break; }
            if round > 0 && ctx.facts.len() == facts_before { break; }
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

