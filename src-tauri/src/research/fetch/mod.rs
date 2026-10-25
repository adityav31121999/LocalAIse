pub mod http;
pub mod browser;
pub use http::HttpFetcher;
pub use browser::BrowserFetcher;

#[derive(Clone, Debug)]
pub struct Page { pub url: String, pub markdown: String }