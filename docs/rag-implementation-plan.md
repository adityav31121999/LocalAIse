# RAG Implementation Plan: Embeddings + Reranker (Rust)

**Based on:** The 10-phase roadmap provided, adapted for LocalAIse codebase
**Target Model:** IBM Granite Embedding R2 97M (multilingual, ~384 dims)
**Hardware:** Mixed CPU/GPU (ONNX Runtime with auto EP selection)
**Storage:** SQLite + sqlite-vec + FTS5 (migrate from JSON)

---

## Current State Assessment

| Component | Current | Target |
|-----------|---------|--------|
| Chunking | Char-based, word-boundary | Structure-aware (MD headings, code blocks, tables) |
| Embeddings | Stub (128-d zeros) | Granite R2 97M via ONNX (fastembed) |
| Vector Store | JSON file (`index.json`) | SQLite + sqlite-vec + FTS5 |
| Retrieval | Keyword/BM25 only | Hybrid: dense (ANN) + BM25 → RRF → Rerank |
| Reranker | None | bge-reranker-base (cross-encoder) |
| Eval | Basic script | Full harness: Recall@k, MRR, nDCG, answer support |

---

## Phase 0: Interfaces & Decisions (Week 1)

### 0.1 Define Traits (New Module: `src-tauri/src/embeddings/`)

```rust
// src-tauri/src/embeddings/traits.rs
use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmbedError {
    #[error("Model load failed: {0}")] ModelLoad(String),
    #[error("Inference failed: {0}")] Inference(String),
    #[error("Tokenization failed: {0}")] Tokenize(String),
    #[error("Dimension mismatch: expected {expected}, got {actual}")] DimMismatch { expected: usize, actual: usize },
}

pub type Result<T> = std::result::Result<T, EmbedError>;

#[async_trait]
pub trait Embedder: Send + Sync {
    fn model_id(&self) -> &str;
    fn dim(&self) -> usize;
    fn max_seq_len(&self) -> usize;
    async fn embed_docs(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    async fn embed_query(&self, q: &str) -> Result<Vec<f32>>;
}

#[async_trait]
pub trait Reranker: Send + Sync {
    fn model_id(&self) -> &str;
    async fn rerank(&self, query: &str, candidates: &[RerankCandidate], top_n: usize) -> Result<Vec<ScoredCandidate>>;
}

#[derive(Debug, Clone)]
pub struct RerankCandidate {
    pub chunk_id: i64,
    pub text: String,
    pub metadata: CandidateMeta,
}

#[derive(Debug, Clone)]
pub struct CandidateMeta {
    pub source_id: i64,
    pub heading_path: String,
    pub chunk_index: usize,
}

#[derive(Debug, Clone)]
pub struct ScoredCandidate {
    pub candidate: RerankCandidate,
    pub score: f32,
}
```

### 0.2 Model Decisions (Lock In)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Embedding Model | `ibm-granite/granite-embedding-107m-multilingual` | 97M params, multilingual, 384d, Apache-2.0 |
| Reranker Model | `BAAI/bge-reranker-v2-m3` | Multilingual, strong quality, ~300M params |
| ONNX Runtime | `ort` crate (direct) | More control than fastembed; supports GPU EPs |
| Tokenizer | `tokenizers` crate (HF tokenizers) | Matches model exactly |
| Vector Store | `sqlite-vec` + `sqlite` (rusqlite) | Embedded, no server, FTS5 built-in |

### 0.3 Exit Criteria
- [ ] Traits compile
- [ ] Mock implementations pass through orchestrator
- [ ] Model download/verify logic works

---

## Phase 1: Eval Harness (Week 1-2, Parallel)

### 1.1 Golden Set Creation
- **Target**: 50-100 (query, relevant_chunk_ids) pairs
- **Bootstrap**: Use existing 4B model to generate questions from indexed chunks → hand-prune
- **Format**: JSONL with `query`, `relevant_ids`, `domain`, `difficulty`

### 1.2 Metrics Implementation
```rust
// src-tauri/src/eval/metrics.rs
pub struct EvalMetrics {
    pub recall_at_5: f32,
    pub recall_at_20: f32,
    pub recall_at_50: f32,
    pub mrr: f32,
    pub ndcg_at_10: f32,
    pub answer_support_rate: f32,  // Verifier-based
    pub latency_embed_ms: f64,
    pub latency_search_ms: f64,
    pub latency_rerank_ms: f64,
}
```

