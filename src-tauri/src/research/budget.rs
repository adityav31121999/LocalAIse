use crate::research::llm::{LlmClient, LlmMessage, LlmParams, parse_llm_json};
use crate::research::error::ResearchError;
use chrono::Utc;
use serde::{Deserialize, Serialize};

/// Budget presets for different research depths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BudgetPreset {
    Quick,    // ~1 min, few sources
    Standard, // ~3 min, balanced
    Deep,     // ~10 min, comprehensive
}

/// Budget configuration with adaptive controls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Budgets {
    pub preset: BudgetPreset,
    pub max_rounds: usize,
    pub max_pages: usize,
    pub max_cost_units: u64,
    pub wall_clock_secs: u64,
    /// Minimum corroborated facts per sub-question before considering it "answered"
    pub min_facts_per_subq: usize,
    /// Marginal gain threshold: stop if new_supported_facts / cost_units < this
    pub marginal_gain_threshold: f32,
    /// Require this many consecutive low-gain rounds before stopping
    pub low_gain_rounds_to_stop: usize,
}

impl Default for Budgets {
    fn default() -> Self {
        Self::standard()
    }
}

impl Budgets {
    pub fn quick() -> Self {
        Self {
            preset: BudgetPreset::Quick,
            max_rounds: 2,
            max_pages: 8,
            max_cost_units: 50,
            wall_clock_secs: 60,
            min_facts_per_subq: 1,
            marginal_gain_threshold: 0.1,
            low_gain_rounds_to_stop: 1,
        }
    }

    pub fn standard() -> Self {
        Self {
            preset: BudgetPreset::Standard,
            max_rounds: 4,
            max_pages: 20,
            max_cost_units: 200,
            wall_clock_secs: 180,
            min_facts_per_subq: 2,
            marginal_gain_threshold: 0.05,
            low_gain_rounds_to_stop: 2,
        }
    }

    pub fn deep() -> Self {
        Self {
            preset: BudgetPreset::Deep,
            max_rounds: 6,
            max_pages: 50,
            max_cost_units: 1000,
            wall_clock_secs: 600,
            min_facts_per_subq: 3,
            marginal_gain_threshold: 0.02,
            low_gain_rounds_to_stop: 3,
        }
    }

    /// Estimate budgets based on number of sub-questions.
    pub fn estimate_for_subquestions(subq_count: usize) -> Self {
        let base = Self::standard();
        // Scale roughly linearly with sub-questions
        let factor = (subq_count as f32 / 3.0).max(0.5).min(3.0);
        Self {
            max_rounds: (base.max_rounds as f32 * factor).ceil() as usize,
            max_pages: (base.max_pages as f32 * factor).ceil() as usize,
            max_cost_units: (base.max_cost_units as f32 * factor).ceil() as u64,
            wall_clock_secs: (base.wall_clock_secs as f32 * factor).ceil() as u64,
            ..base
        }
    }
}

/// Gap analysis result for a sub-question.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GapAnalysis {
    pub subq_index: usize,
    pub status: GapStatus,
    pub missing_aspects: Vec<String>,
    pub followup_queries: Vec<String>,
    pub confidence: f32, // 0.0 - 1.0
}

/// Gap status for a sub-question.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GapStatus {
    Answered,   // Sufficient corroborated facts
    Partial,    // Some facts but gaps remain
    Missing,    // No or very few facts
}

