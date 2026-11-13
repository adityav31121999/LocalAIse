use crate::error::AppResult;
use async_trait::async_trait;
use reqwest::header::HeaderMap;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Trait for HTTP clients. Allows swapping real client for recording/replaying.
#[async_trait]
pub trait HttpClient: Send + Sync {
    async fn get(&self, url: &str, headers: HeaderMap) -> AppResult<HttpResponse>;
    async fn get_with_redirect_policy(
        &self,
        url: &str,
        headers: HeaderMap,
        max_redirects: usize,
        redirect_check: Box<dyn Fn(String) -> Result<(), String> + Send + Sync>,
    ) -> AppResult<HttpResponse>;
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: String,
    pub url: String, // Final URL after redirects
}

/// Configuration for the HTTP client.
#[derive(Debug, Clone)]
pub struct HttpClientConfig {
    pub timeout: Duration,
    pub max_response_bytes: u64,
    pub user_agent: String,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(15),
            max_response_bytes: 5 * 1024 * 1024, // 5 MB
            user_agent: "localAIse-ResearchAgent/0.1.0".to_string(),
        }
    }
}

/// Real HTTP client using reqwest.
pub struct RealHttpClient {
    client: reqwest::Client,
    config: HttpClientConfig,
}

impl RealHttpClient {
    pub fn new(config: HttpClientConfig) -> AppResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .redirect(reqwest::redirect::Policy::none()) // We handle redirects manually
            .build()
            .map_err(|e| crate::error::AppError::msg(format!("HTTP client error: {e}")))?;
        Ok(Self { client, config })
    }
}

#[async_trait]
impl HttpClient for RealHttpClient {
    async fn get(&self, url: &str, headers: HeaderMap) -> AppResult<HttpResponse> {
        let mut request = self.client.get(url);
        for (k, v) in headers.iter() {
            request = request.header(k.as_str(), v.as_bytes());
        }
        request = request.header("User-Agent", &self.config.user_agent);

        let response = request.send().await.map_err(|e| crate::error::AppError::msg(format!("Failed to fetch {url}: {e}")))?;

        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(crate::error::AppError::msg(format!("Web request failed with HTTP {status}")));
        }

        let content_length = response.content_length();
        if content_length.is_some_and(|n| n > self.config.max_response_bytes) {
            return Err(crate::error::AppError::msg("Response too large (>5 MB)."));
        }

        let headers = response.headers().clone();
        let final_url = response.url().to_string();
        let body = response.text().await.map_err(|e| crate::error::AppError::msg(format!("Failed to read response body: {e}")))?;

        Ok(HttpResponse { status, headers, body, url: final_url })
    }

    async fn get_with_redirect_policy(
        &self,
        url: &str,
        headers: HeaderMap,
        max_redirects: usize,
        redirect_check: Box<dyn Fn(String) -> Result<(), String> + Send + Sync>,
    ) -> AppResult<HttpResponse> {
        let mut current_url = url.to_string();
        let mut redirect_count = 0;

        loop {
            let response = self.get(&current_url, headers.clone()).await?;

            // Check for redirect
            if StatusCode::from_u16(response.status).unwrap().is_redirection() {
                redirect_count += 1;
                if redirect_count > max_redirects {
                    return Err(crate::error::AppError::msg("too many redirects"));
                }

                // Get redirect location
                if let Some(location) = response.headers.get("location") {
                    let location_str = location.to_str().map_err(|_| crate::error::AppError::msg("Invalid redirect location"))?;
                    let next_url = reqwest::Url::parse(&current_url)
                        .and_then(|u| u.join(location_str))
                        .map_err(|_| crate::error::AppError::msg("Invalid redirect URL"))?
                        .to_string();

                    // Check redirect policy - pass owned String to avoid lifetime issues
                    redirect_check(next_url.clone()).map_err(|e| crate::error::AppError::msg(e))?;

                    current_url = next_url;
                    continue;
                }
            }

            return Ok(response);
        }
    }
}