### 1.3 Binary: `cargo run --bin rag_eval`
- Outputs: Table + JSON for diffing runs
- Runs against current retrieval pipeline

### 1.4 Exit Criteria
- [ ] Baseline numbers exist before any tuning
- [ ] Can run `cargo run --bin rag_eval` and get reproducible results

---

## Phase 2: Ingestion & Chunking Overhaul (Week 2-3)

### 2.1 Structure-Aware Chunking (Replace `ingest/chunk.rs`)

```rust
// New: src-tauri/src/ingest/semantic_chunk.rs
use crate::ingest::parsers::ParsedDocument;

pub struct SemanticChunk {
    pub content: String,           // Raw text for display/quote checking
    pub embed_text: String,        // With contextual header: "Title > H1 > H2"
    pub heading_path: Vec<String>, // ["Title", "H1", "H2"]
    pub chunk_index: usize,
    pub token_count: usize,
    pub char_span: (usize, usize), // For verbatim quote verification
}

pub fn chunk_document(
    doc: &ParsedDocument,
    target_tokens: usize,      // 300-500
    overlap_pct: f32,          // 10-15%
    tokenizer: &Tokenizer,
) -> Vec<SemanticChunk> {
    // 1. Split on Markdown headings (preserve hierarchy)
    // 2. Split paragraphs, then sentences
    // 3. Never split code blocks or tables mid-way
    // 4. Prepend contextual header to embed_text only
    // 5. Count tokens with embedding model's tokenizer
}
```

### 2.2 Deduplication
- **Exact**: SHA-256 content hash
- **Near-duplicate**: Embedding cosine > 0.95 (after Phase 3)

### 2.3 Metadata Schema (for SQLite)
```sql
-- Chunk metadata (stored in chunks table)
source_id, session_id, heading_path, chunk_index, 
language, content_hash, embed_model, token_count, char_start, char_end
```

### 2.4 Exit Criteria
- [ ] Same page ingested twice → no new rows
- [ ] Chunk size distribution: P50 ~400 tokens, max < 600
- [ ] Code blocks/tables never split
- [ ] Contextual headers present in `embed_text`

---

## Phase 3: Embedding Service (Week 3-4)

### 3.1 ONNX Model Loader (`src-tauri/src/embeddings/ort_embedder.rs`)

```rust
use ort::{GraphOptimizationLevel, Session, ExecutionProvider};
use tokenizers::Tokenizer;

pub struct OrtEmbedder {
    session: Arc<Session>,
    tokenizer: Arc<Tokenizer>,
    model_id: String,
    dim: usize,
    max_seq_len: usize,
    // Query/doc prefixes for Granite (verify from model card)
    query_prefix: String,
    doc_prefix: String,
}

impl OrtEmbedder {
    pub async fn new(model_dir: &Path, prefer_gpu: bool) -> Result<Self> {
        // 1. Download model if missing (with checksum)
        // 2. Load tokenizer (tokenizer.json)
        // 3. Create ONNX session with EP selection:
        //    - CUDA → DirectML → CPU (Windows)
        //    - CoreML → CPU (macOS)
        //    - CUDA → CPU (Linux)
        // 4. Set intra_op_num_threads = max(1, logical_cores / 4)
        // 5. Warm up with dummy input
    }
}

#[async_trait]
impl Embedder for OrtEmbedder {
    async fn embed_docs(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        // Batch 32-64, sort by length, pad, run, L2-normalize
        // Apply doc_prefix to each
    }
    
    async fn embed_query(&self, q: &str) -> Result<Vec<f32>> {
        // Single or small batch, apply query_prefix
    }
}
```

### 3.2 Worker Thread Pattern (Non-blocking)
```rust
// src-tauri/src/embeddings/worker.rs
use tokio::sync::mpsc;
use std::sync::Arc;

struct EmbedRequest {
    texts: Vec<String>,
    is_query: bool,
    respond: oneshot::Sender<Result<Vec<Vec<f32>>>>,
}

pub struct EmbeddingWorker {
    tx: mpsc::Sender<EmbedRequest>,
}

impl EmbeddingWorker {
    pub fn start(embedder: Arc<dyn Embedder>) -> Self {
        let (tx, mut rx) = mpsc::channel(32);
        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::current();
            while let Some(req) = rx.blocking_recv() {
                let result = if req.is_query {
                    rt.block_on(embedder.embed_query(&req.texts[0]))
                } else {
                    rt.block_on(embedder.embed_docs(&req.texts))
                };
                let _ = req.respond.send(result);
            }
        });
        Self { tx }
    }
    
    pub async fn embed_docs(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(EmbedRequest { texts, is_query: false, respond: tx }).await?;
        rx.await?
    }
}
```

