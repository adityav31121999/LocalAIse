use crate::research::error::ResearchResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use url::Url;

/// A single search result from a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
    pub provider: String,
    pub rank: usize,
}

/// Trait for search providers.
#[async_trait]
pub trait SearchProvider: Send + Sync {
    /// Search for a query and return top results.
    async fn search(&self, query: &str, max_results: usize) -> ResearchResult<Vec<SearchResult>>;

    /// Provider name for logging/debugging.
    fn name(&self) -> &'static str;

    /// Endpoint this provider talks to, used by the network gate to apply the
    /// same policy/audit checks as page fetches. Empty means "unknown".
    fn endpoint(&self) -> String {
        String::new()
    }

    /// Whether this provider is currently available.
    async fn is_available(&self) -> bool {
        true
    }
}

/// URL canonicalization for deduplication.
pub fn canonicalize_url(url: &str) -> String {
    let Ok(mut parsed) = Url::parse(url) else { return url.to_string(); };

    // Strip fragment
    parsed.set_fragment(None);

    // Sort query parameters
    let mut pairs: Vec<_> = parsed.query_pairs()
        .filter(|(k, _)| !is_tracking_param(k))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    parsed.query_pairs_mut().clear();
    if pairs.is_empty() {
        parsed.set_query(None);
    } else {
        for (k, v) in pairs {
            parsed.query_pairs_mut().append_pair(&k, &v);
        }
    }

    // Normalize host (lowercase)
    if let Some(host) = parsed.host_str() {
        let _ = parsed.set_host(Some(&host.to_lowercase()));
    }

    // Remove default ports
    if (parsed.port() == Some(80) && parsed.scheme() == "http")
        || (parsed.port() == Some(443) && parsed.scheme() == "https")
    {
        let _ = parsed.set_port(None);
    }

    parsed.to_string()
}

fn is_tracking_param(key: &str) -> bool {
    matches!(
        key.to_lowercase().as_str(),
        "utm_source" | "utm_medium" | "utm_campaign" | "utm_term" | "utm_content"
            | "gclid" | "fbclid" | "msclkid" | "ref" | "ref_src" | "source"
            | "timestamp" | "ts" | "nonce" | "sig" | "signature" | "token"
            | "campaign_id" | "ad_id" | "creative_id" | "placement_id"
    )
}

/// Deduplicate search results by canonicalized URL.
pub fn dedupe_results(results: Vec<SearchResult>) -> Vec<SearchResult> {
    let mut seen = HashSet::new();
    results.into_iter().filter(|r| seen.insert(canonicalize_url(&r.url))).collect()
}

/// Source type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceType {
    Government,
    Educational,
    PrimarySource,
    News,
    Encyclopedia,
    Blog,
    Commercial,
    Forum,
    Unknown,
}

/// Classify a domain into a source type.
pub fn classify_domain(url: &str) -> SourceType {
    let Ok(parsed) = Url::parse(url) else { return SourceType::Unknown; };
    let host = parsed.host_str().unwrap_or("").to_lowercase();

    // Government
    if host.ends_with(".gov") || host.ends_with(".gov.uk") || host.ends_with(".gc.ca") ||
       host.contains(".gov.") {
        return SourceType::Government;
    }

    // Educational
    if host.ends_with(".edu") || host.ends_with(".ac.uk") || host.ends_with(".edu.au") ||
       host.contains(".edu.") {
        return SourceType::Educational;
    }

    // Known encyclopedias
    if host.contains("wikipedia.org") || host.contains("britannica.com") ||
       host.contains("encyclopedia.com") || host.contains("wikidata.org") {
        return SourceType::Encyclopedia;
    }

    // Known news domains
    let news_domains = [
        "reuters.com", "apnews.com", "bbc.com", "bbc.co.uk", "npr.org",
        "nytimes.com", "washingtonpost.com", "wsj.com", "ft.com",
        "theguardian.com", "economist.com", "bloomberg.com", "cnbc.com",
        "cnn.com", "abcnews.go.com", "cbsnews.com", "nbcnews.com",
        "aljazeera.com", "france24.com", "dw.com", "nhk.or.jp",
    ];
    if news_domains.iter().any(|d| host.contains(d)) {
        return SourceType::News;
    }

    // Primary sources (preprints, official docs, etc.)
    let primary_domains = [
        "arxiv.org", "biorxiv.org", "medrxiv.org", "ssrn.com", "osf.io",
        "github.com", "gitlab.com", "docs.rs", "crates.io", "pypi.org",
        "npmjs.com", "pub.dev", "maven.org", "nuget.org",
    ];
    if primary_domains.iter().any(|d| host.contains(d)) {
        return SourceType::PrimarySource;
    }

    // Commercial/shopping
    let commercial_domains = [
        "amazon.com", "ebay.com", "shopify.com", "etsy.com", "walmart.com",
        "target.com", "bestbuy.com", "newegg.com",
    ];
    if commercial_domains.iter().any(|d| host.contains(d)) {
        return SourceType::Commercial;
    }

    // Forums/social
    let forum_domains = [
        "reddit.com", "stackoverflow.com", "stackexchange.com", "quora.com",
        "medium.com", "substack.com", "twitter.com", "x.com", "linkedin.com",
        "facebook.com", "youtube.com", "discord.com", "github.com",
    ];
    if forum_domains.iter().any(|d| host.contains(d)) {
        return SourceType::Forum;
    }

    SourceType::Unknown
}

/// Source quality score components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceScore {
    pub domain_type: SourceType,
    pub recency_score: f32,      // 0.0 - 1.0 (newer = higher)
    pub content_quality: f32,    // 0.0 - 1.0 (text/boilerplate ratio)
    pub authority_score: f32,    // 0.0 - 1.0 (domain authority)
    pub composite: f32,          // Weighted composite
}

