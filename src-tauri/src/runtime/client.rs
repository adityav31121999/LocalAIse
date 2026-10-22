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

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
    temperature: f32,
    /// `false` starts a chat with no KV-cache reuse at all; `None` lets
    /// llama-server reuse the longest matching prompt prefix.
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_prompt: Option<bool>,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ReplyMessage,
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

/// Asks for one complete answer and returns its text.
pub async fn complete(serving: &Serving, model: &str, messages: &[ChatMessage]) -> AppResult<String> {    let http = client(RESPONSE_TIMEOUT)?;

    let request = ChatRequest {
        model,
        messages,
        stream: false,
        temperature: 0.2,
        cache_prompt: None,
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

// Streaming response shapes (OpenAI "delta" chunks).
#[derive(Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
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
}

/// What a streamed turn produced: the answer and, when available, its thinking.
pub struct StreamedCompletion {
    pub content: String,
    pub reasoning: String,
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
pub async fn complete_stream(
    serving: &Serving,
    model: &str,
    messages: &[ChatMessage],
    reuse_cache: bool,
    mut on_delta: impl FnMut(&str),
    mut on_reasoning: impl FnMut(&str),
) -> AppResult<StreamedCompletion> {
    let http = client(RESPONSE_TIMEOUT)?;

    let request = ChatRequest {
        model,
        messages,
        stream: true,
        temperature: 0.2,
        cache_prompt: if reuse_cache { None } else { Some(false) },
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
        Err(AppError::ServerRejected(
            "the model returned an empty answer".to_string(),
        ))
    } else {
        Ok(StreamedCompletion {
            content: full,
            reasoning,
        })
    }
}
