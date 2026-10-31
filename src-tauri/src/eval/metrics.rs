use std::collections::HashSet;

/// Bootstrap 95% CI for a per-item metric (e.g., recall@5 per query).
/// Uses xorshift PRNG — no `rand` dependency.
pub fn bootstrap_ci(per_item: &[f32], iters: usize, seed: u64) -> (f32, f32) {
    let mut s = seed | 1;
    let mut next = || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    let n = per_item.len();
    let mut means: Vec<f32> = (0..iters)
        .map(|_| {
            let sum: f32 = (0..n).map(|_| per_item[(next() as usize) % n]).sum();
            sum / n as f32
        })
        .collect();
    means.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (means[iters / 40], means[iters * 39 / 40]) // 2.5th and 97.5th percentiles
}

/// A retrieved item, reduced to what evaluation needs.
#[derive(Debug, Clone)]
pub struct Hit {
    pub source: String,
    pub text: String,
    pub score: f32,
}

/// One gold relevance judgement: a chunk from `source` containing `contains`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Relevant {
    pub source: String,
    pub contains: String,
}

impl Relevant {
    pub fn matches(&self, hit: &Hit) -> bool {
        hit.source == self.source && hit.text.contains(&self.contains)
    }
}

/// Index of the first hit that satisfies any relevance judgement, if any.
fn first_relevant_rank(hits: &[Hit], rel: &[Relevant]) -> Option<usize> {
    hits.iter().position(|h| rel.iter().any(|r| r.matches(h)))
}

/// Fraction of relevance judgements satisfied within the top k hits.
/// Each judgement counts once even if several hits match it.
pub fn recall_at_k(hits: &[Hit], rel: &[Relevant], k: usize) -> Option<f32> {
    if rel.is_empty() {
        return None; // negatives are excluded from recall
    }
    let top = &hits[..hits.len().min(k)];
    let found = rel
        .iter()
        .filter(|r| top.iter().any(|h| r.matches(h)))
        .count();
    Some(found as f32 / rel.len() as f32)
}

/// Reciprocal rank of the first relevant hit (0 if none in the list).
pub fn reciprocal_rank(hits: &[Hit], rel: &[Relevant]) -> Option<f32> {
    if rel.is_empty() {
        return None;
    }
    Some(match first_relevant_rank(hits, rel) {
        Some(i) => 1.0 / (i as f32 + 1.0),
        None => 0.0,
    })
}

/// Binary-relevance nDCG@k. Gains are 1 for a relevant hit, 0 otherwise.
/// Duplicate hits from the same judgement only count once, which keeps
/// the ideal DCG achievable.
pub fn ndcg_at_k(hits: &[Hit], rel: &[Relevant], k: usize) -> Option<f32> {
    if rel.is_empty() {
        return None;
    }
    let mut claimed: HashSet<usize> = HashSet::new();
    let mut dcg = 0.0f32;
    for (i, h) in hits.iter().take(k).enumerate() {
        if let Some(j) = rel.iter().position(|r| r.matches(h)) {
            if claimed.insert(j) {
                dcg += 1.0 / (i as f32 + 2.0).log2();
            }
        }
    }
    let ideal_hits = rel.len().min(k);
    let idcg: f32 = (0..ideal_hits)
        .map(|i| 1.0 / (i as f32 + 2.0).log2())
        .sum();
    Some(if idcg > 0.0 { dcg / idcg } else { 0.0 })
}

/// For negatives: did the system return anything above `threshold`?
/// Only meaningful once the retriever emits calibrated scores.
pub fn false_positive(hits: &[Hit], threshold: f32) -> bool {
    hits.first().is_some_and(|h| h.score >= threshold)
}

/// Mean of the defined values; None if no item contributed.
pub fn mean(xs: impl IntoIterator<Item = Option<f32>>) -> Option<f32> {
    let (sum, n) = xs
        .into_iter()
        .flatten()
        .fold((0.0f32, 0usize), |(s, n), x| (s + x, n + 1));
    (n > 0).then(|| sum / n as f32)
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Report {
    pub name: String,
    pub n_queries: usize,
    pub n_negatives: usize,
    pub recall_at_5: Option<f32>,
    pub recall_at_5_ci: Option<(f32, f32)>,
    pub recall_at_20: Option<f32>,
    pub recall_at_50: Option<f32>,
    pub mrr: Option<f32>,
    pub ndcg_at_10: Option<f32>,
    pub negative_fp_rate: Option<f32>,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
}