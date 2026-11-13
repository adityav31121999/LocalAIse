use std::path::PathBuf;
use std::sync::Arc;

use crate::network::{gate, http_client::HttpClient, policy};
use crate::research::fetch::{BrowserFetcher, Page};
use crate::research::fetch::http::extract_markdown;
use crate::research::search::SearchProvider;

pub struct ResearchNet {
    pub project_root: PathBuf,
    pub internet_enabled: bool,
    http_client: Option<Arc<dyn HttpClient>>,
    search_provider: Option<Arc<dyn SearchProvider>>,
}

impl ResearchNet {
    pub fn new(project_root: PathBuf, internet_enabled: bool) -> Self {
        Self {
            project_root,
            internet_enabled,
            http_client: None,
            search_provider: None,
        }
    }

    /// Create a ResearchNet with a custom HTTP client (for testing with fixtures).
    pub fn with_client(project_root: PathBuf, internet_enabled: bool, client: Arc<dyn HttpClient>) -> Self {
        Self {
            project_root,
            internet_enabled,
            http_client: Some(client),
            search_provider: None,
        }
    }

    /// Create a ResearchNet with a custom search provider.
    pub fn with_search_provider(mut self, provider: Arc<dyn SearchProvider>) -> Self {
        self.search_provider = Some(provider);
        self
    }

    fn client(&self) -> Arc<dyn HttpClient> {
        self.http_client.clone().unwrap_or_else(|| Arc::new(crate::network::http_client::RealHttpClient::new(
            crate::network::http_client::HttpClientConfig::default()
        ).expect("default http client")))
    }

    fn search_provider(&self) -> Arc<dyn SearchProvider> {
        self.search_provider.clone().unwrap_or_else(|| Arc::new(crate::research::search_providers::DuckDuckGoSearch::new()))
    }

    pub async fn check(&self, url: &str) -> Result<(), String> {
        policy::check_external_resolved(url, self.internet_enabled).await.map_err(|e| e.to_string())
    }

    /// Search using the configured search provider.
    pub async fn search(&self, query: &str, max_results: usize) -> Result<Vec<crate::research::search::SearchResult>, String> {
        if !self.internet_enabled {
            return Err("Internet access is disabled for this project".to_string());
        }
        let provider = self.search_provider();
        provider.search(query, max_results).await.map_err(|e| e.to_string())
    }

    /// Legacy method for backward compatibility - fetches raw HTML from a search URL.
    pub async fn search_html(&self, url: &str) -> Result<String, String> {
        let client = self.client();
        gate::fetch_raw_url_with_client(&self.project_root, url, self.internet_enabled, "research:search", client.as_ref())
            .await.map_err(|e| e.to_string())
    }

    pub async fn fetch_http(&self, url: &str) -> Result<Page, String> {
        let client = self.client();
        let html = gate::fetch_raw_url_with_client(&self.project_root, url, self.internet_enabled, "research:fetch", client.as_ref())
            .await.map_err(|e| e.to_string())?;
        let markdown = extract_markdown(&html).unwrap_or_else(|_| gate::clean_html(&html));
        Ok(Page { url: url.to_string(), markdown })
    }

    pub async fn fetch_browser(&self, b: &BrowserFetcher, url: &str) -> Result<Page, String> {
        self.check(url).await?;
        gate::audit_browser(&self.project_root, url, "research:browser");
        b.fetch(url).await
    }
}