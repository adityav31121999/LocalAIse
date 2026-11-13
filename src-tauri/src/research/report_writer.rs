use crate::research::fact::Fact;
use crate::research::roles::verify::{VerificationLabel, FactVerification};
use crate::research::store::Source;
use crate::research::search::CorroboratedFact;
use chrono::Utc;

/// Citation reference for inline citations.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Citation {
    pub fact_id: usize,
    pub source_id: i64,
    pub source_url: String,
    pub source_title: String,
    pub quote: String,
    pub verification: VerificationLabel,
}

/// Enhanced section with structured content.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WrittenSection {
    pub title: String,
    pub content: String,           // Markdown content with inline citations
    pub citations: Vec<Citation>,  // All citations used in this section
    pub confidence: f32,           // 0.0 - 1.0 based on source count/quality
    pub conflicting_evidence: Option<String>,
    pub limitations: Option<String>,
}

/// Write a section from verified facts with proper citations.
pub fn write_report_section(
    title: &str,
    facts: &[&Fact],
    all_facts: &[Fact],
    verifications: &[FactVerification],
    sources: &[Source],
    corroborated: &[CorroboratedFact],
) -> WrittenSection {
    if facts.is_empty() {
        return WrittenSection {
            title: title.to_string(),
            content: "_No supported evidence found._\n".to_string(),
            citations: vec![],
            confidence: 0.0,
            conflicting_evidence: None,
            limitations: None,
        };
    }

    // Build citation map: fact index -> citation
    let mut citations = Vec::new();
    let mut citation_map: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    
    for (fact_idx, fact) in facts.iter().enumerate() {
        // Find the original fact index in all_facts
        let orig_idx = all_facts.iter().position(|f| 
            f.sub_q == fact.sub_q && f.source_id == fact.source_id && f.claim == fact.claim
        ).unwrap_or(fact_idx);

        let verification = verifications.get(orig_idx);
        let label = verification.and_then(|v| v.stage2_label.clone()).unwrap_or(VerificationLabel::Supported);

        // Find source info
        let source = sources.iter().find(|s| s.id == fact.source_id);
        let (source_url, source_title) = source.map(|s| (s.url.clone(), s.title.clone())).unwrap_or_else(|| {
            // Fallback to corroborated facts
            corroborated.iter()
                .find(|c| c.source_id == fact.source_id)
                .map(|c| (c.source_url.clone(), "".to_string()))
                .unwrap_or_else(|| (format!("source:{}", fact.source_id), "".to_string()))
        });

        let citation = Citation {
            fact_id: citations.len(),
            source_id: fact.source_id,
            source_url,
            source_title,
            quote: fact.quote.clone(),
            verification: label,
        };
        citation_map.insert(fact_idx, citation.fact_id);
        citations.push(citation);
    }

    // Calculate confidence based on number of unique sources and their quality
    let unique_sources: std::collections::HashSet<i64> = facts.iter().map(|f| f.source_id).collect();
    let avg_quality = if !unique_sources.is_empty() {
        sources.iter()
            .filter(|s| unique_sources.contains(&s.id))
            .map(|s| {
                crate::research::search::SourceScore::compute(&s.url, &s.text, None).composite
            })
            .sum::<f32>() / unique_sources.len() as f32
    } else { 0.0 };
    
    let source_count_factor = (unique_sources.len() as f32 / 5.0).min(1.0);
    let confidence = (0.6 * avg_quality + 0.4 * source_count_factor).min(1.0);

    // Build content with inline citations
    let mut content = String::new();
    content.push_str(&format!("## {title}\n\n"));

    for (i, fact) in facts.iter().enumerate() {
        let citation_id = citation_map.get(&i).copied().unwrap_or(0);
        content.push_str(&format!("{} [{}]({})\n\n", fact.claim, citation_id + 1, citations[citation_id].source_url));
    }

    // Check for conflicting evidence
    let conflicting = find_conflicting_evidence(facts, all_facts, verifications, corroborated);
    
    // Generate limitations
    let limitations = generate_limitations(facts, &citations, confidence);

    WrittenSection {
        title: title.to_string(),
        content,
        citations,
        confidence,
        conflicting_evidence: conflicting,
        limitations,
    }
}

/// Find claims that have contradictory evidence.
fn find_conflicting_evidence(
    facts: &[&Fact],
    _all_facts: &[Fact],
    _verifications: &[FactVerification],
    corroborated: &[CorroboratedFact],
) -> Option<String> {
    let mut conflicts = Vec::new();

    for fact in facts {
        // Find corroborated facts with same claim but different verification
        for corr in corroborated {
            if claims_similar(&fact.claim, &corr.claim) && corr.verification_label == VerificationLabel::Contradicted {
                conflicts.push(format!(
                    "Claim \"{}\" is contradicted by source {} ({})",
                    fact.claim, corr.source_id, corr.source_url
                ));
            }
        }
    }

    if conflicts.is_empty() {
        None
    } else {
        Some(format!("### Conflicting Evidence\n\n{}\n", conflicts.join("\n\n")))
    }
}

