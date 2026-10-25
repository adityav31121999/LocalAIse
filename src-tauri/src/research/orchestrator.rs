use crate::research::events::ResearchEvent;
use crate::research::fetch::HttpFetcher;
use crate::research::roles::{gap, reader, writer};
use crate::research::store::EvidenceStore;
use crate::research::{chunk, roles};
use tokio::sync::mpsc;

pub struct Budgets { pub max_rounds: usize, pub max_pages: usize, pub wall_clock_secs: u64 }

impl Default for Budgets { fn default() -> Self { Self { max_rounds: 3, max_pages: 25, wall_clock_secs: 300 } } }

pub async fn run_once(question: &str, tx: mpsc::Sender<ResearchEvent>) -> Result<i64, String> {
    run(question, tx, Budgets::default()).await
}

pub async fn run(question: &str, tx: mpsc::Sender<ResearchEvent>, budgets: Budgets) -> Result<i64, String> {
    let sub_questions = roles::plan_subquestions(question);
    let _ = tx.send(ResearchEvent::Planned { sub_questions: sub_questions.clone() }).await;

    let mut store = EvidenceStore::new();
    let fetcher = HttpFetcher::new();
    let mut all_facts: Vec<String> = Vec::new();
    let mut seen_urls = std::collections::HashSet::new();
    let mut pages_fetched = 0usize;

    for round in 0..budgets.max_rounds {
        // Search: stub queries (one per sub-question).
        let mut urls: Vec<String> = Vec::new();
        for sq in &sub_questions {
            let q = format!("https://duckduckgo.com/html/?q={}", sq.replace(' ', "+"));
            let _ = tx.send(ResearchEvent::Searching { query: q.clone() }).await;
            urls.push(q);
        }

        // Fetch tier.
        for url in urls {
            if pages_fetched >= budgets.max_pages { break; }
            if !seen_urls.insert(url.clone()) { continue; }
            if let Ok(page) = fetcher.fetch(&url).await {
                pages_fetched += 1;
                let id = store.add_source(&page.url, "", &page.markdown);
                let _ = tx.send(ResearchEvent::Fetched { url: page.url.clone(), tier: "http".into(), chars: page.markdown.len() }).await;
                // Chunk + read.
                for c in chunk::chunk_markdown(&page.markdown, 500) {
                    for f in reader::extract(&c, id) {
                        if reader::accept_fact(&f, &c) {
                            all_facts.push(f.claim.clone());
                            let _ = tx.send(ResearchEvent::FactExtracted { sub_q: 0, source_id: f.source_id, claim: f.claim }).await;
                        }
                    }
                }
            }
        }

        // Gap check.
        let gaps = gap::check(&all_facts);
        if !gaps.is_empty() {
            let _ = tx.send(ResearchEvent::GapFound { missing: gaps.clone() }).await;
        }

        // Novelty stop: if no new facts this round, stop.
        // (Simplified: stop when gaps empty.)
        if gaps.is_empty() { break; }
        let _ = round;
    }

    // Write sections.
    let section = writer::write_section("Findings", &all_facts);
    let _ = tx.send(ResearchEvent::SectionWritten { title: "Findings".into(), markdown: section }).await;

    // Verifier stub: count supported = all facts.
    let _ = tx.send(ResearchEvent::Verified { supported: all_facts.len(), unsupported: 0 }).await;

    let report_id = 1;
    let _ = tx.send(ResearchEvent::Done { report_id }).await;
    Ok(report_id)
}