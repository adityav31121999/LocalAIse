use crate::research::llm::{LlmClient, LlmMessage, LlmParams, parse_llm_json};
use chrono::Utc;

/// Raw fact extracted from a text chunk by the LLM reader.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct RawFact {
    pub claim: String,
    pub quote: String,
    pub source_id: i64,
    #[serde(default)]
    pub entities: Vec<String>,
    #[serde(default)]
    pub dates: Vec<String>,
    #[serde(default)]
    pub numbers: Vec<String>,
}

/// Normalize text for comparison (lowercase, collapse whitespace).
pub fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Cheap filter: quote must appear verbatim in chunk (after normalization).
pub fn accept_fact(fact: &RawFact, chunk: &str) -> bool {
    fact.quote.len() >= 20 && norm(chunk).contains(&norm(&fact.quote))
}

/// Extract atomic claims from a text chunk using LLM.
/// Falls back to heuristic (first sentence) if LLM fails.
pub async fn extract<C: LlmClient + ?Sized>(llm: &C, chunk: &str, source_id: i64) -> Vec<RawFact> {
    if chunk.trim().len() < 100 {
        return vec![];
    }

    let today = Utc::now().format("%B %d, %Y").to_string();
    let system = format!(
        "You are a precise fact extractor. Today's date is {}. From the provided text chunk, extract 1-5 \
        atomic, verifiable claims. Each claim must have a verbatim supporting quote from the chunk \
        (exact substring, 20+ chars). Include entities, dates, and numbers mentioned in the claim. \
        When the source mentions a date, preserve it exactly as stated. \
        Return ONLY valid JSON matching the schema.",
        today
    );

    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "facts": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "claim": { "type": "string", "minLength": 20 },
                        "quote": { "type": "string", "minLength": 20 },
                        "entities": { "type": "array", "items": { "type": "string" } },
                        "dates": { "type": "array", "items": { "type": "string" } },
                        "numbers": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["claim", "quote"],
                    "additionalProperties": false
                },
                "minItems": 1,
                "maxItems": 5
            }
        },
        "required": ["facts"],
        "additionalProperties": false
    });

    let user = format!("Text chunk:\n{chunk}\n\nExtract atomic facts with verbatim quotes as JSON.");

    let params = LlmParams {
        temperature: Some(0.1),
        json_schema: Some(schema),
        max_tokens: Some(1536),
        ..Default::default()
    };

    let messages = vec![
        LlmMessage { role: "system".into(), content: system.into() },
        LlmMessage { role: "user".into(), content: user },
    ];

    #[derive(serde::Deserialize)]
    struct ExtractResponse {
        facts: Vec<RawFact>,
    }

    match llm.complete(messages, params).await {
        Ok(resp) => {
            match parse_llm_json::<ExtractResponse>(&resp) {
                Ok(extracted) => {
                    // Attach source_id and filter by accept_fact
                    extracted.facts
                        .into_iter()
                        .map(|mut f| { f.source_id = source_id; f })
                        .filter(|f| accept_fact(f, chunk))
                        .collect()
                }
                Err(_) => fallback_extract(chunk, source_id),
            }
        }
        Err(_) => fallback_extract(chunk, source_id),
    }
}

/// Heuristic fallback: first sentence of chunk.
pub fn fallback_extract(chunk: &str, source_id: i64) -> Vec<RawFact> {
    let first = chunk.split(['.', '!', '?']).next().unwrap_or("").trim();
    if first.len() < 20 { return vec![]; }
    vec![RawFact {
        claim: first.to_string(),
        quote: first.to_string(),
        source_id,
        entities: vec![],
        dates: vec![],
        numbers: vec![],
    }]
}

/// Batch extraction for multiple chunks (more efficient).
pub async fn extract_batch<C: LlmClient>(
    llm: &C,
    chunks: &[(String, i64)], // (chunk_text, source_id)
) -> Vec<RawFact> {
    let mut all_facts = Vec::new();
    for (chunk, source_id) in chunks {
        let facts = extract(llm, chunk, *source_id).await;
        all_facts.extend(facts);
    }
    all_facts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::llm::MockLlmClient;
    use crate::research::llm::mock_json_response;

    #[tokio::test]
    async fn test_extract_mock() {
        let mock = MockLlmClient::new("test-model");
        let expected = serde_json::json!({
            "facts": [{
                "claim": "Tokyo population is 37 million",
                "quote": "Tokyo population is 37 million",
                "entities": ["Tokyo"],
                "dates": [],
                "numbers": ["37 million"]
            }]
        });
        mock.with_default_response(mock_json_response(&expected).unwrap());

        let chunk = "Tokyo population is 37 million as of 2023. It is the largest metro area.";
        let facts = extract(&mock, chunk, 1).await;
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].claim, "Tokyo population is 37 million");
        assert_eq!(facts[0].source_id, 1);
    }
}