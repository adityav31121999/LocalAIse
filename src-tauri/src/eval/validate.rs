use crate::eval::metrics::Hit;
use crate::index::db::ProjectIndex;

/// Validates that every relevance judgement matches at least one chunk.
/// Returns a list of judgements that matched nothing (silent recall losses).
pub fn validate_golden_against_index(
    golden: &[crate::eval::harness::GoldItem],
    index: &ProjectIndex,
) -> Vec<UnmatchedJudgement> {
    // Build searchable hits from the index (same tokenization as BM25)
    let hits: Vec<Hit> = index
        .chunks
        .iter()
        .map(|c| Hit {
            source: c.file_name.clone(),
            text: c.content.clone(),
            score: 1.0,
        })
        .collect();

    let mut unmatched = Vec::new();

    for item in golden {
        for rel in &item.relevant {
            let matched = hits.iter().any(|h| rel.matches(h));
            if !matched {
                unmatched.push(UnmatchedJudgement {
                    query_id: item.id.clone(),
                    query: item.query.clone(),
                    expected_source: rel.source.clone(),
                    expected_contains: rel.contains.clone(),
                });
            }
        }
    }

    unmatched
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UnmatchedJudgement {
    pub query_id: String,
    pub query: String,
    pub expected_source: String,
    pub expected_contains: String,
}

/// Load index and validate; panics with details if any judgement is unmatched.
pub fn validate_or_panic(golden_path: &std::path::Path, index_path: &std::path::Path) {
    let golden = crate::eval::harness::load_golden(golden_path)
        .unwrap_or_else(|e| panic!("failed to load golden set: {e}"));
    let index = crate::index::db::IndexDb::new(index_path).load();
    let unmatched = validate_golden_against_index(&golden, &index);

    if !unmatched.is_empty() {
        eprintln!("{} golden judgements match NO chunk in the index:", unmatched.len());
        for u in &unmatched {
            eprintln!(
                "  query={} source={} contains={}",
                u.query_id, u.expected_source, u.expected_contains
            );
        }
        panic!("Golden set validation failed — see unmatched judgements above");
    }
}