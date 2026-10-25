# RAG / Deep Research Plan — LocalAIse

## Overview
This document captures the complete end-to-end process for the local-first deep research
pipeline integrated into LocalAIse, including backend crates, data flow, UI contract, and
build order. No code is changed by this document; it is a planning reference.

## Core design principle
Small models fail at long-horizon autonomy (free-form ReAct loops, long context, many tools).
Code owns the control flow; the model performs only small, narrow, schema-constrained jobs.
Think "deterministic pipeline with LLM function calls," not "autonomous agent."

## High-level architecture
```
User query -> Planner (LLM) -> sub-questions (JSON)
        -> Orchestrator (plain code, state machine + budgets)
        -> Search / Fetch / Browser (SearXNG, httpx+trafilatura, Playwright/Chromium)
        -> Clean -> Chunk -> Embed
        -> Evidence Store (SQLite + sqlite-vec / Chroma)
        -> Reader (LLM, per chunk) -> facts + quotes
        -> Gap Checker (LLM) -> new queries (loop)
        -> Writer (LLM, per section) -> top-k facts only
        -> Verifier (NLI + LLM) -> citation check
```

## Component choices
- Inference runtime: llama.cpp server (GBNF/JSON-schema constrained output) or Ollama structured outputs; vLLM for GPU batching.
- Models: Qwen3-4B (planner/gap/writer), Phi-4-mini 3.8B, Gemma 3 4B, Llama 3.2 3B. Reader uses a faster 3B in parallel.
- Embeddings/reranking: bge-small/base or nomic-embed + bge-reranker cross-encoder.
- Search: self-hosted SearXNG (no API keys, JSON output); optional Wikipedia, arXiv, Semantic Scholar.
- Chromium: two tiers — Fetch (httpx + trafilatura) for 90% of cases; Browser (Playwright/CDP) for JS-rendered/thin content.
- Browser agent: compact numbered snapshot + tiny action set (click/type/scroll/extract/back/done), rules for cookie banners, caps at ~8 steps/page.

## Crate map (Rust)
- Async runtime/cancellation: tokio, tokio-util (CancellationToken)
- HTTP: reqwest (rustls)
- Readability: dom_smoothie (Readability.js port) or readability
- HTML->markdown: htmd
- HTML parsing/rules: scraper
- Chromium: chromiumoxide (tokio runtime feature)
- Storage: rusqlite (bundled) + sqlite-vec
- Embeddings/reranker: fastembed (bge-small, bge-reranker via ONNX)
- JSON schema: schemars + serde
- Streaming progress: tokio::sync::mpsc into UI bridge (Tauri events or SSE)
- Logging: tracing

## Module layout
```
research/
  mod.rs
  orchestrator.rs   // state machine, budgets, cancellation
  events.rs         // ResearchEvent (UI contract)
  llm.rs            // LlmClient + typed role calls
  roles/            // planner, query, reader, gap, writer, verifier
  search.rs         // SearXNG client
  fetch/
    mod.rs          // Fetcher trait + tiered fallback
    http.rs         // reqwest + dom_smoothie + htmd
    browser.rs      // chromiumoxide tier
    snapshot.rs     // numbered DOM snapshot for browser agent
  chunk.rs
  store.rs          // SQLite + sqlite-vec
  rank.rs           // embed + rerank
  verify.rs         // quote and citation checks
```

## UI contract (defined first)
```rust
#[derive(Serialize, Clone)]
#[serde(tag = "type")]
pub enum ResearchEvent {
    Planned { sub_questions: Vec<String> },
    Searching { query: String },
    Fetched { url: String, tier: FetchTier, chars: usize },
    FactExtracted { sub_q: usize, source_id: i64, claim: String },
    GapFound { missing: Vec<String> },
    SectionWritten { title: String, markdown: String },
    Verified { supported: usize, unsupported: usize },
    Done { report_id: i64 },
    Error { message: String },
}
```

## Typed, schema-constrained role calls
llama.cpp server accepts `json_schema` on `/completion` (and `response_format` on OpenAI route),
so the model cannot emit invalid JSON. Temperature ~0.2, n_predict ~512, cache_prompt true.

## Fetcher trait with tiered fallback
HttpFetcher first; if markdown length < 800 (thin/JS-rendered), fall back to BrowserFetcher.
Reuse one Browser instance; bound concurrency with Semaphore (2-3 tabs); block images/fonts/media.

## Verbatim-quote gate (anti-hallucination)
```rust
fn accept_fact(f: &RawFact, chunk: &str) -> bool {
    let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    f.quote.len() >= 20 && norm(chunk).contains(&norm(&f.quote))
}
```
Reject facts that fail; cite using code-injected IDs (`[S3]`), never model-invented.

