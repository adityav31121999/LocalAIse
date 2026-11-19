pub mod reader;
pub mod verify;

use crate::research::llm::{LlmClient, LlmMessage, LlmParams, parse_llm_json};

use chrono::Utc;

/// Planner for the full deep-research run: the model produces an ordered
/// plan whose steps are each tagged `local` (project documents) or
/// `internet` (web). The pipeline then follows the plan step by step.
///
/// `allow_internet` reflects the project's internet setting and the requested
/// mode; when false the model is told every step must use `local`.
pub async fn plan_research<C: LlmClient>(
    llm: &C,
    question: &str,
    allow_internet: bool,
) -> (crate::research::transcript::PlanRecord, Vec<crate::research::transcript::PlanStep>) {
    let today = Utc::now().format("%B %d, %Y").to_string();

    let source_instruction = if allow_internet {
        "Each step must set \"source\" to either \"local\" (answerable from the project's own documents) or \"internet\" (requires web search). Prefer local steps for project-specific facts and internet steps for external/current information. Order the steps so local groundwork comes first."
    } else {
        "The internet is disabled for this project. Every step MUST set \"source\" to \"local\"."
    };

    let system = format!(
        "You are a deep-research planner. Today's date is {today}. Break the user's question into an ordered plan of 3-7 focused research steps that together comprehensively answer the main question. {source_instruction} When the question involves 'today', 'current', 'latest', or 'recent' data, ensure relevant steps and their search queries explicitly reference the current date. Each step should be specific, answerable, and non-overlapping. Return ONLY valid JSON matching the schema."
    );

    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "steps": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "question": { "type": "string" },
                        "source": { "type": "string", "enum": ["local", "internet"] },
                        "search_queries": {
                            "type": "array",
                            "items": { "type": "string" },
                            "minItems": 1,
                            "maxItems": 3
                        },
                        "answer_criteria": { "type": "string" }
                    },
                    "required": ["question", "source", "search_queries", "answer_criteria"],
                    "additionalProperties": false
                },
                "minItems": 3,
                "maxItems": 7
            }
        },
        "required": ["steps"],
        "additionalProperties": false
    });

    let user = format!("Main question: {question}\n\nToday's date: {today}\n\nReturn the research plan as JSON.");

    let params = LlmParams {
        temperature: Some(0.3),
        json_schema: Some(schema),
        max_tokens: Some(1536),
        ..Default::default()
    };

    let messages = vec![
        LlmMessage { role: "system".into(), content: system.clone() },
        LlmMessage { role: "user".into(), content: user.clone() },
    ];

    #[derive(serde::Deserialize)]
    struct PlanResponse {
        steps: Vec<PlanStepRaw>,
    }
    #[derive(serde::Deserialize)]
    struct PlanStepRaw {
        question: String,
        source: String,
        #[serde(default)]
        search_queries: Vec<String>,
        #[serde(default)]
        answer_criteria: String,
    }

    let (raw_response, steps) = match llm.complete(messages, params).await {
        Ok(resp) => {
            let raw = resp.content.clone();
            let steps = parse_llm_json::<PlanResponse>(&resp)
                .map(|plan| {
                    plan.steps.into_iter().enumerate().map(|(i, s)| {
                        let queries = if s.search_queries.is_empty() { vec![s.question.clone()] } else { s.search_queries };
                        crate::research::transcript::PlanStep {
                        id: i + 1,
                        question: s.question,
                        source: if s.source == "internet" && allow_internet {
                            crate::research::transcript::StepSource::Internet
                        } else {
                            crate::research::transcript::StepSource::Local
                        },
                        search_queries: queries,
                        answer_criteria: s.answer_criteria,
                    } }).collect()
                })
                .unwrap_or_else(|_| vec![fallback_step(question)]);
            (raw, steps)
        }
        Err(_) => (String::new(), vec![fallback_step(question)]),
    };

    let record = crate::research::transcript::PlanRecord {
        system_prompt: system,
        user_prompt: user,
        raw_response,
        steps: steps.clone(),
    };
    (record, steps)
}

fn fallback_step(question: &str) -> crate::research::transcript::PlanStep {
    crate::research::transcript::PlanStep {
        id: 1,
        question: question.to_string(),
        source: crate::research::transcript::StepSource::Local,
        search_queries: vec![question.to_string()],
        answer_criteria: "Any relevant information".into(),
    }
}

/// Sub-question with metadata for the research pipeline.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SubQuestion {
    pub question: String,
    pub search_queries: Vec<String>,
    pub answer_criteria: String,
}
