use crate::research::llm::{LlmClient, LlmMessage, LlmParams, LlmResponse};
use crate::research::error::{ResearchError, ResearchResult};
use crate::runtime::client::{self, SamplingParams as RuntimeSamplingParams};
use crate::runtime::Serving;
use async_trait::async_trait;

/// Adapter that implements `LlmClient` using the existing runtime client.
pub struct RuntimeLlmClient {
    serving: Serving,
    model_label: String,
}

impl RuntimeLlmClient {
    pub fn new(serving: Serving, model_label: String) -> Self {
        Self { serving, model_label }
    }
}

#[async_trait]
impl LlmClient for RuntimeLlmClient {
    async fn complete(&self, messages: Vec<LlmMessage>, params: LlmParams) -> ResearchResult<LlmResponse> {
        let runtime_messages: Vec<client::ChatMessage> = messages
            .into_iter()
            .map(|m| {
                let role = match m.role.as_str() {
                    "system" => "system",
                    "user" => "user",
                    "assistant" => "assistant",
                    _ => "user",
                };
                client::message(role, m.content)
            })
            .collect();

        let runtime_params = RuntimeSamplingParams {
            temperature: params.temperature.unwrap_or(0.2),
            top_p: params.top_p,
            top_k: None,
            max_tokens: params.max_tokens,
            reasoning_effort: None,
            chat_template_kwargs: params.json_schema.map(|s| serde_json::json!({
                "grammar": format!("json\n{}", serde_json::to_string(&s).unwrap_or_default())
            })),
        };

        // Use complete_with_usage to get token counts
        let (content, usage) = client::complete_with_usage(&self.serving, &self.model_label, &runtime_messages, runtime_params).await
            .map_err(|e| ResearchError::Llm(e.to_string()))?;

        Ok(LlmResponse {
            content,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
        })
    }

    fn model_id(&self) -> &str {
        &self.model_label
    }

    fn input_cost_per_m(&self) -> f64 {
        0.0 // Local model, no API cost
    }

    fn output_cost_per_m(&self) -> f64 {
        0.0
    }
}

/// Convert our LlmParams to runtime SamplingParams (for non-structured output).
pub fn to_runtime_params(params: LlmParams) -> RuntimeSamplingParams {
    RuntimeSamplingParams {
        temperature: params.temperature.unwrap_or(0.2),
        top_p: params.top_p,
        top_k: None,
        max_tokens: params.max_tokens,
        reasoning_effort: None,
        chat_template_kwargs: None,
    }
}