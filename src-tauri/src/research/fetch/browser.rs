use super::Page;
use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;

pub struct BrowserFetcher { chrome_path: Option<String> }

/// Per-process counter so every fetch gets a unique throwaway profile dir.
static PROFILE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl BrowserFetcher {
    pub fn new(chrome_path: Option<String>) -> Self { Self { chrome_path } }
    pub async fn fetch(&self, url: &str) -> Result<Page, String> {
        let mut builder = BrowserConfig::builder();
        if let Some(p) = &self.chrome_path { builder = builder.chrome_executable(p); }
        // Isolated throwaway profile per fetch. Without one, an Edge/Chrome
        // process already running the user's default profile swallows this
        // launch and exits with code 21, so the CDP websocket never appears.
        // The sequence counter keeps parallel fetches from colliding on the
        // same profile dir (same pid + same millisecond otherwise).
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let seq = PROFILE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let profile = std::env::temp_dir().join(format!(
            "localaise-chromium-{}-{}-{stamp}",
            std::process::id(),
            seq
        ));
        builder = builder.user_data_dir(&profile);
        let config = builder.build().map_err(|e| e.to_string())?;
        let (mut browser, mut handler) = Browser::launch(config).await.map_err(|e| e.to_string())?;
        let handle = tokio::spawn(async move { while handler.next().await.is_some() {} });
        let page = browser.new_page(url).await.map_err(|e| e.to_string())?;
        page.wait_for_navigation().await.map_err(|e| e.to_string())?;
        let html = page.content().await.map_err(|e| e.to_string())?;
        page.close().await.map_err(|e| e.to_string())?;
        browser.close().await.map_err(|e| e.to_string())?;
        handle.abort();
        let _ = std::fs::remove_dir_all(&profile);
        let markdown = crate::research::fetch::http::extract_markdown(&html).unwrap_or(html);
        Ok(Page { url: url.to_string(), markdown })
    }
}