use crate::error::AppError;
use std::time::Duration;
use thiserror::Error;

/// Research-specific errors with actionable context for retries and UI.
#[derive(Debug, Error)]
pub enum ResearchError {
    #[error("Network request failed: {0}")]
    Network(String),

    #[error("DNS resolution failed for {host}: {reason}")]
    Dns { host: String, reason: String },

    #[error("Request to {url} blocked by policy: {reason}")]
    PolicyBlocked { url: String, reason: String },

    #[error("HTTP {status} fetching {url}: {body}")]
    HttpStatus { status: u16, url: String, body: String },

    #[error("Response too large ({bytes} bytes, limit {limit} bytes)")]
    ResponseTooLarge { bytes: u64, limit: u64 },

    #[error("Timeout after {secs}s fetching {url}")]
    Timeout { secs: u64, url: String },

    #[error("Too many redirects for {url}")]
    TooManyRedirects { url: String },

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("HTML parsing failed: {0}")]
    Parse(String),

    #[error("Search provider {provider} unavailable: {reason}")]
    SearchUnavailable { provider: String, reason: String },

    #[error("Search returned no results for query: {query}")]
    NoSearchResults { query: String },

    #[error("LLM request failed: {0}")]
    Llm(String),

    #[error("LLM returned invalid JSON for {role}: {raw}")]
    LlmInvalidJson { role: String, raw: String },

    #[error("LLM response missing required field {field} for {role}")]
    LlmMissingField { role: String, field: String },

    #[error("Budget exceeded: {kind} (limit {limit}, used {used})")]
    BudgetExceeded { kind: BudgetKind, limit: u64, used: u64 },

    #[error("Wall-clock time limit reached ({secs}s)")]
    WallClockExceeded { secs: u64 },

    #[error("Store operation failed: {0}")]
    Store(String),

    #[error("Source {source_id} not found in store")]
    SourceNotFound { source_id: i64 },

    #[error("Cancellation requested")]
    Cancelled,

    #[error("Offline research requires a project root")]
    MissingProjectRoot,

    #[error("Browser fetcher not available: {reason}")]
    BrowserUnavailable { reason: String },

    #[error("Internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::Display)]
pub enum BudgetKind {
    Rounds,
    Pages,
    Tokens,
    CostUnits,
    LlmCalls,
}

impl ResearchError {
    /// Whether this error is retryable (transient network/LLM issues).
    pub fn is_retryable(&self) -> bool {
        match self {
            ResearchError::Network(_) => true,
            ResearchError::Dns { .. } => true,
            ResearchError::HttpStatus { status, .. } if *status >= 500 => true,
            ResearchError::Timeout { .. } => true,
            ResearchError::TooManyRedirects { .. } => true,
            ResearchError::SearchUnavailable { .. } => true,
            ResearchError::Llm(_) => true,
            ResearchError::LlmInvalidJson { .. } => true,
            _ => false,
        }
    }

    /// Suggested backoff before retry.
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            ResearchError::Network(_) => Some(Duration::from_secs(2)),
            ResearchError::Dns { .. } => Some(Duration::from_secs(5)),
            ResearchError::HttpStatus { status, .. } if *status == 429 => Some(Duration::from_secs(30)),
            ResearchError::HttpStatus { status, .. } if *status >= 500 => Some(Duration::from_secs(10)),
            ResearchError::Timeout { .. } => Some(Duration::from_secs(5)),
            ResearchError::SearchUnavailable { .. } => Some(Duration::from_secs(10)),
            ResearchError::Llm(_) => Some(Duration::from_secs(5)),
            ResearchError::LlmInvalidJson { .. } => Some(Duration::from_secs(2)),
            _ => None,
        }
    }
}

impl From<ResearchError> for AppError {
    fn from(e: ResearchError) -> Self {
        AppError::msg(e.to_string())
    }
}

impl From<reqwest::Error> for ResearchError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            ResearchError::Timeout {
                secs: 0,
                url: e.url().map(|u| u.to_string()).unwrap_or_default(),
            }
        } else if e.is_redirect() {
            ResearchError::TooManyRedirects {
                url: e.url().map(|u| u.to_string()).unwrap_or_default(),
            }
        } else if let Some(status) = e.status() {
            ResearchError::HttpStatus {
                status: status.as_u16(),
                url: e.url().map(|u| u.to_string()).unwrap_or_default(),
                body: e.to_string(),
            }
        } else if e.is_connect() || e.is_request() {
            ResearchError::Network(e.to_string())
        } else {
            ResearchError::Internal(e.to_string())
        }
    }
}

impl From<url::ParseError> for ResearchError {
    fn from(e: url::ParseError) -> Self {
        ResearchError::InvalidUrl(e.to_string())
    }
}

impl From<rusqlite::Error> for ResearchError {
    fn from(e: rusqlite::Error) -> Self {
        ResearchError::Store(e.to_string())
    }
}

pub type ResearchResult<T> = Result<T, ResearchError>;