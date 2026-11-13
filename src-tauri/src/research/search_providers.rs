use crate::research::error::{ResearchError, ResearchResult};
use crate::research::search::{SearchProvider, SearchResult};
use async_trait::async_trait;
use scraper::{Html, Selector};
use urlencoding;

/// DuckDuckGo HTML search provider (fallback, no API key needed).
pub struct DuckDuckGoSearch {
    http_client: reqwest::Client,
}

impl Default for DuckDuckGoSearch {
    fn default() -> Self {
        Self::new()
    }
}

impl DuckDuckGoSearch {
    pub fn new() -> Self {
        Self {
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .redirect(reqwest::redirect::Policy::limited(5))
                .build()
                .expect("DDG HTTP client"),
        }
    }
}

#[async_trait]
impl SearchProvider for DuckDuckGoSearch {
    async fn search(&self, query: &str, max_results: usize) -> ResearchResult<Vec<SearchResult>> {
        let encoded = urlencoding::encode(query);
        let url = format!("https://html.duckduckgo.com/html/?q={encoded}");

        let resp = self.http_client.get(&url)
            .header("User-Agent", "localAIse-ResearchAgent/0.1.0")
            .send().await
            .map_err(|e| ResearchError::Network(format!("DDG request failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(ResearchError::HttpStatus {
                status: resp.status().as_u16(),
                url: url.clone(),
                body: resp.text().await.unwrap_or_default(),
            });
        }

        let html = resp.text().await
            .map_err(|e| ResearchError::Network(format!("DDG read failed: {e}")))?;

        let results = parse_ddg_html(&html, max_results);
        Ok(results)
    }

    fn name(&self) -> &'static str {
        "duckduckgo"
    }
}

/// Parse DuckDuckGo HTML results.
fn parse_ddg_html(html: &str, max_results: usize) -> Vec<SearchResult> {
    let doc = Html::parse_document(html);
    let link_sel = Selector::parse("a.result__a").unwrap();
    let snippet_sel = Selector::parse("a.result__snippet").unwrap();

    // Collect all snippets first, keyed by their preceding link
    let mut snippets = std::collections::HashMap::new();
    for snippet in doc.select(&snippet_sel) {
        let text = snippet.text().collect::<String>().trim().to_string();
        // Find the preceding link
        if let Some(prev_link) = snippet.prev_siblings()
            .filter_map(|n| n.value().as_element())
            .find(|el| el.attr("class").map(|c| c.contains("result__a")).unwrap_or(false))
        {
            if let Some(href) = prev_link.attr("href") {
                snippets.insert(href.to_string(), text);
            }
        }
    }

    let mut results = Vec::new();
    let mut seen_urls = std::collections::HashSet::new();

    for link in doc.select(&link_sel) {
        if results.len() >= max_results { break; }

        let href = link.value().attr("href").unwrap_or("");
        let title = link.text().collect::<String>().trim().to_string();

        let snippet = snippets.get(href).cloned().unwrap_or_default();

        // DDG redirects through their own domain; extract the real URL
        let real_url = if href.starts_with("//duckduckgo.com/l/?uddg=") {
            urlencoding::decode(href.split("uddg=").nth(1).unwrap_or(""))
                .map(|s| s.to_string())
                .unwrap_or_else(|_| href.to_string())
        } else if href.starts_with("http") {
            href.to_string()
        } else {
            continue;
        };

        if !seen_urls.insert(real_url.clone()) { continue; }
        if title.len() < 3 { continue; }

        results.push(SearchResult {
            title,
            url: real_url,
            snippet,
            published_at: None,
            provider: "duckduckgo".to_string(),
            rank: results.len() + 1,
        });
    }

    results
}

/// Brave Search API provider (requires API key).
pub struct BraveSearch {
    http_client: reqwest::Client,
    api_key: String,
}

impl BraveSearch {
    pub fn new(api_key: String) -> Self {
        Self {
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .expect("Brave HTTP client"),
            api_key,
        }
    }
}

#[async_trait]
impl SearchProvider for BraveSearch {
    async fn search(&self, query: &str, max_results: usize) -> ResearchResult<Vec<SearchResult>> {
        let url = "https://api.search.brave.com/res/v1/web/search";

        let resp = self.http_client.get(url)
            .query(&[("q", query), ("count", &max_results.to_string())])
            .header("Accept", "application/json")
            .header("X-Subscription-Token", &self.api_key)
            .send().await
            .map_err(|e| ResearchError::Network(format!("Brave request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(ResearchError::HttpStatus { status, url: url.to_string(), body });
        }

        let json: serde_json::Value = resp.json().await
            .map_err(|e| ResearchError::Parse(format!("Brave JSON parse failed: {e}")))?;

        let mut results = Vec::new();
        if let Some(web) = json.get("web").and_then(|w| w.get("results")) {
            for (i, item) in web.as_array().unwrap_or(&vec![]).iter().enumerate() {
                if i >= max_results { break; }
                let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let url = item.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let snippet = item.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let published = item.get("age").and_then(|v| v.as_str()).and_then(parse_brave_age);

                if title.is_empty() || url.is_empty() { continue; }

                results.push(SearchResult {
                    title,
                    url,
                    snippet,
                    published_at: published,
                    provider: "brave".to_string(),
                    rank: i + 1,
                });
            }
        }

        Ok(results)
    }

    fn name(&self) -> &'static str {
        "brave"
    }

    async fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }
}

