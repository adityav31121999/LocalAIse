# Resolution Plan — LocalAIse

This plan addresses the issues documented in:
- [`chat-agent-analysis.md`](chat-agent-analysis.md) — Sampling params UI/backend gap, hardcoded constants, agent/web search gaps
- [`unsolicited-pdf-reading.md`](unsolicited-pdf-reading.md) — (Currently empty file; issue inferred from codebase)

---

## Issue 1: Sampling Parameters Not Sent to Backend 🟡 Medium

**Finding from `chat-agent-analysis.md` #1:** The Composer UI has temperature, top-p, top-k controls but they are stored in local component state and **never passed to `chat_send`**. The backend always uses hardcoded `temperature: 0.2`.

### Current Flow
1. **Frontend (`Composer.tsx`)**: User adjusts sliders → updates `temperature`/`topP`/`topK` state
2. **`onSend`**: Only sends `prompt` string to `chat_send` — no sampling params
3. **Backend (`commands/chat.rs` → `client.rs`)**: `ChatRequest` has `temperature: f32` field but it's hardcoded to `0.2`; no `top_p` or `top_k` fields exist

### Resolution

#### Phase 1: Extend Backend Request Types (`src-tauri/src/runtime/client.rs`)
```rust
#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
    temperature: f32,
    top_p: Option<f32>,      // NEW
    top_k: Option<i32>,      // NEW
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_prompt: Option<bool>,
}
```
- Add optional `top_p` (0.0–1.0) and `top_k` (≥0) fields
- Make `temperature` configurable instead of hardcoded

#### Phase 2: Thread Sampling Params Through Call Chain
1. **`client.rs`**: Update `complete()` and `complete_stream()` to accept `SamplingParams` struct
2. **`commands/chat.rs`**: Accept optional sampling params in `chat_send` command signature
3. **`api/chat.ts`**: Pass sampling params from frontend to `chat_send` invoke

#### Phase 3: Frontend Integration (`src/components/chat/Composer.tsx`)
- On submit, include current `temperature`, `topP`, `topK` in the invoke call
- Add validation (clamp to valid ranges)

#### Phase 4: Agent Mode Support (`src-tauri/src/agent/agent_chat.rs`)
- Propagate sampling params to LangChain agent's LLM calls
- Currently agent uses `new()` which "sends no cap and no temperature" — wire through the params

---

## Issue 2: Hardcoded Wikipedia URL in Research Agent 🟡 Medium

**Finding from `chat-agent-analysis.md` #2:** Research web steps use a hardcoded Wikipedia URL scheme (`https://en.wikipedia.org/wiki/{slug}`) — not a real search engine.

### Current Flow (`src-tauri/src/agent/loop.rs`)
```rust
// In execute_research() → step loop for 'web' type steps:
let url = format!("https://en.wikipedia.org/wiki/{}", slug);
```

### Resolution
1. Replace hardcoded Wikipedia URL with actual web search via existing `tools/web_search.rs`
2. Use `web_search::search()` to get real URLs, then `web_fetch::fetch()` to retrieve content
3. This requires `internet_enabled = true` on the project (already gated)

---

## Issue 3: Hardcoded Constants Lack Project Override 🟢 Low

**Findings from `chat-agent-analysis.md`:**
- #3: `MAX_TOOL_STEPS = 6` hard constant, no per-project override
- #6: `HISTORY_LIMIT = 12` constant, not configurable per project

### Resolution
Add optional project settings:
```rust
// In project metadata/settings
struct ProjectChatSettings {
    max_tool_steps: Option<u32>,     // default 6
    history_limit: Option<usize>,    // default 12
    retrieval_limit: Option<usize>,  // default 4
}
```
- Read from project record in `chat_send`
- Fall back to constants when not set

---

## Issue 4: Agent Mode RAG Context Not Pre-injected 🟢 Low

**Finding from `chat-agent-analysis.md` #4:** Agent mode RAG context is built inside the LangChain chain (tool call), not pre-injected like chat mode.

