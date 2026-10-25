use super::Page;
use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;

pub struct BrowserFetcher { chrome_path: Option<String> }

impl BrowserFetcher {
    pub fn new(chrome_path: Option<String>) -> Self { Self { chrome_path } }
    pub async fn fetch(&self, url: &str) -> Result<Page, String> {
        let mut builder = BrowserConfig::builder();
        if let Some(p) = &self.chrome_path { builder = builder.chrome_executable(p); }
        let config = builder.build().map_err(|e| e.to_string())?;
        let (mut browser, mut handler) = Browser::launch(config).await.map_err(|e| e.to_string())?;
        let handle = tokio::spawn(async move { while handler.next().await.is_some() {} });
        let page = browser.new_page(url).await.map_err(|e| e.to_string())?;
        page.wait_for_navigation().await.map_err(|e| e.to_string())?;
        let html = page.content().await.map_err(|e| e.to_string())?;
        page.close().await.map_err(|e| e.to_string())?;
        browser.close().await.map_err(|e| e.to_string())?;
        handle.abort();
        let markdown = crate::research::fetch::http::extract_markdown(&html).unwrap_or(html);
        Ok(Page { url: url.to_string(), markdown })
    }
}