/// Generate limitations section based on evidence quality.
fn generate_limitations(
    facts: &[&Fact],
    citations: &[Citation],
    confidence: f32,
) -> Option<String> {
    let mut limitations = Vec::new();

    let unique_sources: std::collections::HashSet<i64> = facts.iter().map(|f| f.source_id).collect();
    
    if unique_sources.len() < 2 {
        limitations.push("This section is based on a single source; independent verification is recommended.".to_string());
    }
    
    if confidence < 0.5 {
        limitations.push("Source quality is below average; conclusions should be treated as preliminary.".to_string());
    }

    let unsupported_count = citations.iter().filter(|c| c.verification == VerificationLabel::Unsupported).count();
    if unsupported_count > 0 {
        limitations.push(format!("{} citation(s) could not be fully verified against their sources.", unsupported_count));
    }

    if limitations.is_empty() {
        None
    } else {
        Some(format!("### Limitations\n\n{}\n", limitations.join("\n\n")))
    }
}

fn claims_similar(a: &str, b: &str) -> bool {
    let a_norm = normalize_claim(a);
    let b_norm = normalize_claim(b);
    a_norm == b_norm || a_norm.contains(&b_norm) || b_norm.contains(&a_norm)
}

fn normalize_claim(s: &str) -> String {
    s.to_lowercase()
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Write the full report from all sections.
pub fn write_report(
    sections: Vec<WrittenSection>,
    all_sources: &[Source],
) -> String {
    let mut report = String::new();

    // Executive summary
    let today = Utc::now().format("%B %d, %Y").to_string();
    report.push_str("# Research Report\n\n");
    report.push_str(&format!("*Generated on: {}*\n\n", today));
    report.push_str("## Executive Summary\n\n");
    
    let total_citations: usize = sections.iter().map(|s| s.citations.len()).sum();
    let avg_confidence = if !sections.is_empty() {
        sections.iter().map(|s| s.confidence).sum::<f32>() / sections.len() as f32
    } else { 0.0 };

    report.push_str(&format!(
        "This report covers {} sections with {} total citations. Average confidence: {:.0}%.\n\n",
        sections.len(), total_citations, avg_confidence * 100.0
    ));

    // Key findings
    report.push_str("## Key Findings\n\n");
    for section in &sections {
        if section.confidence > 0.6 && !section.citations.is_empty() {
            report.push_str(&format!("### {}\n{}\n", section.title, section.citations.first().map(|c| &c.quote).unwrap_or(&"".to_string())));
        }
    }

    // Detailed sections
    report.push_str("\n---\n\n");
    for section in &sections {
        report.push_str(&section.content);
        
        if let Some(conflicting) = &section.conflicting_evidence {
            report.push_str(conflicting);
            report.push_str("\n");
        }
        
        if let Some(lim) = &section.limitations {
            report.push_str(lim);
            report.push_str("\n");
        }
        
        report.push_str("---\n\n");
    }

    // Sources with quality scores
    report.push_str("## Sources\n\n");
    let mut cited_sources: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for section in &sections {
        for citation in &section.citations {
            cited_sources.insert(citation.source_id);
        }
    }

    let mut source_list: Vec<_> = all_sources.iter()
        .filter(|s| cited_sources.contains(&s.id))
        .map(|s| {
            let score = crate::research::search::SourceScore::compute(&s.url, &s.text, None);
            format!("- **[{}]** {} — {} (type: {:?}, quality: {:.2})", 
                s.id, s.url, s.title, score.domain_type, score.composite)
        })
        .collect();
    source_list.sort();
    report.push_str(&source_list.join("\n"));
    report.push('\n');

    // Methodology
    report.push_str("\n## Methodology\n\n");
    report.push_str("This report was generated through an automated research pipeline:\n\n");
    report.push_str("1. **Planning**: Question decomposed into focused sub-questions using LLM\n");
    report.push_str("2. **Search**: Multi-provider search with deduplication and quality scoring\n");
    report.push_str("3. **Extraction**: LLM-based fact extraction with verbatim quote capture\n");
    report.push_str("4. **Verification**: Two-stage verification (exact match + LLM entailment)\n");
    report.push_str("5. **Corroboration**: Cross-source claim clustering and conflict detection\n");
    report.push_str("6. **Synthesis**: Structured report generation with inline citations\n\n");
    report.push_str("*Confidence scores reflect source quality, corroboration count, and verification status.*\n");

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::fact::Fact;
    use crate::research::roles::verify::{VerificationLabel, FactVerification};
    use crate::research::store::Source;
    use crate::research::search::CorroboratedFact;

    fn make_fact(sub_q: usize, source_id: i64, claim: &str) -> Fact {
        Fact { sub_q, source_id, claim: claim.into(), quote: claim.into() }
    }

    fn make_source(id: i64, url: &str) -> Source {
        Source { id, url: url.into(), title: "Test".into(), text: "Test content".into(), quality_score: 0.8 }
    }

    #[test]
    fn test_write_section() {
        let fact = make_fact(0, 1, "Test claim");
        let verification = FactVerification { fact_index: 0, stage1_pass: true, stage2_label: Some(VerificationLabel::Supported), stage2_reasoning: None };
        let source = make_source(1, "https://example.com");
        
        let section = write_section("Test", &[&fact], &[fact], &[verification], &[source], &[]);
        
        assert!(section.content.contains("Test claim"));
        assert_eq!(section.citations.len(), 1);
        assert!(section.confidence > 0.0);
    }
}