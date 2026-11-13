pub mod reader;
pub mod gap;
pub mod writer;
pub mod verify;

use crate::research::llm::{LlmClient, LlmMessage, LlmParams, parse_llm_json};

use chrono::Utc;

/// LLM-based sub-question planner.
/// Falls back to single-question stub if LLM fails or returns invalid JSON.
pub async fn plan_subquestions<C: LlmClient>(llm: &C, question: &str) -> Vec<String> {
    let today = Utc::now().format("%B %d, %Y").to_string();
    
    let system = format!(
        "You are a research planner. Today's date is {}. Break the user's question into 3-7 focused \
        sub-questions that together comprehensively answer the main question. When the question involves \
        'today', 'current', 'latest', or 'recent' data, ensure ALL sub-questions and search queries \
        explicitly reference the current date or recent time period. Each sub-question should be specific, \
        answerable, and non-overlapping. Return ONLY valid JSON matching the schema.",
        today
    );

    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "sub_questions": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "question": { "type": "string" },
                        "search_queries": {
                            "type": "array",
                            "items": { "type": "string" },
                            "minItems": 1,
                            "maxItems": 3
                        },
                        "answer_criteria": { "type": "string" }
                    },
                    "required": ["question", "search_queries", "answer_criteria"],
                    "additionalProperties": false
                },
                "minItems": 3,
                "maxItems": 7
            }
        },
        "required": ["sub_questions"],
        "additionalProperties": false
    });

    let user = format!("Main question: {question}\n\nToday's date: {}\n\nReturn sub-questions as JSON.", today);

    let params = LlmParams {
        temperature: Some(0.3),
        json_schema: Some(schema),
        max_tokens: Some(1024),
        ..Default::default()
    };

    let messages = vec![
        LlmMessage { role: "system".into(), content: system.into() },
        LlmMessage { role: "user".into(), content: user },
    ];

    #[derive(serde::Deserialize)]
    struct PlanResponse {
        sub_questions: Vec<SubQuestion>,
    }
    #[derive(serde::Deserialize)]
    struct SubQuestion {
        question: String,
        #[allow(dead_code)]
        search_queries: Vec<String>,
        #[allow(dead_code)]
        answer_criteria: String,
    }

    match llm.complete(messages, params).await {
        Ok(resp) => {
            match parse_llm_json::<PlanResponse>(&resp) {
                Ok(plan) => plan.sub_questions.into_iter().map(|sq| sq.question).collect(),
                Err(_) => vec![question.to_string()], // fallback
            }
        }
        Err(_) => vec![question.to_string()], // fallback
    }
}

/// Sub-question with metadata for the research pipeline.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SubQuestion {
    pub question: String,
    pub search_queries: Vec<String>,
    pub answer_criteria: String,
}

impl SubQuestion {
    pub fn simple(q: String) -> Self {
        Self {
            search_queries: vec![q.clone()],
            answer_criteria: "Any relevant information".into(),
            question: q,
        }
    }
}