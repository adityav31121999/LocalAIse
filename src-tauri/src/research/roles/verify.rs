use std::collections::HashMap;
use crate::research::fact::Fact;
use crate::research::roles::reader::norm;
use crate::research::store::EvidenceStore;
use crate::research::llm::{LlmClient, LlmMessage, LlmParams, parse_llm_json};
use crate::research::error::ResearchError;

/// Verification label from stage 2.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerificationLabel {
    Supported,
    Contradicted,
    Unsupported,
}

/// Verdict from two-stage verification.
pub struct Verdict {
    pub supported: Vec<Fact>,
    pub unsupported: Vec<Fact>,
    /// Per-fact verification details for logging/metrics.
    pub details: Vec<FactVerification>,
    /// Stage-2 facts whose LLM call failed (the fact is kept via stage 1,
    /// and this counts them so the UI/transcript can surface the degradation).
    pub stage2_failed: usize,
}

/// Number of facts verified per stage-2 LLM call. One sequential call per
/// fact made Deep-mode runs (60+ facts) take minutes of serial completions;
/// batching cuts that proportionally while keeping the entailment check strict.
const STAGE2_BATCH_SIZE: usize = 4;

#[derive(Debug, Clone)]
pub struct FactVerification {
    pub fact_index: usize,
    pub stage1_pass: bool,
    pub stage2_label: Option<VerificationLabel>,
    pub stage2_reasoning: Option<String>,
}

/// Run two-stage verification on all facts.
/// Stage 1: Exact quote containment (cheap filter).
/// Stage 2: LLM entailment check (semantic verification), batched
/// STAGE2_BATCH_SIZE facts per call so a large fact set doesn't mean
/// dozens of serial completions.
pub async fn verify<C: LlmClient + ?Sized>(
    llm: Option<&C>,
    facts: Vec<Fact>,
    store: &EvidenceStore,
) -> Verdict {
    let normed: HashMap<i64, String> =
        store.sources().iter().map(|s| (s.id, norm(&s.text))).collect();

    // Stage 1 pass first; stage 2 runs on the survivors.
    let mut stage1: Vec<(usize, Fact, bool)> = Vec::with_capacity(facts.len());
    for (idx, f) in facts.into_iter().enumerate() {
        let q = norm(&f.quote);
        let stage1_pass = !q.is_empty() && normed.get(&f.source_id).is_some_and(|t| t.contains(&q));
        stage1.push((idx, f, stage1_pass));
    }

    // Stage 2: LLM entailment, batched. Facts that fail stage 1 never reach the LLM.
    let mut stage2: HashMap<usize, (VerificationLabel, String)> = HashMap::new();
    let mut stage2_failed = 0usize;
    if let Some(llm_client) = llm {
        let pending: Vec<usize> = stage1.iter().enumerate()
            .filter(|(_, (_, _, pass))| *pass)
            .map(|(i, _)| i)
            .collect();
        for batch in pending.chunks(STAGE2_BATCH_SIZE) {
            let batch_facts: Vec<&Fact> = batch.iter().map(|i| &stage1[*i].1).collect();
            match verify_entailment_batch(llm_client, &batch_facts, &normed).await {
                Ok(results) => {
                    let mut filled = std::collections::HashSet::new();
                    for (batch_pos, label, reasoning) in results {
                        if let Some(&i) = batch.get(batch_pos) {
                            stage2.insert(stage1[i].0, (label, reasoning));
                            filled.insert(batch_pos);
                        }
                    }
                    // Facts the model silently omitted fall back to stage 1 and count as failed.
                    stage2_failed += batch.len().saturating_sub(filled.len());
                }
                Err(_) => {
                    // Batch failed (timeout, parse, server hang): every fact in
                    // the batch falls back to stage-1-only support.
                    stage2_failed += batch.len();
                }
            }
        }
    }

    let mut details = Vec::with_capacity(stage1.len());
    let (mut supported, mut unsupported) = (Vec::new(), Vec::new());

    for (idx, f, stage1_pass) in stage1 {
        let mut stage2_label = None;
        let mut stage2_reasoning = None;

        if stage1_pass {
            if let Some((label, reasoning)) = stage2.get(&idx) {
                stage2_label = Some(label.clone());
                stage2_reasoning = Some(reasoning.clone());
                match label {
                    VerificationLabel::Supported => supported.push(f),
                    _ => unsupported.push(f),
                }
            } else {
                // Stage 2 unavailable, skipped (no LLM) or failed: fall back to stage 1.
                supported.push(f);
            }
        } else {
            unsupported.push(f);
        }

        details.push(FactVerification {
            fact_index: idx,
            stage1_pass,
            stage2_label,
            stage2_reasoning,
        });
    }

    Verdict { supported, unsupported, details, stage2_failed }
}

