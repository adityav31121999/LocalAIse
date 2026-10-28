# Chat & Agent Backend Analysis — LocalAIse

## Architecture Overview

LocalAIse is a **Tauri desktop app** (React + Rust) that runs a local LLM entirely on-device via `llama.cpp`. All inference stays on `localhost`; there is no cloud fallback.

```mermaid
flowchart TD
    subgraph Frontend ["Frontend (React / TypeScript)"]
        CV[ChatView.tsx]
        C[Composer.tsx]
        ML[MessageList.tsx]
        CS[chatStore.ts · Zustand]
        CA[api/chat.ts]
    end

    subgraph Tauri ["Tauri IPC Bridge"]
        CMD[commands/chat.rs\nchat_send]
        EVT["chat://token events\n(SSE deltas)"]
    end

    subgraph Backend ["Rust Backend"]
        subgraph Agent ["agent/"]
            AC[agent_chat.rs\nLangChain agent]
            LOOP[loop.rs\nResearch executor]
            MR[mapreduce.rs\nMap-Reduce summarizer]
            PLAN[plan.rs\nResearch plan model]
            PROMPTS[prompts.rs\nPrompt templates]
            FB[fallback.rs\nError messages]
        end
        subgraph Runtime ["runtime/"]
            CLI[client.rs\nHTTP ↔ llama-server]
            SUP[supervisor.rs\nProcess lifecycle]
        end
        subgraph Tools ["tools/"]
            TF[files.rs]
            TS[search_local.rs]
            TD[diary.rs]
            TC[calendar.rs]
            TW[web_search.rs]
            TWF[web_fetch.rs]
        end
        RET[retrieval/search.rs\nRAG chunk search]
    end

    subgraph LLM ["llama-server (localhost:PORT)"]
        LL["/v1/chat/completions\nOpenAI-compat API"]
    end

    C -->|user types| CS
    CS -->|send| CA
    CA -->|invoke chat_send| CMD
    CMD -->|SSE token stream| EVT
    EVT -->|onToken callback| CS
    CS --> ML
    CMD -->|RAG| RET
    CMD -->|chat mode| CLI
    CMD -->|agent mode| AC
    AC -->|tool calls| Tools
    AC -->|LLM calls| CLI
    CLI -->|HTTP POST /v1/chat/completions| LL
    CLI -->|stream bytes| EVT
```

---

## Chat Pipeline (Normal Mode)

The full request lifecycle for a single chat turn:

```mermaid
sequenceDiagram
    participant U as User
    participant CS as chatStore
    participant API as api/chat.ts
    participant CMD as commands/chat.rs
    participant RAG as retrieval/search.rs
    participant LLM as llama-server

    U->>CS: send(projectId, prompt)
    CS->>CS: optimistic UI (pending message)
    CS->>API: streamChat()
    API->>LLM: subscribe to chat://token events
    API->>CMD: invoke("chat_send")
    CMD->>RAG: search_chunks(prompt, limit=4)
    RAG-->>CMD: scored chunks + citations
    CMD->>CMD: build messages[] with RAG context
    CMD->>CMD: wait_until_ready() health check
    loop Token stream
        LLM-->>CMD: SSE delta
        CMD-->>API: emit("chat://token", {delta, reasoning})
        API-->>CS: onToken / onReasoning
        CS-->>U: live token render
    end
    CMD-->>API: ChatStreamResult {content, citations}
    API-->>CS: patch answer (authoritative)
    CS->>CS: save session, refresh chat list
```

### Key constants

| Constant | Value | Purpose |
|---|---|---|
| `RETRIEVAL_LIMIT` | 4 chunks | RAG context injected per turn |
| `HISTORY_LIMIT` | 12 turns | Bounded prompt replay |
| `READY_TIMEOUT` | 180 s | Health-poll before giving up |
| `RESPONSE_TIMEOUT` | 900 s | Max time for one completion |
| `temperature` | 0.2 | Fixed in non-agent LLM calls |

---

## Agent Mode (LangChain Tools Agent)