### 3.3 Caching Layer
```rust
// src-tauri/src/embeddings/cache.rs
use blake3::Hash;

pub struct EmbeddingCache {
    db: Arc<rusqlite::Connection>, // Separate cache DB
}

impl EmbeddingCache {
    pub fn get(&self, model_id: &str, content_hash: Hash) -> Option<Vec<f32>>;
    pub fn put(&self, model_id: &str, content_hash: Hash, vec: &[f32]);
}
```

### 3.4 Model Download Manager
- Progress events via `ResearchEvent::ModelDownload { model_id, progress, total }`
- Offline-first: check local cache first
- SHA-256 verification

### 3.5 Exit Criteria
- [ ] Throughput measured (chunks/sec) at batch=32
- [ ] No UI stalls during LLM generation (worker thread verified)
- [ ] Query/doc prefixes correct (validated against model card)
- [ ] Cache hit rate > 90% on re-ingest

---

## Phase 4: Storage & Hybrid Index (Week 4-5)

### 4.1 SQLite Schema Migration

```sql
-- src-tauri/src/index/schema_v2.sql

-- Main chunks table
CREATE TABLE chunks (
    id INTEGER PRIMARY KEY,
    source_id INTEGER NOT NULL,
    session_id INTEGER,
    content TEXT NOT NULL,           -- Raw text for display/quotes
    embed_text TEXT NOT NULL,        -- With contextual header
    heading_path TEXT,               -- JSON array ["Title", "H1"]
    chunk_index INTEGER NOT NULL,
    token_count INTEGER NOT NULL,
    char_start INTEGER NOT NULL,
    char_end INTEGER NOT NULL,
    content_hash BLOB NOT NULL,      -- blake3 32 bytes
    embed_model TEXT NOT NULL,       -- e.g., "granite-embedding-107m-multilingual"
    language TEXT,
    created_at INTEGER DEFAULT (strftime('%s','now')),
    UNIQUE(source_id, chunk_index)
);

-- Vector index (sqlite-vec)
CREATE VIRTUAL TABLE chunk_vec USING vec0(
    embedding float[384],  -- Match model dim
    chunk_id INTEGER       -- References chunks.id
);

-- Full-text search (FTS5)
CREATE VIRTUAL TABLE chunk_fts USING fts5(
    content, 
    heading_path,
    content='chunks',
    content_rowid='id',
    tokenize='unicode61'
);

-- Triggers to keep FTS in sync
CREATE TRIGGER chunks_ai AFTER INSERT ON chunks BEGIN
    INSERT INTO chunk_fts(rowid, content, heading_path) 
    VALUES (new.id, new.content, new.heading_path);
END;

CREATE TRIGGER chunks_ad AFTER DELETE ON chunks BEGIN
    INSERT INTO chunk_fts(chunk_fts, rowid, content, heading_path) 
    VALUES ('delete', old.id, old.content, old.heading_path);
END;

CREATE TRIGGER chunks_au AFTER UPDATE ON chunks BEGIN
    INSERT INTO chunk_fts(chunk_fts, rowid, content, heading_path) 
    VALUES ('delete', old.id, old.content, old.heading_path);
    INSERT INTO chunk_fts(rowid, content, heading_path) 
    VALUES (new.id, new.content, new.heading_path);
END;

-- Indexes
CREATE INDEX idx_chunks_source ON chunks(source_id);
CREATE INDEX idx_chunks_session ON chunks(session_id);
CREATE INDEX idx_chunks_model ON chunks(embed_model);
CREATE INDEX idx_chunks_hash ON chunks(content_hash);
```

### 4.2 Migration Strategy
```rust
// src-tauri/src/index/migration.rs
pub async fn migrate_v1_to_v2(project_dir: &Path) -> Result<()> {
    // 1. Read old index.json
    // 2. Create new SQLite DB
    // 3. For each chunk:
    //    - Compute content_hash (blake3)
    //    - Generate embed_text with contextual header (from file structure)
    //    - Token count with embedding tokenizer
    //    - Insert into chunks table
    //    - Generate embedding → insert into chunk_vec
    // 4. Verify row counts match
    // 5. Backup old index.json → index.json.v1.bak
}
```

