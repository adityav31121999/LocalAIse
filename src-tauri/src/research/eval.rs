use crate::research::error::ResearchError;
use serde::{Deserialize, Serialize};

/// A single expected fact in the evaluation set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedFact {
    /// The claim that should appear in the report.
    pub claim: String,
    /// Alternative phrasings/aliases that also count as a match.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// If true, this fact must appear for the run to pass.
    #[serde(default = "default_true")]
    pub required: bool,
    /// Expected source domain (optional). If provided, the fact must be cited from this domain.
    #[serde(default)]
    pub expected_domain: Option<String>,
}

fn default_true() -> bool {
    true
}

/// An evaluation question with expected outcomes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalQuestion {
    /// Unique identifier for this question.
    pub id: String,
    /// The research question.
    pub question: String,
    /// Expected facts that should be found and cited.
    pub expected_facts: Vec<ExpectedFact>,
    /// Domains that are considered authoritative for this question.
    #[serde(default)]
    pub acceptable_domains: Vec<String>,
    /// Name of the fixture set to use for this question (for offline/replay testing).
    #[serde(default)]
    pub fixture_set: Option<String>,
    /// Maximum acceptable cost units for this question.
    #[serde(default)]
    pub max_cost_units: Option<u64>,
    /// Maximum acceptable wall-clock seconds.
    #[serde(default)]
    pub max_wall_clock_secs: Option<u64>,
}

/// Evaluation result for a single question.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResult {
    pub question_id: String,
    /// Whether all required facts were found.
    pub required_facts_covered: bool,
    /// Fraction of expected facts found (0.0 to 1.0).
    pub fact_coverage: f64,
    /// Fraction of citations that are actually supported by their sources (0.0 to 1.0).
    pub citation_precision: f64,
    /// Number of facts found vs expected.
    pub facts_found: usize,
    pub facts_expected: usize,
    /// Number of required facts missing.
    pub required_facts_missing: usize,
    /// Total cost units used.
    pub cost_units: u64,
    /// Wall-clock time in seconds.
    pub wall_clock_secs: u64,
    /// Per-fact detail.
    pub fact_details: Vec<FactDetail>,
}

/// Detail for a single expected fact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactDetail {
    pub claim: String,
    pub found: bool,
    pub required: bool,
    pub matched_alias: Option<String>,
    pub cited_source_url: Option<String>,
    pub source_domain: Option<String>,
    pub verification_stage1_pass: bool,
    pub verification_stage2_label: Option<String>,
}

/// Aggregate evaluation results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalReport {
    pub results: Vec<EvalResult>,
    pub summary: EvalSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalSummary {
    pub total_questions: usize,
    pub questions_passed: usize,
    pub mean_fact_coverage: f64,
    pub mean_citation_precision: f64,
    pub total_cost_units: u64,
    pub total_wall_clock_secs: u64,
}

/// Run the evaluation suite against a research function.
pub async fn run_eval<F, Fut>(
    questions: Vec<EvalQuestion>,
    mut run_research: F,
) -> Result<EvalReport, ResearchError>
where
    F: FnMut(EvalQuestion) -> Fut,
    Fut: std::future::Future<Output = Result<EvalRunOutput, ResearchError>>,
{
    let mut results = Vec::new();

    for question in questions {
        let start = std::time::Instant::now();
        let output = run_research(question.clone()).await?;
        let wall_clock_secs = start.elapsed().as_secs();

        let result = evaluate_question(&question, &output, wall_clock_secs);
        results.push(result);
    }

    let summary = compute_summary(&results);
    Ok(EvalReport { results, summary })
}

/// Output from a research run for evaluation.
#[derive(Debug, Clone)]
pub struct EvalRunOutput {
    /// The generated report markdown.
    pub report_markdown: String,
    /// Sources used with their URLs.
    pub sources: Vec<EvalSource>,
    /// Facts extracted with verification info.
    pub facts: Vec<EvalFact>,
    /// Cost units used.
    pub cost_units: u64,
}

