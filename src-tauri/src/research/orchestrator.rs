use std::collections::{BTreeSet, HashMap, HashSet};
use tokio::sync::mpsc;
use crate::research::{chunk, roles};
use crate::research::fact::Fact;
use crate::research::events::ResearchEvent;
use crate::research::fetch::{BrowserFetcher, Page};
use crate::research::net::ResearchNet;
use crate::research::request::*;
use crate::research::roles::{gap, reader, verify, writer};
use crate::research::store::EvidenceStore;

pub struct Budgets { pub max_rounds: usize, pub max_pages: usize, pub wall_clock_secs: u64 }
impl Default for Budgets { fn default() -> Self { Self { max_rounds: 3, max_pages: 25, wall_clock_secs: 300 } } }

pub async fn run(
    req: ResearchRequest, net: &ResearchNet,
    tx: mpsc::Sender<ResearchEvent>, budgets: Budgets,
) -> Result<i64, String> {
    let mut sub_questions = roles::plan_subquestions(&req.question);
    if sub_questions.is_empty() { sub_questions.push(req.question.clone()); }
    let _ = tx.send(ResearchEvent::Planned { sub_questions: sub_questions.clone() }).await;

    let mut ctx = Ctx {
        store: EvidenceStore::new(), facts: Vec::new(),
        seen_claims: HashSet::new(), tx: tx.clone(),
    };

    match req.mode {
        ResearchMode::Offline => run_offline(&req, &sub_questions, &mut ctx).await?,
        ResearchMode::Online if !req.internet_enabled => {
            let _ = tx.send(ResearchEvent::Notice {
                message: "Internet is disabled for this project; running offline.".into(),
            }).await;
            run_offline(&req, &sub_questions, &mut ctx).await?;
        }
        ResearchMode::Online => run_online(&req, &sub_questions, net, &mut ctx, &budgets).await?,
    }

    let verdict = verify::verify(std::mem::take(&mut ctx.facts), &ctx.store);

    for (i, sq) in sub_questions.iter().enumerate() {
        let facts: Vec<&Fact> = verdict.supported.iter().filter(|f| f.sub_q == i).collect();
        let _ = tx.send(ResearchEvent::SectionWritten {
            title: sq.clone(), markdown: writer::write_section(sq, &facts),
        }).await;
    }
    let general: Vec<&Fact> = verdict.supported.iter()
        .filter(|f| f.sub_q >= sub_questions.len()).collect();
    if !general.is_empty() {
        let _ = tx.send(ResearchEvent::SectionWritten {
            title: "Provided sources".into(),
            markdown: writer::write_section("Provided sources", &general),
        }).await;
    }

    let cited: BTreeSet<i64> = verdict.supported.iter().map(|f| f.source_id).collect();
    let listing = ctx.store.sources().iter()
        .filter(|s| cited.contains(&s.id))
        .map(|s| format!("- [{}] {}", s.id, s.url))
        .collect::<Vec<_>>().join("\n");
    let _ = tx.send(ResearchEvent::SectionWritten {
        title: "Sources".into(), markdown: format!("## Sources\n\n{listing}\n"),
    }).await;

    let _ = tx.send(ResearchEvent::Verified {
        supported: verdict.supported.len(), unsupported: verdict.unsupported.len(),
    }).await;

    let report_id = 1;
    let _ = tx.send(ResearchEvent::Done { report_id }).await;
    Ok(report_id)
}

struct Ctx {
    store: EvidenceStore,
    facts: Vec<Fact>,
    seen_claims: HashSet<String>,
    tx: mpsc::Sender<ResearchEvent>,
}

