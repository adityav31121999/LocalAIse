use super::metrics::{self, Hit, Relevant, Report};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{path::Path, time::Instant};

#[derive(Debug, Clone, Deserialize)]
pub struct GoldItem {
    pub id: String,
    pub query: String,
    pub relevant: Vec<Relevant>,
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub difficulty: String,
}

/// Anything that can answer a query with ranked hits.
/// Implement once per variant: BM25, dense, hybrid, hybrid+rerank.
#[async_trait]
pub trait Retriever: Send + Sync {
    fn name(&self) -> &str;
    /// Return up to `limit` hits, best first. Limit is at least 50 so
    /// Recall@50 is meaningful.
    async fn retrieve(&self, query: &str, limit: usize) -> anyhow::Result<Vec<Hit>>;
}

pub fn load_golden(path: &Path) -> anyhow::Result<Vec<GoldItem>> {
    std::fs::read_to_string(path)?
        .lines()
        .filter(|l| !l.trim().is_empty())
        .enumerate()
        .map(|(i, l)| {
            serde_json::from_str(l)
                .map_err(|e| anyhow::anyhow!("golden line {}: {e}", i + 1))
        })
        .collect()
}

#[derive(Serialize)]
pub struct ItemResult {
    pub id: String,
    pub recall_at_5: Option<f32>,
    pub mrr: Option<f32>,
    pub latency_ms: f64,
    pub top_hits: Vec<(String, f32)>, // (source, score) for debugging
}

#[derive(Serialize)]
pub struct RunOutput {
    pub report: Report,
    pub items: Vec<ItemResult>,
}

pub async fn run(
    retriever: &dyn Retriever,
    golden: &[GoldItem],
    negative_threshold: f32,
) -> anyhow::Result<RunOutput> {
    let mut items = Vec::with_capacity(golden.len());
    let mut latencies = Vec::with_capacity(golden.len());
    let mut fp_flags = Vec::new();

    // Collected per-metric so each mean ignores items where it's undefined.
    let (mut r5, mut r20, mut r50, mut rr, mut nd) =
        (vec![], vec![], vec![], vec![], vec![]);

    // Per-item recall@5 for bootstrap CI
    let mut r5_per_item: Vec<f32> = Vec::new();

    for item in golden {
        let t0 = Instant::now();
        let hits = retriever.retrieve(&item.query, 50).await?;
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        latencies.push(ms);

        let rel = &item.relevant;
        let r5_val = metrics::recall_at_k(&hits, rel, 5);
        r5.push(r5_val);
        r20.push(metrics::recall_at_k(&hits, rel, 20));
        r50.push(metrics::recall_at_k(&hits, rel, 50));
        rr.push(metrics::reciprocal_rank(&hits, rel));
        nd.push(metrics::ndcg_at_k(&hits, rel, 10));

        // Collect defined recall@5 values for bootstrap
        if let Some(v) = r5_val {
            r5_per_item.push(v);
        }

        if rel.is_empty() {
            fp_flags.push(Some(metrics::false_positive(&hits, negative_threshold) as u8 as f32));
        }

        items.push(ItemResult {
            id: item.id.clone(),
            recall_at_5: r5_val,
            mrr: metrics::reciprocal_rank(&hits, rel),
            latency_ms: ms,
            top_hits: hits.iter().take(5).map(|h| (h.source.clone(), h.score)).collect(),
        });
    }

    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pct = |p: f64| -> f64 {
        if latencies.is_empty() {
            return 0.0;
        }
        let idx = ((latencies.len() as f64 - 1.0) * p).round() as usize;
        latencies[idx]
    };

    let recall_at_5_ci = if r5_per_item.len() >= 5 {
        Some(metrics::bootstrap_ci(&r5_per_item, 1000, 0xC0FFEE))
    } else {
        None
    };

    let report = Report {
        name: retriever.name().to_string(),
        n_queries: golden.len(),
        n_negatives: golden.iter().filter(|g| g.relevant.is_empty()).count(),
        recall_at_5: metrics::mean(r5),
        recall_at_5_ci,
        recall_at_20: metrics::mean(r20),
        recall_at_50: metrics::mean(r50),
        mrr: metrics::mean(rr),
        ndcg_at_10: metrics::mean(nd),
        negative_fp_rate: metrics::mean(fp_flags),
        p50_latency_ms: pct(0.50),
        p95_latency_ms: pct(0.95),
    };

    Ok(RunOutput { report, items })
}

/// Pretty-print a set of reports side by side for quick comparison.
pub fn print_table(reports: &[Report]) {
    let f = |x: Option<f32>| x.map_or("   -  ".to_string(), |v| format!("{:.3}", v));
    let ci_str = |ci: Option<(f32, f32)>| {
        ci.map_or("         ".to_string(), |(lo, hi)| format!("[{:.3},{:.3}]", lo, hi))
    };
    println!(
        "{:<28} {:>7} {:>11} {:>7} {:>7} {:>7} {:>8} {:>8}",
        "retriever", "R@5", "R@5 CI", "R@20", "R@50", "MRR", "p50ms", "p95ms"
    );
    for r in reports {
        println!(
            "{:<28} {:>7} {:>11} {:>7} {:>7} {:>7} {:>8.0} {:>8.0}",
            r.name,
            f(r.recall_at_5),
            ci_str(r.recall_at_5_ci),
            f(r.recall_at_20),
            f(r.recall_at_50),
            f(r.mrr),
            r.p50_latency_ms,
            r.p95_latency_ms
        );
    }
}