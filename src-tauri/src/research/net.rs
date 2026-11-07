use std::path::PathBuf;

use crate::network::{gate, policy};
use crate::research::fetch::{BrowserFetcher, Page};
use crate::research::fetch::http::extract_markdown;

pub struct ResearchNet { pub project_root: PathBuf, pub internet_enabled: bool }

impl ResearchNet {
    pub async fn check(&self, url: &str) -> Result<(), String> {
        policy::check_external_resolved(url, self.internet_enabled).await.map_err(|e| e.to_string())
    }

    pub async fn search_html(&self, url: &str) -> Result<String, String> {
        gate::fetch_raw_url(&self.project_root, url, self.internet_enabled, "research:search")
            .await.map_err(|e| e.to_string())
    }

    pub async fn fetch_http(&self, url: &str) -> Result<Page, String> {
        let html = gate::fetch_raw_url(&self.project_root, url, self.internet_enabled, "research:fetch")
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