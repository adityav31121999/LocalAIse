use super::Page;
use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;
use std::time::Duration;

pub struct BrowserFetcher {
    chrome_path: Option<String>,
    /// One Chromium instance per run, launched lazily on the first fetch and
    /// reused for every page. Launching per page cost 1-3s+ each and leaked
    /// profile dirs when a launch failed mid-way; reusing one browser keeps a
    /// Deep run to a single startup cost.
    launched: tokio::sync::Mutex<Option<Launched>>,
}

struct Launched {
    browser: Browser,
    handler: tokio::task::JoinHandle<()>,
    profile: std::path::PathBuf,
}

/// Hard cap on the whole browser fetch (launch + navigation + read). Without
/// it, a page that never finishes loading would hang the research run forever
/// — and would ignore the user's Stop request.
const FETCH_TIMEOUT: Duration = Duration::from_secs(60);
/// Cap on how long we wait for the page to finish navigating.
const NAV_TIMEOUT: Duration = Duration::from_secs(30);

/// Per-process counter so concurrent runs/tests never share a profile dir.
static PROFILE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl BrowserFetcher {
    pub fn new(chrome_path: Option<String>) -> Self {
        Self { chrome_path, launched: tokio::sync::Mutex::new(None) }
    }

    pub async fn fetch(&self, url: &str) -> Result<Page, String> {
        tokio::time::timeout(FETCH_TIMEOUT, self.fetch_inner(url))
            .await
            .map_err(|_| format!("Browser fetch timed out after {}s", FETCH_TIMEOUT.as_secs()))?
    }

    async fn fetch_inner(&self, url: &str) -> Result<Page, String> {
        let mut guard = self.launched.lock().await;
        if guard.is_none() {
            let launched = Self::launch(self.chrome_path.as_deref()).await?;
            *guard = Some(launched);
        }
        let Some(launched) = guard.as_mut() else {
            return Err("browser launch failed".to_string());
        };

        let result = Self::fetch_page(&mut launched.browser, url).await;
        // A dead browser is dropped so the next fetch relaunches instead of
        // hammering a zombie CDP connection.
        if let Err(e) = &result {
            if e.contains("disconnected") || e.contains("closed") || e.contains("browser") {
                if let Some(dead) = guard.take() {
                    dead.handler.abort();
                    let _ = std::fs::remove_dir_all(&dead.profile);
                }
            }
        }
        result
    }

    async fn launch(chrome_path: Option<&str>) -> Result<Launched, String> {
        let mut builder = BrowserConfig::builder();
        if let Some(p) = chrome_path { builder = builder.chrome_executable(p); }
        // Isolated profile for this run. Without one, an Edge/Chrome process
        // already running the user's default profile swallows this launch and
        // exits with code 21, so the CDP websocket never appears. The dir is
        // removed by `shutdown` or `Drop`, on every exit path.
        let seq = PROFILE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let profile = std::env::temp_dir().join(format!(
            "localaise-chromium-{}-{seq}",
            std::process::id()
        ));
        builder = builder.user_data_dir(&profile);

        let config = builder.build().map_err(|e| e.to_string())?;
        match Browser::launch(config).await {
            Ok((browser, mut handler)) => {
                let handle = tokio::spawn(async move { while handler.next().await.is_some() {} });
                Ok(Launched { browser, handler: handle, profile })
            }
            Err(e) => {
                let _ = std::fs::remove_dir_all(&profile);
                Err(e.to_string())
            }
        }
    }

    async fn fetch_page(browser: &mut Browser, url: &str) -> Result<Page, String> {
        let page = browser.new_page(url).await.map_err(|e| e.to_string())?;
        let result = async {
            tokio::time::timeout(NAV_TIMEOUT, page.wait_for_navigation())
                .await
                .map_err(|_| format!("Page navigation timed out after {}s", NAV_TIMEOUT.as_secs()))?
                .map_err(|e| e.to_string())?;
            let html = page.content().await.map_err(|e| e.to_string())?;
            Ok(html)
        }
        .await;
        let _ = page.close().await;
        match result {
            Ok(html) => {
                let markdown = crate::research::fetch::http::extract_markdown(&html).unwrap_or(html);
                Ok(Page { url: url.to_string(), markdown })
            }
            Err(e) => Err(e),
        }
    }

    /// Close the shared browser (if launched) and remove its throwaway profile.
    /// Idempotent; safe to call when no fetch ever happened.
    pub async fn shutdown(&self) {
        let mut guard = self.launched.lock().await;
        if let Some(launched) = guard.take() {
            let Launched { mut browser, handler, profile } = launched;
            let _ = browser.close().await;
            handler.abort();
            let _ = std::fs::remove_dir_all(&profile);
        }
    }
}

impl Drop for BrowserFetcher {
    fn drop(&mut self) {
        // Best-effort backstop for runs that end without `shutdown()` (e.g. an
        // early error return). The handler task is detached, so abort it and
        // remove the profile dir; never block or panic in drop.
        if let Ok(mut guard) = self.launched.try_lock() {
            if let Some(launched) = guard.take() {
                launched.handler.abort();
                let _ = std::fs::remove_dir_all(&launched.profile);
            }
        }
    }
}