impl Ctx {
    async fn ingest(&mut self, sub_q: usize, source_id: i64, markdown: &str) {
        for c in chunk::chunk_markdown(markdown, 500) {
            for raw in reader::extract(&c, source_id) {
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

async fn run_offline(req: &ResearchRequest, subs: &[String], ctx: &mut Ctx) -> Result<(), String> {
    let root = req.project_root.as_ref().ok_or("offline research needs a project root")?;
    let docs = crate::research::local::load_corpus(root)?;
    let mut ids: HashMap<String, i64> = HashMap::new();

    for (i, sq) in subs.iter().enumerate() {
        for doc in crate::research::local::rank(&docs, std::slice::from_ref(sq), 8) {
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

    let gaps = gap::check(subs.len(), &ctx.facts);
    if !gaps.is_empty() {
        let _ = ctx.tx.send(ResearchEvent::GapFound {
            missing: gaps.iter().map(|&i| subs[i].clone()).collect(),
        }).await;
    }
    Ok(())
}

async fn run_online(
    req: &ResearchRequest, subs: &[String], net: &ResearchNet,
    ctx: &mut Ctx, b: &Budgets,
) -> Result<(), String> {
    let browser = req.use_browser.then(|| BrowserFetcher::new(None));
    let general = subs.len();
    let mut seen = HashSet::new();
    let mut pages = 0usize;
    let started = std::time::Instant::now();
    let mut pending: Vec<usize> = (0..subs.len()).collect();

    let mut queue: Vec<(usize, String)> =
        req.provided_urls.iter().map(|u| (general, u.clone())).collect();

    for round in 0..b.max_rounds {
        if started.elapsed().as_secs() >= b.wall_clock_secs { break; }

        for &i in &pending {
            let search_url = format!("https://duckduckgo.com/html/?q={}", urlencoding::encode(&subs[i]));
            let _ = ctx.tx.send(ResearchEvent::Searching { query: subs[i].clone() }).await;
            if let Ok(html) = net.search_html(&search_url).await {
                queue.extend(extract_result_links(&html, round * 5, 5).into_iter().map(|u| (i, u)));
            }
        }

        let before = ctx.facts.len();
        for (sq, url) in std::mem::take(&mut queue) {
            if pages >= b.max_pages { break; }
            if !seen.insert(url.clone()) { continue; }
            match fetch_tiered(net, browser.as_ref(), &url).await {
                Ok((page, tier)) => {
                    pages += 1;
                    let id = ctx.store.add_source(&page.url, "", &page.markdown);
                    let _ = ctx.tx.send(ResearchEvent::Fetched {
                        url: page.url.clone(), tier: tier.into(), chars: page.markdown.len(),
                    }).await;
                    ctx.ingest(sq, id, &page.markdown).await;
                }
                Err(why) => {
                    let _ = ctx.tx.send(ResearchEvent::Notice {
                        message: format!("Skipped {url}: {why}"),
                    }).await;
                }
            }
        }

        pending = gap::check(subs.len(), &ctx.facts);
        if !pending.is_empty() {
            let _ = ctx.tx.send(ResearchEvent::GapFound {
                missing: pending.iter().map(|&i| subs[i].clone()).collect(),
            }).await;
        }
        if pending.is_empty() || pages >= b.max_pages { break; }
        if round > 0 && ctx.facts.len() == before { break; }
    }
    Ok(())
}

async fn fetch_tiered(
    net: &ResearchNet, browser: Option<&BrowserFetcher>, url: &str,
) -> Result<(Page, &'static str), String> {
    const THIN: usize = 400;
    match net.fetch_http(url).await {
        Ok(p) if p.markdown.len() >= THIN || browser.is_none() => Ok((p, "http")),
        other => match browser {
            Some(b) => net.fetch_browser(b, url).await.map(|p| (p, "browser")),
            None => other.map(|p| (p, "http")),
        },
    }
}

fn extract_result_links(html: &str, skip: usize, take: usize) -> Vec<String> {
    let doc = scraper::Html::parse_document(html);
    let sel = scraper::Selector::parse("a.result__a").unwrap();
    doc.select(&sel)
        .filter_map(|a| a.value().attr("href"))
        .filter_map(|h| {
            let full = if h.starts_with("//") { format!("https:{h}") } else { h.to_string() };
            let u = url::Url::parse(&full).ok()?;
            u.query_pairs().find(|(k, _)| k == "uddg").map(|(_, v)| v.into_owned())
                .or(Some(full))
        })
        .skip(skip)
        .take(take)
        .collect()
}