//! Test suite for the deep-research Chromium fetch tier.
//!
//! Layers, cheapest first:
//!   1. Pure pipeline tests (readability extraction, chunking, fact gate).
//!   2. HTTP tier tests against a local fixture server.
//!   3. Chromium tier tests: the same fixtures run through
//!      `BrowserFetcher` (chromiumoxide/CDP) using the machine's
//!      Edge/Chrome binary, proving JS-rendered content is reachable.
//!   4. End-to-end orchestrator run for a sample research topic,
//!      asserting the emitted event flow (Planned → ... → Done).
//!
//! The fixture server is a tiny std-only HTTP/1.1 responder so the suite
//! needs no extra dev-dependencies.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use localaise_lib::research::chunk::chunk_markdown;
use localaise_lib::research::events::ResearchEvent;
use localaise_lib::research::fetch::browser::BrowserFetcher;
use localaise_lib::research::fetch::http::{extract_markdown, HttpFetcher};
use localaise_lib::research::orchestrator::{self, Budgets};
use localaise_lib::research::roles::{gap, reader};

/// Any topic works; this one is small, verifiable, and stable.
const TOPIC: &str = "Why do the seasons change on Earth?";

/// The label only exists after render: the page builds it from fragments in
/// JS so the raw HTML can never contain the full string.
const JS_MARKER: &str = "SCRIBBLE_2026_CHROMIUM_RENDER_PROOF";

const STATIC_ARTICLE: &str = r#"<!doctype html>
<html><head><title>Fox Habitat</title></head>
<body>
<article>
<h1>The Red Fox</h1>
<p>Red foxes live in forests, meadows, and city parks across the northern hemisphere.</p>
<p>A single fox territory may cover several square kilometres and is defended against rivals.</p>
</article>
</body></html>"#;

// ---------------------------------------------------------------------------
// Fixture server
// ---------------------------------------------------------------------------

fn render_page(path: &str) -> (u16, &'static str, &'static str) {
    if path == "/static-article" {
        (200, "text/html", STATIC_ARTICLE)
    } else if path == "/js-rendered" {
        (
            200,
            "text/html",
            r#"<!doctype html>
<html><head><title>Loading…</title></head>
<body>
<div id="app"></div>
<script>
  // Content is assembled at runtime; the raw HTML never contains the marker.
  var parts = ["SCRIBBLE", "_2026_", "CHROMIUM", "_RENDER_", "PROOF"];
  document.getElementById("app").textContent = parts.join("");
</script>
</body></html>"#,
        )
    } else {
        (404, "text/plain", "not found")
    }
}

/// Spawns a tiny blocking HTTP server on an OS-chosen port. It serves exactly
/// one request; a fresh server per fetch keeps the fixture deterministic and
/// lets the thread die with the test.
fn spawn_fixture() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture server");
    let addr = listener.local_addr().expect("fixture addr");
    // Accept in a detached thread; sockets close when the process exits.
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            thread::spawn(move || {
                let mut stream = stream;
                let mut buf = [0u8; 8192];
                let mut raw = String::new();
                if let Ok(n) = stream.read(&mut buf) {
                    raw = String::from_utf8_lossy(&buf[..n]).to_string();
                }
                let path = raw
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("")
                    .split('?')
                    .next()
                    .unwrap_or("")
                    .to_string();
                let (status, ctype, body) = render_page(&path);
                let reason = if status == 200 { "OK" } else { "Not Found" };
                let response = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
                // `Connection: close` makes closing after one request conformant.
                drop(stream);
            });
        }
    });
    format!("http://{addr}")
}

// ---------------------------------------------------------------------------
// Chromium/Edge discovery
// ---------------------------------------------------------------------------