### 4.3 Background Re-embedding Job
```rust
// For model upgrades: find rows where embed_model != current_model
// Re-embed in batches, update chunk_vec
```

### 4.4 Exit Criteria
- [ ] Dense search (top-k by cosine) works
- [ ] BM25 search (FTS5) works
- [ ] Filtered queries (by session_id, source_id) work
- [ ] Migration from JSON → SQLite is lossless
- [ ] All covered by integration tests

---

## Phase 5: Hybrid Retrieval + Fusion (Week 5-6)

### 5.1 Retrieval Pipeline
```rust
// src-tauri/src/retrieval/pipeline.rs

pub struct RetrievalPipeline {
    embedder: Arc<dyn Embedder>,
    vector_store: Arc<dyn VectorStore>,
    fts_store: Arc<dyn FtsStore>,
    reranker: Option<Arc<dyn Reranker>>,
    config: RetrievalConfig,
}

pub struct RetrievalConfig {
    pub dense_top_k: usize,      // 50
    pub bm25_top_k: usize,       // 50
    pub fused_pool_size: usize,  // 40
    pub rrf_k: f32,              // 60.0
    pub final_top_k: usize,      // 8 (after rerank)
    pub mmr_lambda: f32,         // 0.5 for diversity
    pub per_source_cap: usize,   // 2
}

impl RetrievalPipeline {
    pub async fn retrieve(&self, query: &str, scope: SearchScope) -> Result<Vec<Evidence>> {
        // 1. Embed query
        let qvec = self.embedder.embed_query(query).await?;
        
        // 2. Dense search (cosine similarity via sqlite-vec)
        let dense_ids = self.vector_store.search_dense(&qvec, self.config.dense_top_k, scope).await?;
        
        // 3. BM25 search (FTS5)
        let bm25_ids = self.fts_store.search_bm25(query, self.config.bm25_top_k, scope).await?;
        
        // 4. RRF Fusion
        let fused = rrf(&[dense_ids, bm25_ids], self.config.rrf_k);
        let pool = fused.into_iter().take(self.config.fused_pool_size).collect::<Vec<_>>();
        
        // 5. Fetch candidate texts
        let candidates = self.vector_store.fetch_candidates(&pool).await?;
        
        // 6. Rerank (if enabled)
        let reranked = if let Some(reranker) = &self.reranker {
            reranker.rerank(query, &candidates, self.config.final_top_k * 2).await?
        } else {
            candidates.into_iter().map(|c| ScoredCandidate { candidate: c, score: 0.0 }).collect()
        };
        
        // 7. MMR + Per-source cap
        let final_candidates = apply_mmr_and_cap(reranked, self.config);
        
        // 8. Convert to Evidence with quote spans
        Ok(final_candidates.into_iter().map(|sc| Evidence {
            chunk_id: sc.candidate.chunk_id,
            source_id: sc.candidate.metadata.source_id,
            quote_span: (sc.candidate.metadata.char_start, sc.candidate.metadata.char_end),
            score: sc.score,
        }).collect())
    }
}
```

### 5.2 RRF Implementation
```rust
fn rrf(lists: &[Vec<i64>], k: f32) -> Vec<(i64, f32)> {
    use std::collections::HashMap;
    let mut scores: HashMap<i64, f32> = HashMap::new();
    for list in lists {
        for (rank, &id) in list.iter().enumerate() {
            *scores.entry(id).or_default() += 1.0 / (k + rank as f32 + 1.0);
        }
    }
    let mut v: Vec<_> = scores.into_iter().collect();
    v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    v
}
```

### 5.3 Exit Criteria
- [ ] Hybrid Recall@50 > Dense-only Recall@50 (measured by eval harness)
- [ ] RRF fusion produces sensible ordering
- [ ] Latency: < 200ms for dense + BM25 + fusion (before rerank)

---

## Phase 6: Reranker (Week 6-7)

### 6.1 Cross-Encoder Implementation (`src-tauri/src/embeddings/ort_reranker.rs`)