/// Stage 2: LLM entailment check, batched.
/// Given a claim+quote and source context for each fact in the batch, determine if the
/// source supports, contradicts, or doesn't address the claim. Returns one
/// (batch_position, label, reasoning) triple per fact the model judged.
async fn verify_entailment_batch<C: LlmClient + ?Sized>(
    llm: &C,
    facts: &[&Fact],
    normed: &HashMap<i64, String>,
) -> Result<Vec<(usize, VerificationLabel, String)>, ResearchError> {
    // One compact context window per fact; the batch as a whole stays well
    // within even small local-model context budgets.
    let mut blocks = String::new();
    for (i, f) in facts.iter().enumerate() {
        let source_text = normed.get(&f.source_id)
            .ok_or(ResearchError::SourceNotFound { source_id: f.source_id })?;
        let context = extract_context(source_text, &f.quote, 300);
        blocks.push_str(&format!(
            "[{}] Claim: {}\nQuote: {}\nContext:\n{}\n\n",
            i + 1, f.claim, f.quote, context
        ));
    }

    let system = "You are a precise fact verifier. For each fact below, decide whether its \
        source context:\n\
        - SUPPORTED: The quote and context clearly support the claim.\n\
        - CONTRADICTED: The context contradicts the claim.\n\
        - UNSUPPORTED: The context doesn't contain enough information to verify the claim.\n\
        Return ONLY valid JSON matching the schema, with exactly one results entry per fact, \
        keyed by the fact's number.";

    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "results": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "index": { "type": "integer", "minimum": 1 },
                        "label": { "type": "string", "enum": ["supported", "contradicted", "unsupported"] },
                        "reasoning": { "type": "string" }
                    },
                    "required": ["index", "label", "reasoning"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["results"],
        "additionalProperties": false
    });

    let user = format!(
        "Verify these {} facts. Return JSON with one results entry per fact.\n\n{}",
        facts.len(), blocks
    );

    let params = LlmParams {
        temperature: Some(0.0),
        json_schema: Some(schema),
        max_tokens: Some(1024),
        ..Default::default()
    };

    let messages = vec![
        LlmMessage { role: "system".into(), content: system.into() },
        LlmMessage { role: "user".into(), content: user },
    ];

    #[derive(serde::Deserialize)]
    struct BatchResponse {
        results: Vec<BatchEntry>,
    }
    #[derive(serde::Deserialize)]
    struct BatchEntry {
        index: usize,
        label: VerificationLabel,
        reasoning: String,
    }

    let resp = tokio::time::timeout(
        std::time::Duration::from_secs(180),
        llm.complete(messages, params),
    )
    .await
    .map_err(|_| ResearchError::Timeout { secs: 180, url: format!("llm:{}", llm.model_id()) })??;
    let parsed = parse_llm_json::<BatchResponse>(&resp)?;
    Ok(parsed.results.into_iter()
        // Model indices are 1-based; anything out of range is dropped instead of panicking.
        .filter_map(|e| e.index.checked_sub(1).map(|i| (i, e.label, e.reasoning)))
        .filter(|(i, _, _)| *i < facts.len())
        .collect())
}

/// Extract a context window around the quote in the source text.
///
/// The source handed in is already normalized (lowercased, whitespace
/// collapsed) by `verify`. Everything is done on one owned lowercased string and
/// byte offsets are snapped to char boundaries, so multi-byte UTF-8 content
/// cannot panic the slice.
fn extract_context(source: &str, quote: &str, window: usize) -> String {
    let lower = source.to_lowercase();
    let quote_lower = quote.to_lowercase();

    let (pos, quote_len) = match lower.find(&quote_lower) {
        Some(pos) => (pos, quote_lower.len()),
        // Quote not found exactly; return the beginning of the text.
        None => return lower.chars().take(window * 2).collect(),
    };

    let mut start = pos.saturating_sub(window);
    while start > 0 && !lower.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = (pos + quote_len + window).min(lower.len());
    while end < lower.len() && !lower.is_char_boundary(end) {
        end += 1;
    }
    lower[start..end].to_string()
}

/// Synchronous version for backward compatibility (stage 1 only).
pub fn verify_stage1_only(facts: Vec<Fact>, store: &EvidenceStore) -> Verdict {
    let normed: HashMap<i64, String> =
        store.sources().iter().map(|s| (s.id, norm(&s.text))).collect();

    let mut supported = Vec::new();
    let mut unsupported = Vec::new();
    let mut details = Vec::new();

    for (idx, f) in facts.into_iter().enumerate() {
        let q = norm(&f.quote);
        let stage1_pass = !q.is_empty() && normed.get(&f.source_id).is_some_and(|t| t.contains(&q));

        if stage1_pass {
            supported.push(f);
        } else {
            unsupported.push(f);
        }

        details.push(FactVerification {
            fact_index: idx,
            stage1_pass,
            stage2_label: None,
            stage2_reasoning: None,
        });
    }

    Verdict { supported, unsupported, details, stage2_failed: 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::store::EvidenceStore;

    fn fact(source_id: i64, claim: &str, quote: &str) -> Fact {
        Fact { sub_q: 0, source_id, claim: claim.into(), quote: quote.into() }
    }

    #[test]
    fn test_stage1_splits_supported_from_fabricated() {
        let mut store = EvidenceStore::new();
        let id = store.add_source("file:a.md", "a", "Alpha  beta\ngamma delta epsilon zeta.");

        let v = verify_stage1_only(vec![
            fact(id, "alpha beta gamma delta", "alpha beta gamma delta"),
            fact(id, "this sentence was never in the source", "this sentence was never in the source"),
            fact(999, "alpha beta gamma delta", "alpha beta gamma delta"),
        ], &store);

        assert_eq!(v.supported.len(), 1);
        assert_eq!(v.unsupported.len(), 2);
        assert!(v.details[0].stage1_pass);
        assert!(!v.details[1].stage1_pass);
        assert!(!v.details[2].stage1_pass);
    }

    #[test]
    fn test_extract_context() {
        let source = "The quick brown fox jumps over the lazy dog. The fox is very quick.";
        let quote = "brown fox jumps";
        let ctx = extract_context(source, quote, 20);
        assert!(ctx.contains("brown fox jumps"));
    }

    #[test]
    fn test_extract_context_does_not_panic_on_multibyte_text() {
        // Multi-byte characters make byte-window offsets land inside a char;
        // the slice must snap to char boundaries instead of panicking.
        let source = "日本語のテキストはとても長く、重要な事実をいくつか含んでいます。同じ内容をもう一度繰り返します。";
        let quote = "重要な事実";
        let ctx = extract_context(source, quote, 12);
        assert!(ctx.contains("重要な事実"));
    }
}