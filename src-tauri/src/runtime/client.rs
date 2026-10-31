//! Talks to the llama-server instance the supervisor started.
//!
//! The server listens on a random localhost port with a per-session API key, so
//! every request here stays on this machine. There is no fallback: when the
//! server is not answering, the caller gets an error instead of a canned reply.

use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::runtime::Serving;

/// A cold model can take a while to load before it serves its first request.
const READY_TIMEOUT: Duration = Duration::from_secs(180);
/// Generous, because a small machine generating a long answer is still working.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(900);
const POLL_INTERVAL: Duration = Duration::from_millis(400);

/// One turn of the conversation, in the OpenAI chat shape llama-server speaks.
#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub role: &'static str,
    pub content: String,
}

pub fn message(role: &'static str, content: impl Into<String>) -> ChatMessage {
    ChatMessage {
        role,
        content: content.into(),
    }
}

/// Sampling parameters for LLM inference.
#[derive(Debug, Clone, Serialize)]
pub struct SamplingParams {
    pub temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i32>,
    /// Maximum number of tokens to generate. `None` lets the server decide.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// OAI reasoning effort: "none" disables reasoning for models with thinking enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    /// Chat template kwargs passed to llama-server (e.g. {"enable_thinking": false}).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_template_kwargs: Option<serde_json::Value>,
}

impl Default for SamplingParams {
    fn default() -> Self {
        Self {
            temperature: 0.2,
            top_p: None,
            top_k: None,
            max_tokens: None,
            reasoning_effort: None,
            chat_template_kwargs: None,
        }
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<i32>,
    /// `false` starts a chat with no KV-cache reuse at all; `None` lets
    /// llama-server reuse the longest matching prompt prefix.
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_prompt: Option<bool>,
    /// Ask llama-server to close the stream with the turn's token counts.
    stream_options: StreamOptions,
    /// Maximum tokens the model may generate in this turn.
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chat_template_kwargs: Option<&'a serde_json::Value>,
}

#[derive(Serialize, Clone, Copy)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Choice {
    message: ReplyMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ReplyMessage {
    content: Option<String>,
}

fn client(timeout: Duration) -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(timeout)
        // Inference is local; a proxy would only get in the way.
        .no_proxy()
        .build()
        .map_err(|error| AppError::msg(format!("could not prepare the local request: {error}")))
}

fn endpoint(serving: &Serving, path: &str) -> String {
    format!("http://127.0.0.1:{}{path}", serving.port)
}

