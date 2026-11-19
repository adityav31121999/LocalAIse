//! Pool-based end-to-end suite for deep research.
//!
//! Tests check out shared infrastructure from a pool instead of building and
//! tearing down per test — the pool model Playwright and Testcontainers use,
//! minus driving the UI or real network:
//!
//!   - `POOL` pre-registers fixture "sites" (named sets of HTML pages behind
//!     one in-memory HTTP client). Sites are read-only, so checkout is just an
//!     `Arc` clone and every test in the binary can safely share them. Each
//!     site is used by exactly one test, so request counters never leak
//!     across tests.
//!   - Project folders are created per test under the system temp dir with a
//!     process-unique suffix (parallel-safe) and removed at the end of the test.
//!   - The LLM is a `MockLlmClient` programmed per test; nothing here talks to
//!     llama-server, the public web, or Chromium.
//!
//! Fixture URLs live on `https://1.1.1.1/…` (a public IP literal): the network
//! gate passes IP literals without a DNS lookup, so the pipeline runs fully
//! offline while still exercising the policy/audit path.
//!
//! Assertions target the `research:event` stream plus the files a run leaves
//! behind (transcript, export payload, report under `outputs/`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use async_trait::async_trait;
use reqwest::header::HeaderMap;

use localaise_lib::error::{AppError, AppResult};
use localaise_lib::network::http_client::{HttpClient, HttpResponse};
use localaise_lib::research::budget::Budgets;
use localaise_lib::research::export::{ExportFormat, ExportOptions, export_report};
use localaise_lib::research::llm::LlmResponse;
use localaise_lib::research::net::ResearchNet;
use localaise_lib::research::orchestrator;
use localaise_lib::research::request::{ResearchMode, ResearchRequest};
use localaise_lib::research::search::{SearchProvider, SearchResult};
use localaise_lib::research::transcript;
use localaise_lib::research::{MockLlmClient, ResearchEvent};

// ---------------------------------------------------------------------------
// Fixture pages
// ---------------------------------------------------------------------------

const SEASONS_PAGES: &[(&str, &str)] = &[
    ("https://1.1.1.1/research/seasons/tilt",
     "<html><head><title>Axial tilt</title></head><body><article><h1>Axial tilt</h1><p>Earth's seasons happen because the planet's axis is tilted about 23.5 degrees relative to its orbital plane. As the planet orbits, different hemispheres receive more direct sunlight at different times of the year, and this variation in sunlight intensity drives the seasons.</p></article></body></html>"),
    ("https://1.1.1.1/research/seasons/orbit",
     "<html><head><title>Orbital distance</title></head><body><article><h1>Orbital distance</h1><p>Earth travels around the Sun in a nearly circular orbit that takes just over 365 days to complete. The distance from the Sun barely changes over the year and has a negligible effect on seasonal temperatures.</p></article></body></html>"),
    ("https://1.1.1.1/research/seasons/hemispheres",
     "<html><head><title>Hemispheres</title></head><body><article><h1>Hemispheres</h1><p>When the Northern Hemisphere tilts toward the Sun during June it experiences summer, while the Southern Hemisphere experiences winter at the same time. Six months later the situation is reversed.</p></article></body></html>"),
];

const PROVIDED_PAGES: &[(&str, &str)] = &[
    ("https://1.1.1.1/research/provided/press-release",
     "<html><head><title>Press release</title></head><body><article><h1>Press release</h1><p>The Acme Consortium announced in 2025 that its low-power accelerator chip entered mass production with volume shipments beginning in the third quarter. The chip is aimed at on-device inference workloads in laptops and phones.</p></article></body></html>"),
];

// ---------------------------------------------------------------------------
// In-memory fixture HTTP client (read-only pages, request counter)
// ---------------------------------------------------------------------------

struct FixtureSiteClient {
    pages: HashMap<String, String>,
    requests: AtomicU64,
}

impl FixtureSiteClient {
    fn new(pages: &[(&str, &str)]) -> Self {
        Self {
            pages: pages.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            requests: AtomicU64::new(0),
        }
    }