## Parallel reader with bounded fan-out
```rust
let facts = stream::iter(chunks)
    .map(|c| async move { reader.extract(&sub_q, &c).await.map(|f| (c, f)) })
    .buffer_unordered(slots)   // = llama.cpp --parallel
    .filter_map(|r| async { r.ok() })
    .collect::<Vec<_>>().await;
```

## Orchestrator rules
- Thread `CancellationToken` through every await (UI stop button).
- Hard budgets: rounds, pages, tokens, wall-clock.
- Stop loop on novelty rate < ~10% new facts per round.

## Evidence store schema (SQLite)
```
sources(id, url, title, fetched_at, content_hash, quality_score)
chunks(id, source_id, text, embedding)
facts(id, subq_id, claim, quote, source_id, confidence)
questions(id, parent_id, text, status)  -- question tree, supports recursion
```

## Techniques for small models
| Problem | Mitigation |
|---|---|
| Invalid JSON/tool calls | Grammar/schema-constrained decoding |
| Context overflow / lost in the middle | Keep calls under ~3-4K tokens; retrieve top-k chunks |
| Hallucinated facts | Extractive-first: verbatim quote + source ID; reject non-substrings |
| Weak reasoning on long tasks | Decompose: one sub-question, one chunk, one job per call |
| Infinite loops | Hard budgets + novelty-rate stop |
| Poor writing over big evidence | Write per section from <=10 facts, then stitch |
| Citation errors | Cite as [S3] IDs injected by code; verifier checks each sentence via NLI |
| Duplicate content | Hash + embedding dedup before reader stage |
| Slow throughput | Batch reader calls (--parallel, vLLM batching); parallel async fetches |

## Optional upgrades
- Source quality scoring (domain heuristics, recency, rerank score).
- Contradiction detection (cluster facts, flag NLI disagreements).
- Disk cache of fetched pages for instant reruns.
- Reflection pass ("what would an expert challenge?") triggering one extra search round.
- Distillation: log traces, fine-tune 3B (LoRA) on planner/reader prompts.

## Build order
1. Search, fetch, clean, store (no LLM). Verify content quality.
2. Retrieval + reader with verbatim-quote check.
3. Single-round pipeline: plan -> search -> read -> write with citations.
4. Add gap-check loop and budgets.
5. Add Chromium tier and verifier.
6. Evaluate on 20-30 questions with known answers; tune prompts per role.

## Hardware defaults (until specified)
| Hardware | Model / quant | --parallel | ctx per slot |
|---|---|---|---|
| CPU only | 3-4B Q4_K_M | 1-2 | 3K |
| 6-8 GB GPU | 4B Q4/Q5 | 4 | 4K |
| 12 GB+ GPU | 4-5B Q6/Q8 | 4-8 | 4-6K |

Note: context per slot = ctx / parallel, so -c 16384 --parallel 4 gives 4K per slot,
matching the "keep every call under 3-4K tokens" rule.

## LocalAIse integration notes
- Research module: src-tauri/src/research/ (events, orchestrator, fetch, store, roles, chunk, rank).
- Tauri command: run_deep_research (emits "research:event" to frontend).
- React view: src/components/research/DeepResearchView.tsx renders the event list.
- Runtime: one llama.cpp server for phase 1; roles are prompt+schema pairs, not model swaps.
  Start with --parallel 4 for reader fan-out.
- LlmClient (roles) is stubbed pending wiring to the serving runtime.
- Browser tier: chromiumoxide, in-process, behind a Fetcher trait; uses user's Chromium/Chrome.
- Storage: in-memory EvidenceStore now; SqliteStore available for persistence.

## Status
- [x] Module scaffold + events + orchestrator (single round, stubs)
- [x] Tauri command run_deep_research + event forwarding
- [x] Http fetch (dom_smoothie + htmd)
- [x] Chromium browser tier
- [x] SQLite store (schema) + fastembed dependency
- [x] React event-list view
- [ ] Real LLM role calls (schema-constrained) wired to serving runtime
- [ ] Tiered fetcher fallback (http -> browser on thin content)
- [ ] Embeddings + reranker implementation
- [ ] sqlite-vec integration for vector search
- [ ] Gap loop with novelty-rate stop (currently stubbed)
- [ ] Section writer + verifier with NLI
- [ ] Budgets enforcement (wall-clock, tokens)
- [ ] Citation verification end-to-end
- [ ] Evaluation harness (20-30 questions)