```rust
pub struct OrtReranker {
    session: Arc<Session>,
    tokenizer: Arc<Tokenizer>,
    model_id: String,
    max_seq_len: usize,  // 512 for bge-reranker
}

#[async_trait]
impl Reranker for OrtReranker {
    async fn rerank(&self, query: &str, candidates: &[RerankCandidate], top_n: usize) -> Result<Vec<ScoredCandidate>> {
        // 1. Build pairs: [query, chunk_text] with special tokens
        // 2. Truncate to 512 tokens (query first, then chunk)
        // 3. Batch (16-32 pairs), run on worker thread
        // 4. Apply sigmoid to logits → scores
        // 5. Sort by score desc, take top_n
    }
}
```

### 6.2 Latency Budget Management
| Pool Size | Expected Latency (CPU) | Action if Exceeded |
|-----------|------------------------|---------------------|
| 40 | ~2-4s | Shrink to 20-30 |
| 20 | ~1-2s | Acceptable |
| 10 | ~0.5-1s | Fast |

### 6.3 Score Threshold & "Insufficient Evidence"
```rust
pub struct RerankConfig {
    pub top_n: usize,
    pub min_score_threshold: f32,  // Calibrated from eval
    pub mmr_lambda: f32,
    pub per_source_cap: usize,
}

impl RetrievalPipeline {
    fn apply_threshold(&self, scored: Vec<ScoredCandidate>) -> RetrievalResult {
        if scored.is_empty() || scored[0].score < self.config.min_score_threshold {
            return RetrievalResult::InsufficientEvidence { 
                best_score: scored.first().map(|s| s.score),
                query: self.query.clone(),
            };
        }
        RetrievalResult::Evidence(/* filtered */)
    }
}
```

### 6.4 Exit Criteria
- [ ] nDCG@10 improves over Phase 5 baseline
- [ ] Answer support rate improves
- [ ] Threshold calibrated and documented
- [ ] Latency < 3s for pool=40 on CPU

---

## Phase 7: Query-Side Improvements (Week 7-8)

Add one at a time, keep only what eval supports:

| Feature | Implementation | Eval Gate |
|---------|---------------|-----------|
| Sub-question decomposition | Already in planner | Recall@k |
| Multi-query rewriting | 2-3 paraphrases from 4B model → RRF | Recall@k, latency |
| Keyword extraction | Entities/numbers for BM25 leg | Recall@k on code/error queries |
| HyDE | Hypothetical answer embedding | Likely OFF for 3B-5B |
| Conversation rewrite | Pronoun resolution for follow-ups | MRR on multi-turn |

---

## Phase 8: Research Pipeline Integration (Week 8-9)

### 8.1 Evidence Structure
```rust
// src-tauri/src/research/evidence.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub chunk_id: i64,
    pub source_id: i64,
    pub quote_span: (usize, usize),  // Verbatim span in source
    pub score: f32,
    pub heading_path: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievedEvidence {
    pub sub_question: String,
    pub evidence: Vec<Evidence>,
    pub n_candidates: usize,
    pub n_kept: usize,
    pub top_score: f32,
    pub latency_ms: u64,
}
```

### 8.2 Event Emission
```rust
// In research orchestrator
event_tx.send(ResearchEvent::Retrieved {
    sub_q: sub_question.clone(),
    n_candidates,
    n_kept: evidence.len(),
    top_score: evidence.first().map(|e| e.score).unwrap_or(0.0),
}).ok();
```

### 8.3 Settings Exposure
- Embedding model selector
- Reranker on/off
- Top-k sliders
- CPU/GPU toggle
- Thread cap

### 8.4 Exit Criteria
- [ ] Citations `[S#]` flow from retrieval → reader → writer
- [ ] Verbatim quote gate works (span matches source)
- [ ] UI shows retrieval transparency events

---

## Phase 9: Performance & Hardening (Ongoing)

| Task | Priority |
|------|----------|
| Int8 quantized ONNX models | High (2-3x speedup) |
| Lazy model load/unload | High (RAM pressure) |
| WAL mode + batched inserts | High |
| Corruption/migration tests | Medium |
| Tracing spans per stage | High (observability) |

---

## Phase 10: Advanced (Post-Metrics Plateau)

| Upgrade | Trigger |
|---------|---------|
| Parent-document retrieval | Answers need context |
| Persistent knowledge base | User files (PDF, MD, code) |
| ColBERT / SPLADE | Dense+BM25 recall plateaus |
| Fine-tune embedder | 1000+ labeled pairs |
| Distill reranker | Rerank latency bottleneck |