/// Waits until llama-server reports itself healthy, or gives up.
pub async fn wait_until_ready(serving: &Serving) -> AppResult<()> {
    let http = client(Duration::from_secs(5))?;
    let url = endpoint(serving, "/health");
    let deadline = tokio::time::Instant::now() + READY_TIMEOUT;

    loop {
        if let Ok(response) = http.get(&url).bearer_auth(&serving.token).send().await {
            if response.status().is_success() {
                return Ok(());
            }
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(AppError::ServerNotReady);
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Tokenizes `text` using the model's own tokenizer (embedded in the GGUF and
/// exposed by llama-server's `/tokenize` endpoint). Returns the token IDs so
/// callers can size chunks by real token counts instead of character estimates.
///
/// Large documents can produce tens of thousands of tokens; 60 seconds gives
/// slow machines enough headroom without blocking the response-timeout path.
pub async fn tokenize(serving: &Serving, text: &str) -> AppResult<Vec<u32>> {
    let http = client(Duration::from_secs(60))?;
    let response = http
        .post(endpoint(serving, "/tokenize"))
        .bearer_auth(&serving.token)
        .json(&serde_json::json!({ "content": text }))
        .send()
        .await
        .map_err(|error| AppError::msg(format!("llama-server did not answer /tokenize: {error}")))?;

    let status = response.status();
    if !status.is_success() {
        let detail = response.text().await.unwrap_or_default();
        let detail = detail.trim();
        return Err(AppError::ServerRejected(if detail.is_empty() {
            status.to_string()
        } else {
            detail.to_string()
        }));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|error| AppError::msg(format!("could not read /tokenize answer: {error}")))?;

    body.get("tokens")
        .and_then(|v| v.as_array())
        .ok_or_else(|| AppError::msg("/tokenize response missing token array"))?
        .iter()
        .map(|v| {
            v.as_u64()
                .ok_or_else(|| AppError::msg("/tokenize returned a non-integer token"))
                .map(|n| n as u32)
        })
        .collect::<AppResult<Vec<u32>>>()
}

/// Asks for one complete answer and returns its text.
pub async fn complete(
    serving: &Serving,
    model: &str,
    messages: &[ChatMessage],
    sampling: SamplingParams,
) -> AppResult<String> {
    let http = client(RESPONSE_TIMEOUT)?;

    let request = ChatRequest {
        model,
        messages,
        stream: false,
        temperature: sampling.temperature,
        top_p: sampling.top_p,
        top_k: sampling.top_k,
        cache_prompt: None,
        stream_options: StreamOptions { include_usage: false },
        max_tokens: sampling.max_tokens,
        reasoning_effort: sampling.reasoning_effort.as_deref(),
        chat_template_kwargs: sampling.chat_template_kwargs.as_ref(),
    };

    let response = http
        .post(endpoint(serving, "/v1/chat/completions"))
        .bearer_auth(&serving.token)
        .json(&request)
        .send()
        .await
        .map_err(|error| AppError::msg(format!("llama-server did not answer: {error}")))?;

    let status = response.status();
    if !status.is_success() {
        let detail = response.text().await.unwrap_or_default();
        let detail = detail.trim();
        return Err(AppError::ServerRejected(if detail.is_empty() {
            status.to_string()
        } else {
            detail.to_string()
        }));
    }

    let parsed: ChatResponse = response
        .json()
        .await
        .map_err(|error| AppError::ServerRejected(format!("could not read the answer: {error}")))?;

    parsed
        .choices
        .into_iter()
        .next()
        .and_then(|choice| choice.message.content)
        .filter(|content| !content.trim().is_empty())
        .ok_or_else(|| AppError::ServerRejected("the model returned an empty answer".to_string()))
}

/// Like [`complete`], but also returns token usage from the response.
pub async fn complete_with_usage(
    serving: &Serving,
    model: &str,
    messages: &[ChatMessage],
    sampling: SamplingParams,
) -> AppResult<(String, Usage)> {
    let http = client(RESPONSE_TIMEOUT)?;

    let request = ChatRequest {
        model,
        messages,
        stream: false,
        temperature: sampling.temperature,
        top_p: sampling.top_p,
        top_k: sampling.top_k,
        cache_prompt: None,
        stream_options: StreamOptions { include_usage: true },
        max_tokens: sampling.max_tokens,
        reasoning_effort: sampling.reasoning_effort.as_deref(),
        chat_template_kwargs: sampling.chat_template_kwargs.as_ref(),
    };

    let response = http
        .post(endpoint(serving, "/v1/chat/completions"))
        .bearer_auth(&serving.token)
        .json(&request)
        .send()
        .await
        .map_err(|error| AppError::msg(format!("llama-server did not answer: {error}")))?;

    let status = response.status();
    if !status.is_success() {
        let detail = response.text().await.unwrap_or_default();
        let detail = detail.trim();
        return Err(AppError::ServerRejected(if detail.is_empty() {
            status.to_string()
        } else {
            detail.to_string()
        }));
    }

    let parsed: ChatResponse = response
        .json()
        .await
        .map_err(|error| AppError::ServerRejected(format!("could not read the answer: {error}")))?;

    let choice = parsed
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| AppError::ServerRejected("the model returned no choices".to_string()))?;

    if let Some(reason) = &choice.finish_reason {
        if reason == "length" {
            log::warn!("[client] completion stopped due to token limit (finish_reason=length)");
        }
    }

    let content = choice
        .message
        .content
        .filter(|content| !content.trim().is_empty())
        .ok_or_else(|| AppError::ServerRejected("the model returned an empty answer".to_string()))?;

    let usage = parsed.usage.unwrap_or_default();
    Ok((content, usage))
}

// Streaming response shapes (OpenAI "delta" chunks).
#[derive(Deserialize)]
struct StreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
    /// The final chunk reports the turn's token counts when requested.
    #[serde(default)]
    usage: Option<Usage>,
    /// llama-server adds its own timing block next to the usage.
    #[serde(default)]
    timings: Option<Timings>,
}

/// Token counts reported by llama-server for one model call.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
pub struct Usage {
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,
    #[serde(default)]
    pub total_tokens: u32,
}

impl Usage {
    /// Adds another call's counts, for a multi-call turn such as the agent's.
    pub fn add(&mut self, other: Usage) {
        self.prompt_tokens += other.prompt_tokens;
        self.completion_tokens += other.completion_tokens;
        self.total_tokens += other.total_tokens;
    }
}

/// llama-server's timing block: what one call actually spent.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
pub struct Timings {
    #[serde(default)]
    pub prompt_n: u32,
    #[serde(default)]
    pub prompt_ms: f64,
    #[serde(default)]
    pub predicted_n: u32,
    #[serde(default)]
    pub predicted_ms: f64,
    #[serde(default)]
    pub prompt_per_second: f64,
    #[serde(default)]
    pub predicted_per_second: f64,
}

impl Timings {
    /// Adds another call's timings so a turn's speed means all of its tokens.
    pub fn add(&mut self, other: Timings) {
        self.prompt_n += other.prompt_n;
        self.prompt_ms += other.prompt_ms;
        self.predicted_n += other.predicted_n;
        self.predicted_ms += other.predicted_ms;
        if other.predicted_ms > 0.0 {
            self.predicted_per_second =
                self.predicted_n as f64 / (self.predicted_ms / 1000.0);
        }
        if other.prompt_ms > 0.0 {
            self.prompt_per_second = self.prompt_n as f64 / (self.prompt_ms / 1000.0);
        }
    }
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
    /// llama-server exposes a model's reasoning trace here when thinking is on.
    reasoning_content: Option<String>,
    /// Some server builds spell the same field `reasoning`.
    reasoning: Option<String>,
    /// Tool calls the model asks for, streamed argument fragment by fragment.
    #[serde(default)]
    tool_calls: Vec<ToolCallDelta>,
}

/// One streamed tool-call delta. The id and name arrive once, the arguments
/// arrive in pieces and are joined by index.
#[derive(Deserialize, Default)]
struct ToolCallDelta {
    index: Option<u32>,
    id: Option<String>,
    function: Option<FunctionDelta>,
}

#[derive(Deserialize, Default)]
struct FunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

/// One tool call the model asked for, assembled from its deltas.
#[derive(Debug, Clone)]
pub struct ToolCallRequest {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// What a streamed tool-using turn produced: the visible answer, its thinking,
/// and the tool calls to run before the next turn.
pub struct StreamedToolCompletion {
    pub content: String,
    pub reasoning: String,
    pub tool_calls: Vec<ToolCallRequest>,
    /// Token counts for this model call, when the server reported them.
    pub usage: Option<Usage>,
    /// Generation speed for this model call, when the server reported it.
    pub timings: Option<Timings>,
}

/// What a streamed turn produced: the answer and, when available, its thinking.
pub struct StreamedCompletion {
    pub content: String,
    pub reasoning: String,
    /// Token counts for the turn, when the server reported them.
    pub usage: Option<Usage>,
    /// Generation speed for the turn, when the server reported it.
    pub timings: Option<Timings>,
}

/// Asks for a streamed answer and reports each delta to `on_delta` as it
/// arrives, and any reasoning deltas to `on_reasoning`. Returns the full text
/// once the stream ends.
///
/// Interprets the OpenAI-compatible `data:` lines llama-server emits. Empty
/// deltas (role announcements, keep-alives) are skipped, but they do keep the
/// connection warm while a cold CPU pipeline works.
///
/// `reuse_cache` is `false` for the first turn of a new chat, which tells
/// llama-server to build the answer from a completely fresh cache.
///
/// `allow_empty` controls whether an empty answer is treated as an error (default false).
/// When true, returns empty content instead of rejecting it.
#[allow(clippy::too_many_arguments)]
pub async fn complete_stream(
    serving: &Serving,
    model: &str,
    messages: &[ChatMessage],
    reuse_cache: bool,
    sampling: SamplingParams,
    allow_empty: bool,
    mut on_delta: impl FnMut(&str),
    mut on_reasoning: impl FnMut(&str),
) -> AppResult<StreamedCompletion> {
    let http = client(RESPONSE_TIMEOUT)?;

    let request = ChatRequest {
        model,
        messages,
        stream: true,
        temperature: sampling.temperature,
        top_p: sampling.top_p,
        top_k: sampling.top_k,
        cache_prompt: if reuse_cache { None } else { Some(false) },
        stream_options: StreamOptions { include_usage: true },
        max_tokens: sampling.max_tokens,
        reasoning_effort: sampling.reasoning_effort.as_deref(),
        chat_template_kwargs: sampling.chat_template_kwargs.as_ref(),
    };

    let response = http
        .post(endpoint(serving, "/v1/chat/completions"))
        .bearer_auth(&serving.token)
        .json(&request)
        .send()
        .await
        .map_err(|error| AppError::msg(format!("llama-server did not answer: {error}")))?;

    let status = response.status();
    if !status.is_success() {
        let detail = response.text().await.unwrap_or_default();
        let detail = detail.trim();
        return Err(AppError::ServerRejected(if detail.is_empty() {
            status.to_string()
        } else {
            detail.to_string()
        }));
    }

    let mut full = String::new();
    let mut reasoning = String::new();
    let mut usage: Option<Usage> = None;
    let mut timings: Option<Timings> = None;
    let mut buffer = String::new();
    let mut stream = response.bytes_stream();
    let mut done = false;
    while !done {
        let chunk = match stream.next().await {
            Some(chunk) => chunk,
            // Connection ended without a [DONE] marker; keep what we have.
            None => break,
        };
        let chunk = chunk
            .map_err(|error| AppError::msg(format!("llama-server stopped mid-answer: {error}")))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        // SSE events are \n-separated; process every complete line we have.
        while let Some(end) = buffer.find('\n') {
            let drained = buffer.drain(..=end).collect::<String>();
            let line = drained.trim();
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data == "[DONE]" {
                done = true;
                break;
            }
            if let Ok(parsed) = serde_json::from_str::<StreamChunk>(data) {
                if parsed.usage.is_some() {
                    usage = parsed.usage;
                }
                if parsed.timings.is_some() {
                    timings = parsed.timings;
                }
                if let Some(delta) = parsed.choices.into_iter().next().map(|choice| choice.delta)
                {
                    let thinking = delta
                        .reasoning_content
                        .or(delta.reasoning)
                        .filter(|text| !text.is_empty());
                    if let Some(text) = thinking {
                        on_reasoning(&text);
                        reasoning.push_str(&text);
                    }
                    if let Some(text) = delta.content.filter(|text| !text.is_empty()) {
                        on_delta(&text);
                        full.push_str(&text);
                    }
                }
            }
        }
    }

    drop(stream);
    if full.trim().is_empty() {
        if allow_empty {
            Ok(StreamedCompletion {
                content: String::new(),
                reasoning,
                usage,
                timings,
            })
        } else {
            Err(AppError::ServerRejected(
                "the model returned an empty answer".to_string(),
            ))
        }
    } else {
        Ok(StreamedCompletion {
            content: full,
            reasoning,
            usage,
            timings,
        })
    }
}

#[derive(Serialize)]
struct ToolChatRequest<'a> {
    model: &'a str,
    messages: &'a [serde_json::Value],
    tools: &'a serde_json::Value,
    stream: bool,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<i32>,
    stream_options: StreamOptions,
    /// Maximum tokens the model may generate in this turn.
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chat_template_kwargs: Option<&'a serde_json::Value>,
}

/// Like [`complete_stream`], but offers the model a `tools` array and collects
/// the tool calls it asks for. Reasoning is streamed the same way as the plain
/// path, so the agent's thinking reaches the transcript too.
pub async fn complete_stream_with_tools(
    serving: &Serving,
    model: &str,
    messages: &[serde_json::Value],
    tools: &serde_json::Value,
    sampling: SamplingParams,
    mut on_delta: impl FnMut(&str),
    mut on_reasoning: impl FnMut(&str),
) -> AppResult<StreamedToolCompletion> {
    let http = client(RESPONSE_TIMEOUT)?;

    let request = ToolChatRequest {
        model,
        messages,
        tools,
        stream: true,
        temperature: sampling.temperature,
        top_p: sampling.top_p,
        top_k: sampling.top_k,
        stream_options: StreamOptions { include_usage: true },
        max_tokens: sampling.max_tokens,
        reasoning_effort: sampling.reasoning_effort.as_deref(),
        chat_template_kwargs: sampling.chat_template_kwargs.as_ref(),
    };

    let response = http
        .post(endpoint(serving, "/v1/chat/completions"))
        .bearer_auth(&serving.token)
        .json(&request)
        .send()
        .await
        .map_err(|error| AppError::msg(format!("llama-server did not answer: {error}")))?;

    let status = response.status();
    if !status.is_success() {
        let detail = response.text().await.unwrap_or_default();
        let detail = detail.trim();
        return Err(AppError::ServerRejected(if detail.is_empty() {
            status.to_string()
        } else {
            detail.to_string()
        }));
    }

    let mut full = String::new();
    let mut reasoning = String::new();
    let mut usage: Option<Usage> = None;
    let mut timings: Option<Timings> = None;
    // Tool calls are assembled by their stream index; the argument fragments
    // arrive in order and are simply appended.
    let mut calls: std::collections::BTreeMap<u32, ToolCallRequest> =
        std::collections::BTreeMap::new();
    let mut buffer = String::new();
    let mut stream = response.bytes_stream();
    let mut done = false;

    while !done {
        let chunk = match stream.next().await {
            Some(chunk) => chunk,
            None => break,
        };
        let chunk = chunk
            .map_err(|error| AppError::msg(format!("llama-server stopped mid-answer: {error}")))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(end) = buffer.find('\n') {
            let drained = buffer.drain(..=end).collect::<String>();
            let line = drained.trim();
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data == "[DONE]" {
                done = true;
                break;
            }
            let Ok(parsed) = serde_json::from_str::<StreamChunk>(data) else {
                continue;
            };
            if parsed.usage.is_some() {
                usage = parsed.usage;
            }
            if parsed.timings.is_some() {
                timings = parsed.timings;
            }
            let Some(delta) = parsed.choices.into_iter().next().map(|choice| choice.delta) else {
                continue;
            };

            let thinking = delta
                .reasoning_content
                .or(delta.reasoning)
                .filter(|text| !text.is_empty());
            if let Some(text) = thinking {
                on_reasoning(&text);
                reasoning.push_str(&text);
            }
            if let Some(text) = delta.content.filter(|text| !text.is_empty()) {
                on_delta(&text);
                full.push_str(&text);
            }

            for (position, partial) in delta.tool_calls.into_iter().enumerate() {
                let index = partial.index.unwrap_or(position as u32);
                let entry = calls.entry(index).or_insert_with(|| ToolCallRequest {
                    id: String::new(),
                    name: String::new(),
                    arguments: String::new(),
                });
                if let Some(id) = partial.id.filter(|id| !id.is_empty()) {
                    entry.id = id;
                }
                if let Some(function) = partial.function {
                    if let Some(name) = function.name.filter(|name| !name.is_empty()) {
                        entry.name.push_str(&name);
                    }
                    if let Some(arguments) = function.arguments {
                        entry.arguments.push_str(&arguments);
                    }
                }
            }
        }
    }

    drop(stream);
    if full.trim().is_empty() && calls.is_empty() {
        return Err(AppError::ServerRejected(
            "the model returned an empty answer".to_string(),
        ));
    }

    Ok(StreamedToolCompletion {
        content: full,
        reasoning,
        tool_calls: calls.into_values().collect(),
        usage,
        timings,
    })
}