    fn request_count(&self) -> u64 {
        self.requests.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl HttpClient for FixtureSiteClient {
    async fn get(&self, url: &str, _headers: HeaderMap) -> AppResult<HttpResponse> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        match self.pages.get(url) {
            Some(body) => Ok(HttpResponse {
                status: 200,
                headers: HeaderMap::new(),
                body: body.clone(),
                url: url.to_string(),
            }),
            None => Err(AppError::msg(format!("No fixture page for {url}"))),
        }
    }

    async fn get_with_redirect_policy(
        &self,
        url: &str,
        headers: HeaderMap,
        _max_redirects: usize,
        _redirect_check: Box<
            dyn Fn(String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>
                + Send
                + Sync,
        >,
    ) -> AppResult<HttpResponse> {
        self.get(url, headers).await
    }
}

// ---------------------------------------------------------------------------
// Fixture search provider: deterministic results, counts calls
// ---------------------------------------------------------------------------

struct FixtureSearch {
    results: Vec<SearchResult>,
    calls: AtomicU64,
}

#[async_trait]
impl SearchProvider for FixtureSearch {
    async fn search(&self, _query: &str, _max_results: usize) -> localaise_lib::research::ResearchResult<Vec<SearchResult>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.results.clone())
    }

    fn name(&self) -> &'static str {
        "fixture-search"
    }

    fn endpoint(&self) -> String {
        // Public IP literal: passes the gate without a DNS lookup.
        "https://1.1.1.1/search".to_string()
    }
}

// ---------------------------------------------------------------------------
// The pool: pre-registered sites + per-test project skeletons
// ---------------------------------------------------------------------------

struct Pool {
    seasons: Arc<FixtureSiteClient>,
    provided: Arc<FixtureSiteClient>,
    projects: Mutex<Vec<PathBuf>>,
    seq: AtomicU64,
}

impl Pool {
    fn shared() -> &'static Pool {
        static POOL: OnceLock<Pool> = OnceLock::new();
        POOL.get_or_init(|| Pool {
            seasons: Arc::new(FixtureSiteClient::new(SEASONS_PAGES)),
            provided: Arc::new(FixtureSiteClient::new(PROVIDED_PAGES)),
            projects: Mutex::new(Vec::new()),
            seq: AtomicU64::new(0),
        })
    }

    /// Check out a fresh, isolated project skeleton (`data/`, `chats/`,
    /// `outputs/`). Unique per test even under parallel execution.
    fn checkout_project(&self, label: &str) -> PathBuf {
        let n = self.seq.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("LocalAIse-research-pool-{label}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("data")).expect("create data dir");
        std::fs::create_dir_all(root.join("outputs")).expect("create outputs dir");
        self.projects.lock().unwrap().push(root.clone());
        root
    }

    fn release_project(&self, root: &PathBuf) {
        let _ = std::fs::remove_dir_all(root);
        let mut projects = self.projects.lock().unwrap();
        projects.retain(|p| p != root);
    }

    fn seasons_arc(&self) -> Arc<FixtureSiteClient> {
        Arc::clone(&self.seasons)
    }
}

fn seasons_search() -> Arc<FixtureSearch> {
    Arc::new(FixtureSearch {
        results: SEASONS_PAGES
            .iter()
            .enumerate()
            .map(|(i, (url, _))| SearchResult {
                title: format!("Seasons article {}", i + 1),
                url: url.to_string(),
                snippet: "Why the seasons change".to_string(),
                published_at: None,
                provider: "fixture".to_string(),
                rank: i + 1,
            })
            .collect(),
        calls: AtomicU64::new(0),
    })
}

fn seasons_request(root: &Path) -> ResearchRequest {
    ResearchRequest {
        question: "Why do the Earth's seasons change?".to_string(),
        mode: ResearchMode::Online,
        project_root: Some(root.to_path_buf()),
        provided_urls: vec![],
        use_browser: false,
        internet_enabled: true,
    }
}

fn seasons_plan_llm() -> MockLlmClient {
    let llm = MockLlmClient::new("test-model");
    llm.with_response(
        "Return the research plan as JSON",
        LlmResponse {
            content: serde_json::json!({
                "steps": [{
                    "question": "Why do the Earth's seasons change?",
                    "source": "internet",
                    "search_queries": ["why do the seasons change on Earth"],
                    "answer_criteria": "Explains axial tilt as the cause of seasons"
                }]
            })
            .to_string(),
            prompt_tokens: 10,
            completion_tokens: 10,
        },
    );
    llm
}