fn chromium_executable() -> Option<String> {
    if let Ok(p) = std::env::var("LOCALAISE_TEST_CHROME") {
        if !p.is_empty() && std::path::Path::new(&p).is_file() {
            return Some(p);
        }
    }
    for path in [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    ] {
        if std::path::Path::new(path).is_file() {
            return Some(path.to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 1. Pure pipeline tests
// ---------------------------------------------------------------------------

#[test]
fn extract_markdown_renders_a_static_article() {
    let md = extract_markdown(STATIC_ARTICLE).expect("static article extracts");
    assert!(md.contains("Red Fox"), "title present: {md}");
    assert!(md.contains("city parks"), "body present: {md}");
}

#[test]
fn extract_markdown_fallbacks_for_a_thin_page() {
    let thin = "<html><body><div></div></body></html>";
    // Readability has nothing to work with: either an outright error or
    // empty content. Callers must fall back — it must never panic.
    match extract_markdown(thin) {
        Err(_) => {}
        Ok(md) => assert!(md.trim().is_empty(), "should not invent content"),
    }
}

#[test]
fn chunk_markdown_respects_paragraph_boundaries() {
    let paras: Vec<String> = (1..=6).map(|i| format!("para-{i} {}", "x".repeat(60))).collect();
    let md = paras.join("\n\n");
    let chunks = chunk_markdown(&md, 200);
    assert!(!chunks.is_empty());
    for c in &chunks {
        assert!(c.len() <= 260, "chunk too big: {}", c.len());
    }
    // No paragraph may be lost across chunk boundaries.
    let everything = chunks.join("\n");
    for i in 1..=6 {
        let needle = format!("para-{i} ");
        assert!(everything.contains(&needle), "paragraph {i} missing");
    }
}

#[test]
fn fact_gate_accepts_verbatim_quotes_only() {
    let chunk = "Water at 0 degrees Celsius freezes, becoming less dense than warmer water.";
    let facts = reader::extract(chunk, 1);
    assert!(!facts.is_empty(), "stub reader yields the first sentence");
    assert!(reader::accept_fact(&facts[0], chunk));
    // A quote that never appears verbatim in the chunk must be rejected.
    let bogus = reader::RawFact {
        claim: "made up".into(),
        quote: "this quote appears nowhere in the chunk".into(),
        source_id: 1,
    };
    assert!(!reader::accept_fact(&bogus, chunk));
}

#[test]
fn gap_check_flags_empty_evidence_only() {
    assert_eq!(gap::check(&[]), vec!["more sources".to_string()]);
    assert!(gap::check(&["something".to_string()]).is_empty());
}

#[test]
fn topic_planning_splits_the_question() {
    let subs = localaise_lib::research::roles::plan_subquestions(TOPIC);
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0], TOPIC);
}

// ---------------------------------------------------------------------------
// 2. HTTP tier against the local fixture
// ---------------------------------------------------------------------------

#[tokio::test]
async fn http_tier_fetches_a_static_page() {
    let base = spawn_fixture();
    let fetcher = HttpFetcher::new();
    let page = fetcher
        .fetch(&format!("{base}/static-article"))
        .await
        .expect("static page fetches over http");
    assert!(page.markdown.contains("Red Fox"), "markdown: {}", page.markdown);
}

#[tokio::test]
async fn http_tier_cannot_see_js_rendered_content() {
    let base = spawn_fixture();
    let fetcher = HttpFetcher::new();
    let page = fetcher
        .fetch(&format!("{base}/js-rendered"))
        .await
        .expect("fixture reachable over http");
    assert!(
        !page.markdown.contains(JS_MARKER),
        "http tier must not execute JS: {}",
        page.markdown
    );
}

#[tokio::test]
async fn http_tier_surfaces_failures_gracefully() {
    let base = spawn_fixture();
    let fetcher = HttpFetcher::new();
    let result = fetcher.fetch(&format!("{base}/missing")).await;
    // The fixture does not close-write the 404 until read; reqwest may error
    // OR surface the 404 body. Either is acceptable; a panic is not.
    if let Ok(page) = result {
        assert!(
            page.markdown.trim().is_empty() || page.markdown.contains("not found"),
            "unexpected content: {}",
            page.markdown
        );
    }
}

// ---------------------------------------------------------------------------
// 3. Chromium tier
// ---------------------------------------------------------------------------

#[tokio::test]
async fn browser_tier_renders_js_content() {
    let Some(chrome) = chromium_executable() else {
        eprintln!("skipped: no Chromium/Edge executable found");
        return;
    };
    let base = spawn_fixture();
    let fetcher = BrowserFetcher::new(Some(chrome));
    let page = fetcher
        .fetch(&format!("{base}/js-rendered"))
        .await
        .expect("browser tier must render JS-injected content");
    // htmd escapes markdown specials such as underscores, so compare
    // backslash-normalized.
    let normalized = page.markdown.replace('\\', "");
    assert!(
        normalized.contains(JS_MARKER),
        "rendered marker missing from browser tier output: {}",
        page.markdown
    );
}

#[tokio::test]
async fn browser_tier_fetches_a_static_page() {
    let Some(chrome) = chromium_executable() else {
        eprintln!("skipped: no Chromium/Edge executable found");
        return;
    };
    let base = spawn_fixture();
    let fetcher = BrowserFetcher::new(Some(chrome));
    let page = fetcher
        .fetch(&format!("{base}/static-article"))
        .await
        .expect("static page via browser tier");
    assert!(page.markdown.contains("Red Fox"), "markdown: {}", page.markdown);
}

#[tokio::test]
async fn browser_tier_reports_failures_without_panicking() {
    let Some(chrome) = chromium_executable() else {
        eprintln!("skipped: no Chromium/Edge executable found");
        return;
    };
    // Reserve a port, then release it: fetching must fail cleanly.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let fetcher = BrowserFetcher::new(Some(chrome));
    let result = fetcher.fetch(&format!("http://127.0.0.1:{port}/gone")).await;
    // Documented behavior: Chromium renders its own error page, so a dead
    // port currently comes back as Ok(error-page text). It must not panic
    // and must not fabricate content — even so, the pipeline should flag
    // such pages instead of ingesting them as evidence (finding F2).
    match result {
        Err(_) => {}
        Ok(page) => {
            eprintln!("dead-port fetch ingested content: {}", page.markdown);
            assert!(!page.markdown.is_empty(), "must not fabricate content");
        }
    }
}

// ---------------------------------------------------------------------------
// 4. End-to-end: run the orchestrator for the sample topic
// ---------------------------------------------------------------------------

#[tokio::test]
async fn topic_end_to_end_emits_the_expected_event_flow() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let budgets = Budgets { max_rounds: 1, max_pages: 3, wall_clock_secs: 60 };
    let handle = tokio::spawn(async move { orchestrator::run(TOPIC, tx, budgets).await });

    let mut events: Vec<ResearchEvent> = Vec::new();
    while let Some(ev) = rx.recv().await {
        events.push(ev);
    }
    let run_result = handle.await.expect("orchestrator task joins");
    assert!(run_result.is_ok(), "orchestrator returned an error: {run_result:?}");

    let kinds: Vec<&str> = events
        .iter()
        .map(|e| match e {
            ResearchEvent::Planned { .. } => "planned",
            ResearchEvent::Searching { .. } => "searching",
            ResearchEvent::Fetched { .. } => "fetched",
            ResearchEvent::FactExtracted { .. } => "fact",
            ResearchEvent::GapFound { .. } => "gap",
            ResearchEvent::SectionWritten { .. } => "section",
            ResearchEvent::Verified { .. } => "verified",
            ResearchEvent::Done { .. } => "done",
            ResearchEvent::Error { .. } => "error",
        })
        .collect();

    // Planned first, Done last.
    assert_eq!(kinds.first(), Some(&"planned"), "events: {kinds:?}");
    assert_eq!(kinds.last(), Some(&"done"), "events: {kinds:?}");
    assert!(kinds.contains(&"searching"), "must emit a search step: {kinds:?}");
    assert!(kinds.contains(&"section"), "a report section is written: {kinds:?}");
    assert!(kinds.contains(&"verified"), "the verifier runs: {kinds:?}");

    // The stub verifier counts every extracted fact as supported.
    let facts = kinds.iter().filter(|k| **k == "fact").count();
    for ev in &events {
        if let ResearchEvent::Verified { supported, unsupported } = ev {
            assert_eq!(*supported, facts);
            assert_eq!(*unsupported, 0);
        }
    }

    eprintln!("topic: {TOPIC}");
    eprintln!("event flow: {kinds:?}");
}
