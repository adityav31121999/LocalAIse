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
}

#[derive(Debug, Clone)]
pub struct FactVerification {
    pub fact_index: usize,
    pub stage1_pass: bool,
    pub stage2_label: Option<VerificationLabel>,
    pub stage2_reasoning: Option<String>,
}

/// Run two-stage verification on all facts.
/// Stage 1: Exact quote containment (cheap filter).
/// Stage 2: LLM entailment check (semantic verification).
pub async fn verify<C: LlmClient + ?Sized>(
    llm: Option<&C>,
    facts: Vec<Fact>,
    store: &EvidenceStore,
) -> Verdict {
    let normed: HashMap<i64, String> =
        store.sources().iter().map(|s| (s.id, norm(&s.text))).collect();

    let mut details = Vec::new();
    let (mut supported, mut unsupported) = (Vec::new(), Vec::new());

    for (idx, f) in facts.into_iter().enumerate() {
        // Stage 1: Exact quote containment
        let q = norm(&f.quote);
        let stage1_pass = !q.is_empty() && normed.get(&f.source_id).is_some_and(|t| t.contains(&q));

        let mut stage2_label = None;
        let mut stage2_reasoning = None;

        // Stage 2: LLM entailment (only if stage 1 passes and LLM available)
        if stage1_pass {
            if let Some(llm_client) = llm {
                match verify_entailment(llm_client, &f, &normed).await {
                    Ok((label, reasoning)) => {
                        stage2_label = Some(label.clone());
                        stage2_reasoning = Some(reasoning);
                        match label {
                            VerificationLabel::Supported => supported.push(f),
                            _ => unsupported.push(f),
                        }
                    }
                    Err(_) => {
                        // LLM error: fall back to stage 1 result
                        if stage1_pass { supported.push(f); } else { unsupported.push(f); }
                    }
                }
            } else {
                // No LLM: rely on stage 1
                if stage1_pass { supported.push(f); } else { unsupported.push(f); }
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

    Verdict { supported, unsupported, details }
}

/// Stage 2: LLM entailment check.
/// Given a claim+quote and source context, determine if the source supports, contradicts, or doesn't address the claim.
async fn verify_entailment<C: LlmClient + ?Sized>(
    llm: &C,
    fact: &Fact,
    normed: &HashMap<i64, String>,
) -> Result<(VerificationLabel, String), ResearchError> {
    let source_text = normed.get(&fact.source_id)
        .ok_or_else(|| ResearchError::SourceNotFound { source_id: fact.source_id })?;

    // Use a window around the quote for context
    let context = extract_context(source_text, &fact.quote, 500);

    let system = "You are a precise fact verifier. Given a claim with its supporting quote \
        and the surrounding source context, determine if the context:\n\
        - SUPPORTED: The quote and context clearly support the claim.\n\
        - CONTRADICTED: The context contradicts the claim.\n\
        - UNSUPPORTED: The context doesn't contain enough information to verify the claim.\n\
        Return ONLY valid JSON matching the schema.";

    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "label": { "type": "string", "enum": ["supported", "contradicted", "unsupported"] },
            "reasoning": { "type": "string", "minLength": 10 }
        },
        "required": ["label", "reasoning"],
        "additionalProperties": false
    });

    let user = format!(
        "Claim: {}\n\nVerbatim quote from source: {}\n\nSource context (window around quote):\n{}\n\n\
        Does the context support, contradict, or not address the claim? Return JSON.",
        fact.claim, fact.quote, context
    );

    let params = LlmParams {
        temperature: Some(0.0),
        json_schema: Some(schema),
        max_tokens: Some(512),
        ..Default::default()
    };

    let messages = vec![
        LlmMessage { role: "system".into(), content: system.into() },
        LlmMessage { role: "user".into(), content: user },
    ];

    #[derive(serde::Deserialize)]
    struct VerifyResponse {
        label: VerificationLabel,
        reasoning: String,
    }

    let resp = llm.complete(messages, params).await?;
    let parsed = parse_llm_json::<VerifyResponse>(&resp)?;
    Ok((parsed.label, parsed.reasoning))
}

/// Extract a context window around the quote in the source text.
fn extract_context(source: &str, quote: &str, window: usize) -> String {
    let source_lower = source.to_lowercase();
    let quote_lower = quote.to_lowercase();

    if let Some(pos) = source_lower.find(&quote_lower) {
        let start = pos.saturating_sub(window);
        let end = (pos + quote.len() + window).min(source.len());
        source[start..end].to_string()
    } else {
        // Quote not found exactly; return beginning
        source.chars().take(window * 2).collect()
    }
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

    Verdict { supported, unsupported, details }
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
}