#[derive(Clone, Debug)]
pub struct RawFact { pub claim: String, pub quote: String, pub source_id: i64 }

pub fn accept_fact(fact: &RawFact, chunk: &str) -> bool {
    if fact.quote.len() < 20 { return false; }
    let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    norm(chunk).contains(&norm(&fact.quote))
}

/// Stub reader: turns the first sentence of a chunk into a fact whose quote
/// is a verbatim substring of the chunk, so the gate always passes. Real LLM
/// extraction (schema-constrained) replaces this body later.
pub fn extract(chunk: &str, source_id: i64) -> Vec<RawFact> {
    let first = chunk.split(['.', '!', '?']).next().unwrap_or("").trim();
    if first.len() < 20 { return vec![]; }
    let quote = first.to_string() + ".";
    if !chunk.contains(&quote[..quote.len().saturating_sub(1)]) { return vec![]; }
    vec![RawFact { claim: first.to_string(), quote, source_id }]
}