/// LLM-based gap checker that analyzes coverage and generates follow-up queries.
pub async fn check_gaps<C: LlmClient + ?Sized>(
    llm: &C,
    sub_questions: &[crate::research::roles::SubQuestion],
    facts: &[crate::research::fact::Fact],
    corroborated: &[crate::research::search::CorroboratedFact],
    budgets: &Budgets,
) -> Result<Vec<GapAnalysis>, ResearchError> {
    if sub_questions.is_empty() {
        return Ok(vec![]);
    }

    // Build context for the LLM
    let mut context = String::new();
    context.push_str("Sub-questions and their current evidence:\n\n");

    for (i, sq) in sub_questions.iter().enumerate() {
        context.push_str(&format!("## Sub-question {}: {}\n", i + 1, sq.question));
        context.push_str(&format!("Answer criteria: {}\n", sq.answer_criteria));
        
        // Count facts for this sub-question
        let subq_facts: Vec<_> = facts.iter().filter(|f| f.sub_q == i).collect();
        let subq_corroborated: Vec<_> = corroborated.iter().filter(|f| {
            subq_facts.iter().any(|sf| sf.claim == f.claim && sf.source_id == f.source_id)
        }).collect();

        context.push_str(&format!("Facts extracted: {}\n", subq_facts.len()));
        context.push_str(&format!("Corroborated facts: {}\n", subq_corroborated.len()));
        
        if !subq_corroborated.is_empty() {
            context.push_str("Key findings:\n");
            for f in subq_corroborated.iter().take(5) {
                context.push_str(&format!("- {} (source: {}, score: {:.2})\n", 
                    f.claim, f.source_url, f.source_score.composite));
            }
        } else {
            context.push_str("Key findings: None yet\n");
        }
        context.push('\n');
    }

    context.push_str(&format!(
        "Minimum corroborated facts required per sub-question: {}\n",
        budgets.min_facts_per_subq
    ));

    let today = Utc::now().format("%B %d, %Y").to_string();
    let system = format!(
        "You are a research gap analyzer. Today's date is {}. For each sub-question, determine if it is:\n\
        - ANSWERED: Has sufficient corroborated facts meeting the minimum threshold\n\
        - PARTIAL: Has some facts but important aspects are missing\n\
        - MISSING: Has no or very few relevant facts\n\n\
        For time-sensitive topics (stock market, news, weather, etc.), ensure follow-up queries \
        reference the current date ({}). For PARTIAL and MISSING, generate 1-3 specific follow-up \
        search queries that would fill the gaps. Be precise and targeted.\n\n\
        Return ONLY valid JSON matching the schema.",
        today, today
    );

    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "analyses": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "subq_index": { "type": "integer", "minimum": 0 },
                        "status": { "type": "string", "enum": ["answered", "partial", "missing"] },
                        "missing_aspects": { "type": "array", "items": { "type": "string" } },
                        "followup_queries": { "type": "array", "items": { "type": "string" }, "maxItems": 3 },
                        "confidence": { "type": "number", "minimum": 0.0, "maximum": 1.0 }
                    },
                    "required": ["subq_index", "status", "missing_aspects", "followup_queries", "confidence"],
                    "additionalProperties": false
                },
                "minItems": 1
            }
        },
        "required": ["analyses"],
        "additionalProperties": false
    });

    let user = format!("{}\n\nAnalyze gaps and return JSON.", context);

    let params = LlmParams {
        temperature: Some(0.2),
        json_schema: Some(schema),
        max_tokens: Some(2048),
        ..Default::default()
    };

    let messages = vec![
        LlmMessage { role: "system".into(), content: system.into() },
        LlmMessage { role: "user".into(), content: user },
    ];

    #[derive(Deserialize)]
    struct GapResponse {
        analyses: Vec<GapAnalysisRaw>,
    }

    #[derive(Deserialize)]
    struct GapAnalysisRaw {
        subq_index: usize,
        status: String,
        missing_aspects: Vec<String>,
        followup_queries: Vec<String>,
        confidence: f32,
    }

    let resp = llm.complete(messages, params).await?;
    let parsed = parse_llm_json::<GapResponse>(&resp)?;

    let mut analyses = Vec::new();
    for raw in parsed.analyses {
        let status = match raw.status.to_lowercase().as_str() {
            "answered" => GapStatus::Answered,
            "partial" => GapStatus::Partial,
            "missing" => GapStatus::Missing,
            _ => GapStatus::Missing,
        };
        analyses.push(GapAnalysis {
            subq_index: raw.subq_index,
            status,
            missing_aspects: raw.missing_aspects,
            followup_queries: raw.followup_queries,
            confidence: raw.confidence.clamp(0.0, 1.0),
        });
    }

    // Ensure we have analysis for all sub-questions
    for i in 0..sub_questions.len() {
        if !analyses.iter().any(|a| a.subq_index == i) {
            analyses.push(GapAnalysis {
                subq_index: i,
                status: GapStatus::Missing,
                missing_aspects: vec!["No analysis returned".into()],
                followup_queries: vec![sub_questions[i].question.clone()],
                confidence: 0.5,
            });
        }
    }

    // Sort by subq_index
    analyses.sort_by_key(|a| a.subq_index);
    Ok(analyses)
}

/// Check if we should stop based on marginal gain.
pub fn should_stop_adaptive(
    _round: usize,
    new_supported_facts: usize,
    cost_units_this_round: u64,
    budgets: &Budgets,
    low_gain_streak: &mut usize,
) -> bool {
    if cost_units_this_round == 0 {
        *low_gain_streak += 1;
        return *low_gain_streak >= budgets.low_gain_rounds_to_stop;
    }

    let gain_per_cost = new_supported_facts as f32 / cost_units_this_round as f32;
    
    if gain_per_cost < budgets.marginal_gain_threshold {
        *low_gain_streak += 1;
    } else {
        *low_gain_streak = 0;
    }

    *low_gain_streak >= budgets.low_gain_rounds_to_stop
}

/// Check if all sub-questions are sufficiently answered.
pub fn all_subquestions_answered(
    gap_analyses: &[GapAnalysis],
    _budgets: &Budgets,
) -> bool {
    gap_analyses.iter().all(|a| a.status == GapStatus::Answered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_budget_presets() {
        let quick = Budgets::quick();
        assert_eq!(quick.max_rounds, 2);
        assert_eq!(quick.wall_clock_secs, 60);

        let standard = Budgets::standard();
        assert_eq!(standard.max_rounds, 4);
        assert_eq!(standard.wall_clock_secs, 180);

        let deep = Budgets::deep();
        assert_eq!(deep.max_rounds, 6);
        assert_eq!(deep.wall_clock_secs, 600);
    }

    #[test]
    fn test_budget_estimation() {
        let est = Budgets::estimate_for_subquestions(5);
        assert!(est.max_rounds > Budgets::standard().max_rounds);
        assert!(est.max_pages > Budgets::standard().max_pages);
    }

    #[test]
    fn test_should_stop_adaptive() {
        let budgets = Budgets::standard();
        let mut streak = 0;
        
        // High gain - should not stop
        assert!(!should_stop_adaptive(1, 10, 100, &budgets, &mut streak));
        assert_eq!(streak, 0);
        
        // Low gain - should increment streak
        assert!(!should_stop_adaptive(2, 1, 100, &budgets, &mut streak));
        assert_eq!(streak, 1);
        
        // Another low gain - should stop after 2 (default low_gain_rounds_to_stop)
        assert!(should_stop_adaptive(3, 1, 100, &budgets, &mut streak));
        assert_eq!(streak, 2);
    }
}