### Resolution
- Pre-inject RAG context into the agent's initial system prompt or first user message (like chat mode does)
- Avoid redundant tool calls for retrieval that chat mode already handles

---

## Issue 5: Empty Answer Rejection May Surprise Edge Models 🟢 Low

**Finding from `chat-agent-analysis.md` #5:** `complete_stream` rejects empty answers as errors.

### Resolution
- Make this configurable or log a warning instead of hard error
- Some models legitimately return empty for certain prompts

---

## Issue 6: Unsolicited PDF Reading 🟡 Medium

**Inferred from codebase:** When a user opens a PDF in the DocumentViewer (Files view), the backend automatically parses the full PDF text via `pdf-extract` (`read_document` command). This happens **without explicit user consent** — simply viewing the file triggers extraction.

### Current Flow
1. User clicks a PDF in DirectoryView → opens DocumentViewer
2. `DocumentViewer.tsx` calls `readDocument(projectId, filePath)` on mount
3. Backend `read_document` command parses entire PDF via `pdf-extract`
4. Text content returned and stored in `DocumentContent.text`

### Problems
- **Performance**: Large PDFs take seconds to parse, blocking UI
- **Privacy expectation**: User may not want content extracted just to view visually
- **Resource usage**: `pdf-extract` can panic on malformed PDFs (handled but still costly)

### Resolution Options

#### Option A: Lazy/On-Demand Extraction (Recommended)
- **Visual rendering only by default**: Load PDF bytes for `PDFViewer` without text extraction
- **"Extract Text" button**: User explicitly requests text extraction for:
  - "Cite in Chat" (needs excerpt)
  - Summarization
  - Copy text
- Backend: Add `extract_text: bool` parameter to `read_document` command

#### Option B: Background Extraction with Progress
- Start extraction in background when viewer opens
- Show "Extracting text…" placeholder
- Cache result for subsequent actions

#### Option C: Hybrid (Default for Small, Lazy for Large)
- Auto-extract if PDF < threshold (e.g., 50 pages / 2MB)
- Lazy for larger files

### Implementation Plan (Option A)
1. **Backend** (`commands/projects.rs`):
   - Add `extract_text: bool` parameter to `read_document` (default `false`)
   - Only call `pdf::parse()` when `true`
   - Always return `file_bytes_base64` for visual rendering

2. **Frontend** (`api/document.ts`):
   - Update `readDocument()` to accept `extractText?: boolean`
   - DocumentViewer: call with `extractText: false` initially

3. **DocumentViewer** (`DocumentViewer.tsx`):
   - Load visual PDF immediately (bytes only)
   - Show "Extract Text" button when text not yet loaded
   - On click: re-fetch with `extractText: true`, then enable "Cite in Chat", "Copy", "Summarize"

4. **Cite in Chat**: Only works after text extraction (shows disabled state before)

---

## Implementation Priority

| Priority | Issue | Effort | Impact |
|----------|-------|--------|--------|
| **P0** | Sampling params to backend | Medium | High — UI controls actually work |
| **P1** | Unsolicited PDF reading | Medium | High — privacy/perf, fixes unexpected behavior |
| **P2** | Research web search (real) | Medium | High — research agent becomes useful |
| **P3** | Configurable constants (tool steps, history) | Low | Medium — power user flexibility |
| **P4** | Agent RAG pre-injection | Low | Medium — consistency with chat mode |
| **P5** | Empty answer handling | Low | Low — edge case robustness |

---

## Testing Checklist

- [ ] Sampling params: Verify temperature=0.0 gives deterministic output, temperature=1.0 varies
- [ ] Sampling params: Verify top-p and top-k affect diversity
- [ ] Sampling params: Verify agent mode respects params
- [ ] PDF: Opening large PDF doesn't freeze UI
- [ ] PDF: "Extract Text" button works, enables Cite/Summarize
- [ ] PDF: Cite in Chat uses extracted text, not placeholder
- [ ] Research: Web step uses real search, not Wikipedia
- [ ] Config: Project settings override tool steps/history limit