/// A fixture for recorded HTTP interactions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpFixture {
    pub request: HttpRequestFixture,
    pub response: HttpResponseFixture,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequestFixture {
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponseFixture {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub url: String, // Final URL after redirects
}

/// Recording/replaying HTTP client for tests.
pub struct FixtureHttpClient {
    fixtures: Arc<Mutex<HashMap<String, HttpFixture>>>,
    record_mode: bool,
    fixture_dir: Option<PathBuf>,
    _config: HttpClientConfig,
    real_client: Option<RealHttpClient>,
    recordings: Arc<Mutex<Vec<HttpFixture>>>,
}

impl FixtureHttpClient {
    pub fn new_replay(fixture_dir: impl AsRef<Path>, config: HttpClientConfig) -> AppResult<Self> {
        let dir = fixture_dir.as_ref();
        let mut fixtures = HashMap::new();

        // Load all .json fixtures from directory
        for entry in std::fs::read_dir(dir).map_err(|e| crate::error::AppError::msg(format!("Failed to read fixture dir: {e}")))? {
            let entry = entry.map_err(|e| crate::error::AppError::msg(format!("Failed to read dir entry: {e}")))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                let content = std::fs::read_to_string(&path).map_err(|e| crate::error::AppError::msg(format!("Failed to read fixture: {e}")))?;
                let fixture: HttpFixture = serde_json::from_str(&content).map_err(|e| crate::error::AppError::msg(format!("Failed to parse fixture: {e}")))?;
                let key = Self::fixture_key(&fixture.request.method, &fixture.request.url);
                fixtures.insert(key, fixture);
            }
        }

        Ok(Self {
            fixtures: Arc::new(Mutex::new(fixtures)),
            record_mode: false,
            fixture_dir: Some(dir.to_path_buf()),
            _config: config,
            real_client: None,
            recordings: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub fn new_record(fixture_dir: impl AsRef<Path>, config: HttpClientConfig) -> AppResult<Self> {
        let real_client = RealHttpClient::new(config.clone())?;
        let dir = fixture_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir).map_err(|e| crate::error::AppError::msg(format!("Failed to create fixture dir: {e}")))?;

        Ok(Self {
            fixtures: Arc::new(Mutex::new(HashMap::new())),
            record_mode: true,
            fixture_dir: Some(dir),
            _config: config,
            real_client: Some(real_client),
            recordings: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub fn new_passthrough(config: HttpClientConfig) -> AppResult<Self> {
        let real_client = RealHttpClient::new(config.clone())?;
        Ok(Self {
            fixtures: Arc::new(Mutex::new(HashMap::new())),
            record_mode: false,
            fixture_dir: None,
            _config: config,
            real_client: Some(real_client),
            recordings: Arc::new(Mutex::new(Vec::new())),
        })
    }

    fn fixture_key(method: &str, url: &str) -> String {
        // Normalize URL for matching: strip query params that vary (api keys, timestamps, etc.)
        let normalized = Self::normalize_url(url);
        format!("{} {}", method, normalized)
    }

    fn normalize_url(url: &str) -> String {
        // Basic normalization: strip tracking params, sort remaining
        if let Ok(mut parsed) = reqwest::Url::parse(url) {
            let mut pairs: Vec<_> = parsed.query_pairs()
                .filter(|(k, _)| !Self::is_tracking_param(k))
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            parsed.query_pairs_mut().clear();
            for (k, v) in pairs {
                parsed.query_pairs_mut().append_pair(&k, &v);
            }
            parsed.to_string()
        } else {
            url.to_string()
        }
    }

    fn is_tracking_param(key: &str) -> bool {
        matches!(
            key.to_lowercase().as_str(),
            "utm_source" | "utm_medium" | "utm_campaign" | "utm_term" | "utm_content"
                | "gclid" | "fbclid" | "msclkid" | "ref" | "ref_src" | "source"
                | "timestamp" | "ts" | "nonce" | "sig" | "signature" | "token"
        )
    }

    async fn do_request(&self, url: &str, headers: HeaderMap) -> AppResult<HttpResponse> {
        let key = Self::fixture_key("GET", url);

        // Try replay first
        if let Some(fixture) = self.fixtures.lock().unwrap().get(&key) {
            return Ok(HttpResponse {
                status: fixture.response.status,
                headers: fixture.response.headers.iter()
                    .map(|(k, v)| (k.parse().unwrap(), v.parse().unwrap()))
                    .collect(),
                body: fixture.response.body.clone(),
                url: fixture.response.url.clone(),
            });
        }

        // In replay-only mode, fail if not found
        if !self.record_mode {
            return Err(crate::error::AppError::msg(format!("No fixture for {key}")));
        }

        // Record mode: make real request
        let real_client = self.real_client.as_ref().ok_or_else(|| crate::error::AppError::msg("No real client in record mode"))?;
        let response = real_client.get(url, headers.clone()).await?;

        // Save fixture
        let fixture = HttpFixture {
            request: HttpRequestFixture {
                method: "GET".to_string(),
                url: url.to_string(),
                headers: headers.iter()
                    .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect(),
            },
            response: HttpResponseFixture {
                status: response.status,
                headers: response.headers.iter()
                    .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect(),
                body: response.body.clone(),
                url: response.url.clone(),
            },
        };

        self.fixtures.lock().unwrap().insert(key.clone(), fixture.clone());
        self.recordings.lock().unwrap().push(fixture);

        Ok(response)
    }
}

#[async_trait]
impl HttpClient for FixtureHttpClient {
    async fn get(&self, url: &str, headers: HeaderMap) -> AppResult<HttpResponse> {
        self.do_request(url, headers).await
    }

    async fn get_with_redirect_policy(
        &self,
        url: &str,
        headers: HeaderMap,
        _max_redirects: usize,
        _redirect_check: Box<dyn Fn(String) -> Result<(), String> + Send + Sync>,
    ) -> AppResult<HttpResponse> {
        // For fixtures, we don't follow redirects - the fixture should have the final response
        self.do_request(url, headers).await
    }
}

impl FixtureHttpClient {
    /// Save all recordings to the fixture directory.
    pub fn save_recordings(&self) -> AppResult<()> {
        let dir = self.fixture_dir.as_ref().ok_or_else(|| crate::error::AppError::msg("No fixture dir"))?;
        let recordings = self.recordings.lock().unwrap();
        for (i, fixture) in recordings.iter().enumerate() {
            let key = Self::fixture_key(&fixture.request.method, &fixture.request.url);
            // Sanitize key for filename
            let filename = key
                .chars()
                .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
                .collect::<String>();
            let path = dir.join(format!("{i:04}_{filename}.json"));
            let content = serde_json::to_string_pretty(fixture).map_err(|e| crate::error::AppError::msg(format!("Failed to serialize fixture: {e}")))?;
            std::fs::write(&path, content).map_err(|e| crate::error::AppError::msg(format!("Failed to write fixture: {e}")))?;
        }
        Ok(())
    }
}