When a project is set to **Agent** mode, [`agent_chat.rs`](file:///d:/LocalAIse/src-tauri/src/agent/agent_chat.rs) runs a **LangChain `OpenAiToolAgentBuilder`** loop over the same local llama-server endpoint.

### Tool Set

| Tool | Backend helper | Condition |
|---|---|---|
| `read_file` | `tools::files::read_file` | Always |
| `list_files` | `tools::files::list_files` | Always |
| `write_file` | `tools::files::write_file` | Always (guarded by prompt) |
| `search_local` | `tools::search_local::search_local` | Always |
| `search_diary` | `tools::diary::search_diary` | Always |
| `list_events` | `tools::calendar::list_events_tool` | Always |
| `web_search` | `tools::web_search::search` | `internet_enabled = true` |
| `web_fetch` | `tools::web_fetch::fetch` | `internet_enabled = true` |

### Agent Loop

```mermaid
flowchart TD
    A[Build SimpleMemory from history] --> B[Build OpenAiToolAgent]
    B --> C[executor.stream_events]
    C -->|AgentStreamEvent::Text| D[deliver delta to UI]
    C -->|AgentStreamEvent::ToolStart| E["emit 🔧 note to UI"]
    E --> F[Execute tool]
    F --> G[Feed result back to model]
    G --> C
    C -->|AgentStreamEvent::Finish| H[Resolve AgentTurn]
    H --> I[Save to session]
```

**Hard cap:** `MAX_TOOL_STEPS = 6` — after 6 tool rounds the agent is forced to answer, preventing infinite loops.  
**Note clipping:** Tool call annotations shown to the user are capped at `MAX_NOTE_CHARS = 80` chars.

### System Prompt Composition (Agent Mode)

```
<user's custom project prompt, or default>
↓ appended automatically ↓
AGENT RULES: You are in agent mode … [tool use instructions]
[internet line: on/off]
```

---

## Research Agent (Deep Research)

The research workflow lives in [`agent/loop.rs`](file:///d:/LocalAIse/src-tauri/src/agent/loop.rs) and is separate from the chat agent. It executes a [`ResearchPlan`](file:///d:/LocalAIse/src-tauri/src/agent/plan.rs):

```mermaid
stateDiagram-v2
    [*] --> planned: create_starter_plan()
    planned --> approved: user approves in UI
    approved --> running: execute_research()
    running --> running: step loop (local RAG + web fetch)
    running --> completed: synthesis LLM call
    completed --> [*]: save report to outputs/*.md
```

**Plan structure** (auto-generated):
1. `local` — search indexed sources via RAG
2. `web` *(only if internet on)* — fetch Wikipedia-style URL
3. `synthesize` — LLM call to produce the final Markdown report

> [!NOTE]  
> Web steps currently use a hardcoded Wikipedia URL slug (`https://en.wikipedia.org/wiki/{slug}`). This is a placeholder — real web search goes through `network/gate.rs`.

---

## Map-Reduce Summarizer

[`mapreduce.rs`](file:///d:/LocalAIse/src-tauri/src/agent/mapreduce.rs) handles documents larger than the context window:

```
CHUNK_CHAR_LIMIT = 3,000 chars

Document ─► chunks[] ─► Map: summarize each chunk (concurrent LLM calls)
                        ─► Reduce: synthesize intermediate summaries into final report
```

Used by the analysis commands (`commands/files.rs`) for large file summarization.

---

## Streaming Architecture

Two separate channels keep the UI live during generation:

| Channel | Rust side | TS side |
|---|---|---|
| `chat://token` Tauri event | `app.emit("chat://token", {delta, reasoning})` | `subscribeChatTokens()` in `useStream.ts` |
| `invoke()` return value | `ChatStreamResult` | Authoritative final answer, replaces optimistic stream |

The **reasoning trace** (`<think>...</think>` or `reasoning_content` field in llama-server SSE) is a first-class concept:  
- Parsed in `client.rs` from `reasoning_content` or `reasoning` delta fields  
- Emitted separately with `reasoning: true`  
- Stored in `StoredMessage.reasoning`  
- Rendered as a collapsible `<details>` block in [`MessageList.tsx`](file:///d:/LocalAIse/src\components\chat\MessageList.tsx), hidden by `showThinking` toggle (persisted to `localStorage`)

---

## Session Persistence

Each project stores chats as JSON files in `<project>/.LocalAIse/chats/`:

```
chats/
  chat-001.json   ← { id, title, messages: [{ id, role, content, reasoning, citations, createdAt }] }
  chat-002.json
```

- `session_id = None` → new chat, fresh KV-cache (`cache_prompt: false`)
- `session_id = Some(id)` → continues saved session, KV-cache reuse allowed
- History is **windowed** to the last `HISTORY_LIMIT = 12` messages before being sent to the model

---

## GPU Fallback Behaviour

If the GPU server fails the health check (`wait_until_ready`), `commands/chat.rs` automatically:

1. Stops the GPU supervisor
2. Sets a fallback notice in the UI
3. Spawns a CPU-profile server (`ComputeDevice::Cpu`)
4. Retries the health check
5. If CPU also fails → marks state as failed and returns error

---

## Composer Capabilities

[`Composer.tsx`](file:///d:/LocalAIse/src\components\chat\Composer.tsx) is feature-rich beyond a basic textarea:

| Feature | Detail |
|---|---|
| File attachments | Text files + PDFs (pdf.js extracts text client-side) |
| Sampling controls | Temperature, Top-p, Top-k (UI state only; **not yet wired to the backend call**) |
| System prompt editor | Per-project custom system prompt, saved to project metadata |
| Cite-in-chat | Document viewer can queue a passage into the composer via `navStore.composerInsert` |
| Auto-resize textarea | `scrollHeight` capped at 180 px |

> [!WARNING]  
> The **sampling parameters** (temperature, top-p, top-k) in the Composer UI are stored in component state but are **not passed to `chat_send`**. The backend always uses a hardcoded `temperature: 0.2`. This is a gap between the UI and the actual inference call.

---

## Key Observations & Gaps

| # | Finding | Severity |
|---|---|---|
| 1 | Sampling params (temp, top-p, top-k) exist in Composer UI but are never sent to the Rust backend | 🟡 Medium |
| 2 | Research web steps use a hardcoded Wikipedia URL scheme — not a real search engine | 🟡 Medium |
| 3 | `MAX_TOOL_STEPS = 6` is a hard constant with no per-project override | 🟢 Low |
| 4 | Agent mode RAG context is built inside the LangChain chain (tool call), not pre-injected like chat mode | 🟢 Low |
| 5 | `complete_stream` rejects empty answers as errors — benign for safety but may surprise edge models | 🟢 Low |
| 6 | History window (`HISTORY_LIMIT = 12`) is a constant, not configurable per project | 🟢 Low |
