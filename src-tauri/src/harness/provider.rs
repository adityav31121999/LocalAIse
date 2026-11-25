//! Provider capability & adapter boundary (T3 Code pattern).
//!
//! Decouples the agent harness loop from `llama-server` so local inference
//! and future external OpenAI-compatible or CLI/ACP providers share a uniform
//! capability contract.

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;

use crate::error::AppResult;
use crate::runtime::client::{self, SamplingParams, StreamedToolCompletion};
use crate::runtime::Serving;

/// Declares what a provider adapter supports so the harness and UI never
/// branch on provider names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCapabilities {
    pub streaming: bool,
    pub native_tools: bool,
    pub reasoning_trace: bool,
    pub approvals: bool,
    pub subagents: bool,
    pub checkpoints: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDescriptor {
    pub id: String,
    pub name: String,
    pub capabilities: ProviderCapabilities,
}

/// Uniform interface for model-turn execution across providers.
#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    fn id(&self) -> &'static str;

    fn capabilities(&self) -> ProviderCapabilities;

    async fn complete_with_tools(
        &self,
        messages: &[Value],
        tools: &Value,
        sampling: SamplingParams,
        should_stop: &(dyn Fn() -> bool + Send + Sync),
        on_delta: &mut (dyn for<'b> FnMut(&'b str) + Send),
        on_reasoning: &mut (dyn for<'b> FnMut(&'b str) + Send),
    ) -> AppResult<StreamedToolCompletion>;
}

/// Default on-device provider backed by LocalAIse's supervised `llama-server`.
pub struct LlamaServerAdapter<'a> {
    serving: &'a Serving,
    model_label: &'a str,
}

impl<'a> LlamaServerAdapter<'a> {
    pub fn new(serving: &'a Serving, model_label: &'a str) -> Self {
        Self {
            serving,
            model_label,
        }
    }

    pub fn descriptor() -> ProviderDescriptor {
        ProviderDescriptor {
            id: "llama-server".to_string(),
            name: "Local llama-server (On-Device)".to_string(),
            capabilities: ProviderCapabilities {
                streaming: true,
                native_tools: true,
                reasoning_trace: true,
                approvals: true,
                subagents: true,
                checkpoints: true,
            },
        }
    }
}

#[async_trait]
impl<'a> ProviderAdapter for LlamaServerAdapter<'a> {
    fn id(&self) -> &'static str {
        "llama-server"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        Self::descriptor().capabilities
    }

    async fn complete_with_tools(
        &self,
        messages: &[Value],
        tools: &Value,
        sampling: SamplingParams,
        should_stop: &(dyn Fn() -> bool + Send + Sync),
        on_delta: &mut (dyn for<'b> FnMut(&'b str) + Send),
        on_reasoning: &mut (dyn for<'b> FnMut(&'b str) + Send),
    ) -> AppResult<StreamedToolCompletion> {
        client::complete_stream_with_tools(
            self.serving,
            self.model_label,
            messages,
            tools,
            sampling,
            || should_stop(),
            |delta| on_delta(delta),
            |delta| on_reasoning(delta),
        )
        .await
    }
}

/// Lists configured provider descriptors for the UI / diagnostics.
pub fn list_providers() -> Vec<ProviderDescriptor> {
    vec![LlamaServerAdapter::descriptor()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llama_server_descriptor_exposes_full_local_capabilities() {
        let providers = list_providers();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].id, "llama-server");
        assert!(providers[0].capabilities.streaming);
        assert!(providers[0].capabilities.native_tools);
        assert!(providers[0].capabilities.reasoning_trace);
        assert!(providers[0].capabilities.approvals);
        assert!(providers[0].capabilities.subagents);
        assert!(providers[0].capabilities.checkpoints);
    }
}