/// Drain every event from the receiver while the run executes.
async fn collect_events(
    rx: &mut tokio::sync::mpsc::Receiver<ResearchEvent>,
    handle: tokio::task::JoinHandle<localaise_lib::research::ResearchResult<i64>>,
) -> (Vec<&'static str>, localaise_lib::research::ResearchResult<i64>) {
    let mut kinds = Vec::new();
    while let Some(ev) = rx.recv().await {
        kinds.push(match ev {
            ResearchEvent::Planned { .. } => "planned",
            ResearchEvent::Searching { .. } => "searching",
            ResearchEvent::Fetched { .. } => "fetched",
            ResearchEvent::FactExtracted { .. } => "fact",
            ResearchEvent::GapFound { .. } => "gap",
            ResearchEvent::SectionWritten { .. } => "section",
            ResearchEvent::Verified { .. } => "verified",
            ResearchEvent::Notice { .. } => "notice",
            ResearchEvent::Done { .. } => "done",
            ResearchEvent::Error { .. } => "error",
        });
    }
    let result = handle.await.expect("orchestrator task joins");
    (kinds, result)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn online_run_goes_from_plan_to_done_against_fixtures() {
    let pool = Pool::shared();
    let root = pool.checkout_project("online");

    let net = ResearchNet::with_client(root.clone(), true, Arc::clone(&pool.seasons) as Arc<dyn HttpClient>)
        .with_search_provider(seasons_search());
    let llm = seasons_plan_llm();

    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let request = seasons_request(&root);
    let handle =
        tokio::spawn(async move { orchestrator::run(llm, request, &net, tx, Budgets::quick(), None).await });

    let (kinds, result) = collect_events(&mut rx, handle).await;
    assert!(result.is_ok(), "run failed: {result:?}");

    // Planned first, Done last; a search, at least one fetch, and verification ran.
    assert_eq!(kinds.first(), Some(&"planned"), "events: {kinds:?}");
    assert_eq!(kinds.last(), Some(&"done"), "events: {kinds:?}");
    assert!(kinds.contains(&"searching"), "must search: {kinds:?}");
    assert!(kinds.contains(&"fetched"), "must fetch: {kinds:?}");
    assert!(kinds.contains(&"verified"), "must verify: {kinds:?}");
    assert!(kinds.contains(&"section"), "must write a section: {kinds:?}");

    // The fixture client served the pages (the run never touched the network).
    assert!(pool.seasons_arc().request_count() >= 1);

    // Files left behind: transcript, export payload and the markdown report.
    let chats = root.join("chats");
    let transcript_path = chats.join("research-001.json");
    assert!(transcript_path.exists(), "transcript missing at {transcript_path:?}");
    let export_payload = chats.join("research-001.export.json");
    assert!(export_payload.exists(), "export payload missing at {export_payload:?}");
    let payload = transcript::load_export_payload(&chats, "research-001").expect("payload loads");
    assert!(!payload.sections.is_empty(), "payload carries the written sections");
    assert!(!payload.sources.is_empty(), "payload carries the sources");
    let outputs = root.join("outputs");
    let report_written = std::fs::read_dir(&outputs)
        .map(|d| d.flatten().any(|e| e.file_name().to_string_lossy().ends_with(".md")))
        .unwrap_or(false);
    assert!(report_written, "a markdown report should exist under outputs/");

    pool.release_project(&root);
}

#[tokio::test]
async fn provided_urls_land_in_the_provided_sources_section() {
    let pool = Pool::shared();
    let root = pool.checkout_project("provided");

    let net = ResearchNet::with_client(root.clone(), true, Arc::clone(&pool.provided) as Arc<dyn HttpClient>)
        .with_search_provider(seasons_search());
    let llm = seasons_plan_llm();

    let mut request = seasons_request(&root);
    request.provided_urls = vec!["https://1.1.1.1/research/provided/press-release".to_string()];

    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let handle =
        tokio::spawn(async move { orchestrator::run(llm, request, &net, tx, Budgets::quick(), None).await });

    let mut provided_section = false;
    let mut done = false;
    while let Some(ev) = rx.recv().await {
        match ev {
            ResearchEvent::SectionWritten { title, .. } if title == "Provided sources" => {
                provided_section = true;
            }
            ResearchEvent::Done { .. } => done = true,
            _ => {}
        }
    }
    let result = handle.await.expect("orchestrator task joins");
    assert!(result.is_ok(), "run failed: {result:?}");
    assert!(done, "run must finish");
    assert!(provided_section, "the provided-URLs section must be written");

    // The export payload includes the provided section as well.
    let payload = transcript::load_export_payload(&root.join("chats"), "research-001").expect("payload loads");
    assert!(
        payload.sections.iter().any(|s| s.title == "Provided sources"),
        "export payload should contain the provided-sources section"
    );

    pool.release_project(&root);
}

#[tokio::test]
async fn offline_mode_never_touches_the_network() {
    let pool = Pool::shared();
    let root = pool.checkout_project("offline");

    // A client that fails on every call: if the pipeline tries to fetch, that's a bug.
    struct DeadClient;
    #[async_trait]
    impl HttpClient for DeadClient {
        async fn get(&self, url: &str, _h: HeaderMap) -> AppResult<HttpResponse> {
            Err(AppError::msg(format!("offline test: unexpected fetch of {url}")))
        }
        async fn get_with_redirect_policy(
            &self,
            url: &str,
            h: HeaderMap,
            _m: usize,
            _c: Box<dyn Fn(String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> + Send + Sync>,
        ) -> AppResult<HttpResponse> {
            self.get(url, h).await
        }
    }

    let search = FixtureSearch { results: vec![], calls: AtomicU64::new(0) };
    let net = ResearchNet::with_client(root.clone(), false, Arc::new(DeadClient) as Arc<dyn HttpClient>)
        .with_search_provider(Arc::new(search));

    // The planner is told to use internet steps; the pipeline must coerce them
    // to local because the project has the internet toggle off.
    let llm = seasons_plan_llm();
    let mut request = seasons_request(&root);
    request.mode = ResearchMode::Offline;
    request.internet_enabled = false;

    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let handle =
        tokio::spawn(async move { orchestrator::run(llm, request, &net, tx, Budgets::quick(), None).await });

    let mut searches = 0usize;
    let mut fetches = 0usize;
    let mut done = false;
    while let Some(ev) = rx.recv().await {
        match ev {
            ResearchEvent::Searching { .. } => searches += 1,
            ResearchEvent::Fetched { .. } => fetches += 1,
            ResearchEvent::Done { .. } => done = true,
            _ => {}
        }
    }
    let result = handle.await.expect("orchestrator task joins");
    assert!(result.is_ok(), "run failed: {result:?}");
    assert!(done, "run must finish");
    assert_eq!(searches, 0, "no search may happen with the internet toggle off");
    assert_eq!(fetches, 0, "no fetch may happen with the internet toggle off");

    pool.release_project(&root);
}

#[tokio::test]
async fn export_payload_renders_every_format() {
    let pool = Pool::shared();
    let root = pool.checkout_project("export");

    let net = ResearchNet::with_client(root.clone(), true, Arc::clone(&pool.seasons) as Arc<dyn HttpClient>)
        .with_search_provider(seasons_search());
    let llm = seasons_plan_llm();

    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let request = seasons_request(&root);
    let handle =
        tokio::spawn(async move { orchestrator::run(llm, request, &net, tx, Budgets::quick(), None).await });
    while rx.recv().await.is_some() {}
    let result = handle.await.expect("orchestrator task joins");
    assert!(result.is_ok(), "run failed: {result:?}");

    let payload = transcript::load_export_payload(&root.join("chats"), "research-001").expect("payload loads");
    let options = ExportOptions::default();
    for format in [
        ExportFormat::Markdown,
        ExportFormat::Json,
        ExportFormat::Html,
        ExportFormat::Notion,
        ExportFormat::Text,
        ExportFormat::CslJson,
    ] {
        let opts = ExportOptions { format, ..options.clone() };
        let out = export_report(&payload.sections, &payload.sources, opts)
            .unwrap_or_else(|e| panic!("{format:?} export failed: {e}"));
        assert!(!out.trim().is_empty(), "{format:?} export must not be empty");
    }

    pool.release_project(&root);
}

#[tokio::test]
async fn cancel_before_the_run_yields_an_empty_partial_report() {
    let pool = Pool::shared();
    let root = pool.checkout_project("cancel");

    let net = ResearchNet::with_client(root.clone(), true, Arc::clone(&pool.seasons) as Arc<dyn HttpClient>)
        .with_search_provider(seasons_search());
    let llm = seasons_plan_llm();

    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let request = seasons_request(&root);
    let handle =
        tokio::spawn(async move { orchestrator::run(llm, request, &net, tx, Budgets::quick(), Some(cancel)).await });

    let mut fetches = 0usize;
    let mut searches = 0usize;
    let mut done = false;
    while let Some(ev) = rx.recv().await {
        match ev {
            ResearchEvent::Fetched { .. } => fetches += 1,
            ResearchEvent::Searching { .. } => searches += 1,
            ResearchEvent::Done { .. } => done = true,
            _ => {}
        }
    }
    let result = handle.await.expect("orchestrator task joins");
    assert!(result.is_ok(), "run failed: {result:?}");
    assert!(done, "a cancelled run still writes a (partial) report");
    assert_eq!(fetches, 0, "no page may be fetched after cancellation");
    assert_eq!(searches, 0, "no search may run after cancellation");

    pool.release_project(&root);
}