---

## Implementation Order & Dependencies

```
Phase 0 (Traits) 
    ↓
Phase 1 (Eval) ──────────────────┐
    ↓                              │
Phase 2 (Chunking)                │  (run in parallel)
    ↓                              │
Phase 3 (Embedder) ───────────────┤
    ↓                              │
Phase 4 (SQLite + sqlite-vec) ────┤
    ↓                              │
Phase 5 (Hybrid + RRF) ───────────┤
    ↓                              │
Phase 6 (Reranker) ───────────────┤
    ↓                              ↓
Phase 7 (Query improvements) ←───┘ (eval-gated)
    ↓
Phase 8 (Integration)
    ↓
Phase 9 (Hardening)
```

---

## Cargo.toml Additions

```toml
[dependencies]
# ONNX Runtime
ort = { version = "1.17", features = ["load-dynamic"] }

# Tokenization
tokenizers = "0.19"

# SQLite + sqlite-vec
rusqlite = { version = "0.31", features = ["bundled", "modern_sqlite", "vtab"] }
sqlite-vec = { version = "0.1", features = ["sqlite"] }  # or use bundled vec0

# Hashing
blake3 = "1.5"

# Async
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Error handling
thiserror = "2"
anyhow = "1.0"

# Metrics
metrics = "0.21"
```

---

## File Structure (New/Modified)

```
src-tauri/src/
├── embeddings/
│   ├── mod.rs              # Public exports
│   ├── traits.rs           # Embedder, Reranker traits
│   ├── ort_embedder.rs     # Granite embedding via ONNX
│   ├── ort_reranker.rs     # bge-reranker-v2-m3 via ONNX
│   ├── worker.rs           # Non-blocking worker thread
│   ├── cache.rs            # Embedding cache (model_id + hash)
│   └── download.rs         # Model download manager
├── ingest/
│   ├── semantic_chunk.rs   # NEW: Structure-aware chunking
│   └── chunk.rs            # DEPRECATE or adapt
├── index/
│   ├── db.rs               # REWRITE: SQLite + sqlite-vec
│   ├── schema.rs           # NEW: Schema + migrations
│   ├── migration.rs        # NEW: JSON → SQLite migration
│   └── fts.rs              # NEW: FTS5 wrapper
├── retrieval/
│   ├── pipeline.rs         # NEW: Hybrid retrieval pipeline
│   ├── fusion.rs           # RRF, MMR, per-source cap
│   ├── search.rs           # REWRITE: Use new pipeline
│   └── citations.rs        # ADAPT: Quote span verification
├── eval/
│   ├── mod.rs
│   ├── metrics.rs
│   ├── golden_set.rs
│   └── harness.rs          # Binary entry point
└── research/
    ├── evidence.rs         # NEW: Evidence structures
    └── orchestrator.rs     # INTEGRATE: Use new pipeline
```

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| ONNX Runtime GPU EP issues on Windows | Default to CPU; GPU opt-in with fallback |
| sqlite-vec compilation issues | Use bundled `vec0` extension; test early |
| Model download failures | Retry with exponential backoff; offline cache |
| CPU contention (ONNX + llama.cpp) | Cap intra_op_threads; monitor via tracing |
| Migration data loss | Dry-run mode; backup old index; verify counts |
| Reranker latency too high | Dynamic pool sizing; config threshold |

---

## Success Metrics (from Eval Harness)

| Metric | Baseline (BM25) | Target (Hybrid + Rerank) |
|--------|-----------------|--------------------------|
| Recall@5 | ~0.25 | > 0.45 |
| Recall@20 | ~0.50 | > 0.70 |
| Recall@50 | ~0.65 | > 0.85 |
| MRR | ~0.30 | > 0.50 |
| nDCG@10 | ~0.35 | > 0.55 |
| Answer Support Rate | ~0.40 | > 0.70 |
| P95 Latency | ~50ms | < 3000ms |

---

## Next Steps

1. **Start Phase 0**: Create `embeddings/traits.rs` and mock implementations
2. **Verify Granite model**: Download ONNX, check tokenizer, confirm prefixes
3. **Set up eval harness**: Create golden set from existing indexed projects
4. **Begin Phase 2**: Rewrite chunking to be structure-aware

---

*Plan created: 2026-10-25*
*Estimated timeline: 8-10 weeks for Phases 0-8*