use crate::research::error::{ResearchError, ResearchResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Messages sent to the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: String, // "system" | "user" | "assistant"
    pub content: String,
}

/// Response from the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

/// Trait for LLM clients. Implementations can be real (llama.cpp, OpenAI, etc.)
/// or mock (for testing).
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// Complete a chat conversation.
    async fn complete(&self, messages: Vec<LlmMessage>, params: LlmParams) -> ResearchResult<LlmResponse>;

    /// Model identifier for logging/cost tracking.
    fn model_id(&self) -> &str;

    /// Estimated cost per 1M input tokens (USD).
    fn input_cost_per_m(&self) -> f64;

    /// Estimated cost per 1M output tokens (USD).
    fn output_cost_per_m(&self) -> f64;
}

/// Sampling parameters for LLM completion.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmParams {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    pub stop: Option<Vec<String>>,
    pub json_schema: Option<serde_json::Value>, // For structured output
}

/// Mock LLM client for deterministic testing.
/// Returns pre-programmed responses based on the last user message content.
pub struct MockLlmClient {
    responses: Arc<Mutex<HashMap<String, LlmResponse>>>,
    default_response: LlmResponse,
    model_id: String,
    call_log: Arc<Mutex<Vec<MockCall>>>,
}

#[derive(Debug, Clone)]
pub struct MockCall {
    pub messages: Vec<LlmMessage>,
    pub params: LlmParams,
    pub response: LlmResponse,
}

impl MockLlmClient {
    pub fn new(model_id: impl Into<String>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(HashMap::new())),
            default_response: LlmResponse {
                content: "{}".to_string(),
                prompt_tokens: 10,
                completion_tokens: 10,
            },
            model_id: model_id.into(),
            call_log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Set a response for a specific user message (matched by substring).
    pub fn with_response(&self, user_content_contains: impl Into<String>, response: LlmResponse) {
        self.responses.lock().unwrap().insert(user_content_contains.into(), response);
    }

    /// Set the default response when no specific match is found.
    pub fn with_default_response(&mut self, response: LlmResponse) {
        self.default_response = response;
    }

    /// Get the call log for assertions.
    pub fn calls(&self) -> Vec<MockCall> {
        self.call_log.lock().unwrap().clone()
    }

    /// Clear the call log.
    pub fn clear_log(&self) {
        self.call_log.lock().unwrap().clear();
    }

    /// Get the last call.
    pub fn last_call(&self) -> Option<MockCall> {
        self.call_log.lock().unwrap().last().cloned()
    }
}

#[async_trait]
impl LlmClient for MockLlmClient {
    async fn complete(&self, messages: Vec<LlmMessage>, params: LlmParams) -> ResearchResult<LlmResponse> {
        // Find the last user message for matching
        let user_content = messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.clone())
            .unwrap_or_default();

        // Try to find a matching response
        let responses = self.responses.lock().unwrap();
        let response = responses
            .iter()
            .find(|(key, _)| user_content.contains(key.as_str()))
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| self.default_response.clone());

        // Log the call
        self.call_log.lock().unwrap().push(MockCall {
            messages: messages.clone(),
            params: params.clone(),
            response: response.clone(),
        });

        Ok(response)
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn input_cost_per_m(&self) -> f64 {
        0.0
    }

    fn output_cost_per_m(&self) -> f64 {
        0.0
    }
}

/// Helper to create a JSON-structured response for mocking.
pub fn mock_json_response<T: Serialize>(value: &T) -> ResearchResult<LlmResponse> {
    let content = serde_json::to_string(value).map_err(|e| ResearchError::Llm(e.to_string()))?;
    let completion_tokens = (content.len() / 4) as u32;
    Ok(LlmResponse {
        content,
        prompt_tokens: 50,
        completion_tokens,
    })
}

/// Helper to parse JSON from an LLM response.
pub fn parse_llm_json<T: for<'de> Deserialize<'de>>(response: &LlmResponse) -> ResearchResult<T> {
    serde_json::from_str(&response.content).map_err(|e| {
        ResearchError::LlmInvalidJson {
            role: "unknown".to_string(),
            raw: format!("JSON parse error: {e}\nContent: {}", response.content),
        }
    })
}