impl SourceScore {
    pub fn compute(url: &str, text: &str, published_at: Option<chrono::DateTime<chrono::Utc>>) -> Self {
        let domain_type = classify_domain(url);

        // Recency score: exponential decay, half-life ~1 year
        let recency_score = published_at.map(|dt| {
            let days_old = (chrono::Utc::now() - dt).num_days() as f32;
            (-days_old / 365.0).exp().max(0.0)
        }).unwrap_or(0.5);

        // Content quality: text length vs boilerplate estimate
        let lines: Vec<&str> = text.lines().collect();
        let non_empty: Vec<&str> = lines.iter().filter(|l| l.trim().len() > 20).copied().collect();
        let content_quality = if lines.is_empty() { 0.0 } else {
            (non_empty.len() as f32 / lines.len() as f32).min(1.0)
        };

        // Authority by domain type
        let authority_score = match domain_type {
            SourceType::Government => 1.0,
            SourceType::Educational => 0.95,
            SourceType::PrimarySource => 0.9,
            SourceType::Encyclopedia => 0.85,
            SourceType::News => 0.8,
            SourceType::Commercial => 0.5,
            SourceType::Blog => 0.4,
            SourceType::Forum => 0.3,
            SourceType::Unknown => 0.4,
        };

        // Weighted composite
        let composite = 0.35 * authority_score
            + 0.25 * content_quality
            + 0.25 * recency_score
            + 0.15 * (if text.len() > 5000 { 1.0 } else { text.len() as f32 / 5000.0 });

        Self { domain_type, recency_score, content_quality, authority_score, composite }
    }
}

/// Corroboration cluster for claims across sources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorroborationCluster {
    pub claim_signature: String,  // Normalized claim for grouping
    pub facts: Vec<CorroboratedFact>,
    pub source_count: usize,
    pub conflicting: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorroboratedFact {
    pub claim: String,
    pub quote: String,
    pub source_id: i64,
    pub source_url: String,
    pub source_score: SourceScore,
    pub verification_label: crate::research::roles::verify::VerificationLabel,
}

/// Token-set Jaccard similarity between two claims, catching near-paraphrases
/// that substring matching misses ("founded in 2020" vs "in 2020 it was founded").
pub fn token_jaccard(a: &str, b: &str) -> f32 {
    let tokens = |s: &str| -> std::collections::HashSet<String> {
        s.to_lowercase()
            .split_whitespace()
            .filter(|w| w.len() > 2)
            .map(|w| w.to_string())
            .collect()
    };
    let ta = tokens(a);
    let tb = tokens(b);
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    ta.intersection(&tb).count() as f32 / ta.union(&tb).count() as f32
}

/// Group facts into corroboration clusters.
pub fn cluster_claims(facts: &[CorroboratedFact]) -> Vec<CorroborationCluster> {
    const CLUSTER_THRESHOLD: f32 = 0.6;
    let mut clusters: Vec<CorroborationCluster> = Vec::new();

    for fact in facts {
        let sig = normalize_for_clustering(&fact.claim);
        let mut placed = false;
        for cluster in clusters.iter_mut() {
            if token_jaccard(&cluster.claim_signature, &sig) >= CLUSTER_THRESHOLD {
                cluster.facts.push(fact.clone());
                placed = true;
                break;
            }
        }
        if !placed {
            clusters.push(CorroborationCluster {
                claim_signature: sig,
                facts: vec![fact.clone()],
                source_count: 0,
                conflicting: false,
            });
        }
    }

    // Recompute source counts and conflict flags after grouping.
    for cluster in clusters.iter_mut() {
        let sources: std::collections::HashSet<_> = cluster.facts.iter().map(|f| f.source_id).collect();
        let has_supported = cluster.facts.iter().any(|f| f.verification_label == crate::research::roles::verify::VerificationLabel::Supported);
        let has_contradicted = cluster.facts.iter().any(|f| f.verification_label == crate::research::roles::verify::VerificationLabel::Contradicted);
        cluster.source_count = sources.len();
        cluster.conflicting = has_supported && has_contradicted;
    }

    // Sort by source count descending
    clusters.sort_by_key(|c| std::cmp::Reverse(c.source_count));
    clusters
}

fn normalize_for_clustering(claim: &str) -> String {
    claim.to_lowercase()
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonicalize_url() {
        assert_eq!(
            canonicalize_url("https://example.com/page?utm_source=google&b=2&a=1#fragment"),
            "https://example.com/page?a=1&b=2"
        );
        assert_eq!(
            canonicalize_url("https://EXAMPLE.COM:443/page"),
            "https://example.com/page"
        );
    }

    #[test]
    fn test_classify_domain() {
        assert_eq!(classify_domain("https://www.cdc.gov/health"), SourceType::Government);
        assert_eq!(classify_domain("https://harvard.edu/research"), SourceType::Educational);
        assert_eq!(classify_domain("https://en.wikipedia.org/wiki/AI"), SourceType::Encyclopedia);
        assert_eq!(classify_domain("https://reuters.com/world"), SourceType::News);
        assert_eq!(classify_domain("https://arxiv.org/abs/1234"), SourceType::PrimarySource);
        assert_eq!(classify_domain("https://random-blog.com/post"), SourceType::Unknown);
    }

    #[test]
    fn test_scorer() {
        let text = "This is a substantial article with many lines of content.\n".repeat(100);
        let score = SourceScore::compute("https://example.com", &text, Some(chrono::Utc::now()));
        assert!(score.composite > 0.0 && score.composite <= 1.0);
        assert_eq!(score.domain_type, SourceType::Unknown);
    }

}