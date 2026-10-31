use super::Page;
use reqwest::Client;

pub struct HttpFetcher { client: Client }

impl Default for HttpFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpFetcher {
    pub fn new() -> Self { Self { client: Client::new() } }
    pub async fn fetch(&self, url: &str) -> Result<Page, String> {
        let resp = self.client.get(url).send().await.map_err(|e| e.to_string())?.text().await.map_err(|e| e.to_string())?;
        // Readability extraction first, then HTML -> markdown.
        let markdown = match extract_markdown(&resp) {
            Ok(md) if !md.trim().is_empty() => md,
            _ => resp.lines().filter(|l| !l.trim().is_empty()).collect::<Vec<_>>().join("\n"),
        };
        Ok(Page { url: url.to_string(), markdown })
    }
}

pub fn extract_markdown(html: &str) -> Result<String, String> {
    use dom_smoothie::Readability;
    let mut readability = Readability::new(html, None, None).map_err(|e| format!("{e}"))?;
    let article = readability.parse().map_err(|e| format!("{e}"))?;
    let md = htmd::convert(&article.content).map_err(|e| format!("{e}"))?;
    Ok(md)
}