/// Parse Brave's age string (e.g., "2 days ago") to DateTime.
fn parse_brave_age(age: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    // Simplified parsing - in production use a proper relative time parser
    let age = age.to_lowercase();
    let now = chrono::Utc::now();
    if let Some(num) = age.split_whitespace().next().and_then(|s| s.parse::<i64>().ok()) {
        if age.contains("hour") {
            return Some(now - chrono::Duration::hours(num));
        } else if age.contains("day") {
            return Some(now - chrono::Duration::days(num));
        } else if age.contains("week") {
            return Some(now - chrono::Duration::weeks(num));
        } else if age.contains("month") {
            return Some(now - chrono::Duration::days(num * 30));
        } else if age.contains("year") {
            return Some(now - chrono::Duration::days(num * 365));
        }
    }
    None
}

/// Tavily Search API provider (requires API key).
pub struct TavilySearch {
    http_client: reqwest::Client,
    api_key: String,
}

impl TavilySearch {
    pub fn new(api_key: String) -> Self {
        Self {
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Tavily HTTP client"),
            api_key,
        }
    }
}

#[async_trait]
impl SearchProvider for TavilySearch {
    async fn search(&self, query: &str, max_results: usize) -> ResearchResult<Vec<SearchResult>> {
        let url = "https://api.tavily.com/search";

        let body = serde_json::json!({
            "query": query,
            "max_results": max_results,
            "search_depth": "advanced",
            "include_answer": false,
            "include_images": false,
            "include_raw_content": false,
        });

        let resp = self.http_client.post(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send().await
            .map_err(|e| ResearchError::Network(format!("Tavily request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(ResearchError::HttpStatus { status, url: url.to_string(), body });
        }

        let json: serde_json::Value = resp.json().await
            .map_err(|e| ResearchError::Parse(format!("Tavily JSON parse failed: {e}")))?;

        let mut results = Vec::new();
        if let Some(items) = json.get("results").and_then(|v| v.as_array()) {
            for (i, item) in items.iter().enumerate() {
                if i >= max_results { break; }
                let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let url = item.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let snippet = item.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let published = item.get("published_date").and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc));

                if title.is_empty() || url.is_empty() { continue; }

                results.push(SearchResult {
                    title,
                    url,
                    snippet,
                    published_at: published,
                    provider: "tavily".to_string(),
                    rank: i + 1,
                });
            }
        }

        Ok(results)
    }

    fn name(&self) -> &'static str {
        "tavily"
    }

    async fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }
}

/// SearXNG instance provider (self-hosted, no API key).
pub struct SearXNGSearch {
    http_client: reqwest::Client,
    instance_url: String,
}

impl SearXNGSearch {
    pub fn new(instance_url: String) -> Self {
        Self {
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .expect("SearXNG HTTP client"),
            instance_url,
        }
    }
}

#[async_trait]
impl SearchProvider for SearXNGSearch {
    async fn search(&self, query: &str, max_results: usize) -> ResearchResult<Vec<SearchResult>> {
        let url = format!("{}/search", self.instance_url.trim_end_matches('/'));

        let resp = self.http_client.get(&url)
            .query(&[("q", query), ("format", "json"), ("max_results", &max_results.to_string())])
            .send().await
            .map_err(|e| ResearchError::Network(format!("SearXNG request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(ResearchError::HttpStatus { status, url, body });
        }

        let json: serde_json::Value = resp.json().await
            .map_err(|e| ResearchError::Parse(format!("SearXNG JSON parse failed: {e}")))?;

        let mut results = Vec::new();
        if let Some(items) = json.get("results").and_then(|v| v.as_array()) {
            for (i, item) in items.iter().enumerate() {
                if i >= max_results { break; }
                let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let url = item.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let snippet = item.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let published = item.get("publishedDate").and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc));

                if title.is_empty() || url.is_empty() { continue; }

                results.push(SearchResult {
                    title,
                    url,
                    snippet,
                    published_at: published,
                    provider: "searxng".to_string(),
                    rank: i + 1,
                });
            }
        }

        Ok(results)
    }

    fn name(&self) -> &'static str {
        "searxng"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::search::{canonicalize_url, dedupe_results, is_near_duplicate};

    #[test]
    fn test_canonicalize() {
        assert_eq!(
            canonicalize_url("https://example.com/?utm_source=google&b=2&a=1"),
            "https://example.com/?a=1&b=2"
        );
    }

    #[test]
    fn test_dedupe() {
        let results = vec![
            SearchResult { title: "A".into(), url: "https://a.com".into(), snippet: "".into(), published_at: None, provider: "ddg".into(), rank: 1 },
            SearchResult { title: "B".into(), url: "https://a.com?utm_source=x".into(), snippet: "".into(), published_at: None, provider: "ddg".into(), rank: 2 },
        ];
        let deduped = dedupe_results(results);
        assert_eq!(deduped.len(), 1);
    }

    #[test]
    fn test_simhash() {
        let t1 = "The quick brown fox jumps over the lazy dog";
        let t2 = "The quick brown fox jumps over the lazy dog.";
        assert!(is_near_duplicate(t1, t2, 3));
    }
}