#[derive(Debug, Clone)]
pub struct EvalSource {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct EvalFact {
    pub claim: String,
    pub quote: String,
    pub source_id: i64,
    pub sub_q: usize,
    pub verification_stage1_pass: bool,
    pub verification_stage2_label: Option<String>, // "supported" | "contradicted" | "unsupported"
}

/// Evaluate a single question's output.
fn evaluate_question(question: &EvalQuestion, output: &EvalRunOutput, wall_clock_secs: u64) -> EvalResult {
    let mut fact_details = Vec::new();
    let mut facts_found = 0;
    let mut required_facts_missing = 0;

    for expected in &question.expected_facts {
        let mut found = false;
        let mut matched_alias = None;
        let mut cited_source_url = None;
        let mut source_domain = None;
        let mut verification_stage1_pass = false;
        let mut verification_stage2_label = None;

        for fact in &output.facts {
            // Check if this fact matches the expected claim
            let claim_match = facts_match(&expected.claim, &fact.claim);
            let alias_match = expected.aliases.iter().any(|a| facts_match(a, &fact.claim));

            if claim_match || alias_match {
                found = true;
                if alias_match {
                    matched_alias = expected.aliases.iter().find(|a| facts_match(a, &fact.claim)).cloned();
                }

                // Get source info
                if let Some(source) = output.sources.iter().find(|s| s.id == fact.source_id) {
                    cited_source_url = Some(source.url.clone());
                    source_domain = extract_domain(&source.url);
                }

                verification_stage1_pass = fact.verification_stage1_pass;
                verification_stage2_label = fact.verification_stage2_label.clone();
                break;
            }
        }

        if found {
            facts_found += 1;
        } else if expected.required {
            required_facts_missing += 1;
        }

        fact_details.push(FactDetail {
            claim: expected.claim.clone(),
            found,
            required: expected.required,
            matched_alias,
            cited_source_url,
            source_domain,
            verification_stage1_pass,
            verification_stage2_label,
        });
    }

    let fact_coverage = if question.expected_facts.is_empty() {
        1.0
    } else {
        facts_found as f64 / question.expected_facts.len() as f64
    };

    // Citation precision: fraction of facts that passed both verification stages
    let verified_facts = output.facts.iter()
        .filter(|f| f.verification_stage1_pass && f.verification_stage2_label.as_deref() == Some("supported"))
        .count();
    let citation_precision = if output.facts.is_empty() {
        1.0
    } else {
        verified_facts as f64 / output.facts.len() as f64
    };

    let required_facts_covered = required_facts_missing == 0;

    EvalResult {
        question_id: question.id.clone(),
        required_facts_covered,
        fact_coverage,
        citation_precision,
        facts_found,
        facts_expected: question.expected_facts.len(),
        required_facts_missing,
        cost_units: output.cost_units,
        wall_clock_secs,
        fact_details,
    }
}

/// Simple fuzzy match for fact claims.
fn facts_match(expected: &str, actual: &str) -> bool {
    let expected_norm = normalize_for_matching(expected);
    let actual_norm = normalize_for_matching(actual);

    // Check containment both ways for fuzzy matching
    actual_norm.contains(&expected_norm) || expected_norm.contains(&actual_norm)
}

fn normalize_for_matching(s: &str) -> String {
    s.to_lowercase()
        .split_whitespace()
        .filter(|w| w.len() > 2) // Skip very short words
        .collect::<Vec<_>>()
        .join(" ")
}

fn extract_domain(url: &str) -> Option<String> {
    reqwest::Url::parse(url).ok()?.host_str().map(|s| s.to_string())
}

fn compute_summary(results: &[EvalResult]) -> EvalSummary {
    let total_questions = results.len();
    let questions_passed = results.iter().filter(|r| r.required_facts_covered).count();
    let mean_fact_coverage = if total_questions > 0 {
        results.iter().map(|r| r.fact_coverage).sum::<f64>() / total_questions as f64
    } else {
        0.0
    };
    let mean_citation_precision = if total_questions > 0 {
        results.iter().map(|r| r.citation_precision).sum::<f64>() / total_questions as f64
    } else {
        0.0
    };
    let total_cost_units = results.iter().map(|r| r.cost_units).sum();
    let total_wall_clock_secs = results.iter().map(|r| r.wall_clock_secs).sum();

    EvalSummary {
        total_questions,
        questions_passed,
        mean_fact_coverage,
        mean_citation_precision,
        total_cost_units,
        total_wall_clock_secs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_facts_match() {
        assert!(facts_match("The sky is blue", "The sky appears blue"));
        assert!(facts_match("GDP grew 2%", "GDP growth was 2 percent"));
        assert!(!facts_match("The sky is blue", "The grass is green"));
    }

    #[test]
    fn test_normalize_for_matching() {
        assert_eq!(normalize_for_matching("The Sky is Blue!"), "sky blue");
        assert_eq!(normalize_for_matching("GDP grew 2%"), "gdp grew 2%");
    }
}