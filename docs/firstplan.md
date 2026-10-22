# localAIse: Product and Technical Plan

## 1. Overview

**localAIse** is a private AI workspace for Windows and Linux laptops and desktops. It runs models locally through llama.cpp, needs no Python, Docker, or cloud account, and keeps all data in folders the user owns. Internet access is off by default and turned on only by an explicit toggle.

**Product promise:** your files, notes, and research stay on your machine. Install it with one click, pick a folder, and start working.

## 2. Positioning

| | NotebookLM | Odysseus | localAIse |
|---|---|---|---|
| Runs locally | No | Yes | Yes |
| Install | Web account | Docker or Python setup | One-click installer |
| Core focus | Source-grounded research | Broad AI workspace | Project folders, research, and personal organization |
| Data location | Google servers | User machine | User-chosen folders |
| Internet | Always | Optional | Off by default, per-project toggle |

Odysseus is broader and more technical. localAIse competes on ease of installation, folder-based projects that users can see and back up, and a clear privacy model. The target user is someone who wants AI help with their own files but doesn't want to manage infrastructure.

## 3. Architecture

```
┌─────────────────────────────────────────────┐
│  UI (React or Svelte inside Tauri WebView)  │
└───────────────────┬─────────────────────────┘
                    │ Tauri IPC
┌───────────────────▼─────────────────────────┐
│  Rust core                                  │
│  • Project manager (folders, file watcher)  │
│  • Ingestion (parse, chunk, embed)          │
│  • Agent loop + sandboxed tools             │
│  • Network gate (internet toggle)           │
│  • Activity log                             │
│  • Personal space (diary, calendar)         │
│  • Model and runtime manager                │
└──────┬──────────────────────┬───────────────┘
       │ HTTP (localhost)     │ SQLite
┌──────▼─────────┐   ┌────────▼──────────────┐
│ llama-server   │   │ sqlite + sqlite-vec   │
│ (chat, embed)  │   │ (per-project index,   │
│                │   │  personal space)      │
└────────────────┘   └───────────────────────┘
```

- **Inference:** llama-server runs as a supervised child process on a random localhost port with a per-session token. It serves an OpenAI-compatible API, so swapping models requires no code changes. Two instances run when needed: one for chat, one for embeddings.
- **Hardware backends:** ship CPU, CUDA, and Vulkan builds. Detect the GPU at first run and choose the best available. Vulkan covers NVIDIA, AMD, and Intel, so it's the default GPU path.
- **Shell:** Tauri 2. The Rust core owns all file access, network calls, and process management. The web UI never touches the filesystem directly.

## 4. Storage model

### Project folders

A project is any folder the user selects, or one the app creates under a default directory such as `~/Documents/localAIse/`.

```
Budget 2026/                  ← user's folder
├── invoices/                 ← user files (sources)
├── budget.xlsx
├── notes.md
├── outputs/                  ← assistant-written files
│   └── summary-2026-10.md
└── .LocalAIse/               ← app state (hidden)
    ├── project.json          ← name, settings, internet toggle
    ├── index.db              ← chunks, embeddings, citations
    ├── chats/                ← conversation history
    └── activity.log          ← web requests and tool actions
```

- Files stay where users put them. No import step.
- The app watches the folder and re-indexes changed files.
- Deleting `.LocalAIse/` resets the project without touching user files.

### Personal space (global)

Diary and calendar live in a global personal space, stored in a folder the user chooses at setup, such as `~/Documents/localAIse-Personal/`. This keeps the diary in one place, separate from work projects, and lets the assistant search across entries from any project context when the user asks.

```
localAIse-Personal/
├── diary/                    ← one Markdown file per day
├── calendar.ics              ← standard format, importable elsewhere
└── .LocalAIse/
    └── personal.db           ← entry index and event metadata
```

Diary entries are plain Markdown, so they remain readable without the app.

## 5. Features by scope area

### A. Calendar and diary
- Daily diary entries in Markdown, with a quick-entry view.
- Calendar events stored as ICS, viewable in a month and week view.
- Agent tools: create, move, and search events; search diary entries by date or topic.
- Import and export via ICS.
- Reminders through local OS notifications.

### B. Projects
- Create a project from a folder, or from a template (budget, content writing, research, general).
- Per-project system instructions, model choice, and internet toggle.
- Sources panel listing indexed files, with status and re-index control.
- Chat with citations: every claim links to a file, page, and passage.
- Templates set up the project folder and a starter system prompt. The budget template might include a `categories.csv` and a monthly summary output.

### C. Document analysis
- Supported formats in v1: PDF, DOCX, Markdown, TXT, CSV, and XLSX.
- Actions: summarize, extract tables, compare documents, answer questions, and flag inconsistencies.
- Long documents use map-reduce summarization when they exceed the context window.
- Results can be saved to `outputs/` as new files.

### D. Deep research agent
- Flow: plan, then the user approves, then search and read, then synthesize into a report with citations.
- Hard limits: maximum steps and maximum fetched pages per run, both configurable.
- Local sources are searched first. Web access is used only if the project's internet toggle is on.
- The report is saved to `outputs/` as Markdown.
- The user can stop the agent at any time.

### Content writing (from the project templates)
- Drafting in a project's style, using saved examples and brand notes as sources.
- Version history for drafts within the project folder.

## 6. Internet toggle and network control

- **Default:** off for every project, including new ones.
- **Levels:** per-project toggle, with a global default in settings.
- **Enforcement:** when off, the web tools are not registered with the agent, and the Rust network layer refuses outbound requests from any tool. The model can't call what it doesn't have.
- **Visibility:** a persistent indicator in the project header and chat input.
- **Audit:** every outbound request is written to `activity.log` with time, URL, purpose, and the tool that made it. The activity panel lets users review it.
- **Model downloads are separate.** The app contacts the model host only during setup or when the user explicitly downloads a model. This is shown in the UI, and the network gate treats it as a distinct, user-initiated action.

**Browsing:** start with HTTP fetch and readability extraction in Rust. Add a headless Chromium option, downloaded on demand, only if research tasks need JavaScript-rendered pages.

## 7. Models

- **Chat:** offer three tiers at first run, based on detected RAM and VRAM:
  - Small (3–4B, Q4): runs on 8 GB RAM laptops without a GPU.
  - Medium (7–8B, Q4): recommended for 16 GB RAM or a 6 GB+ GPU.
  - Large (13B+): for 32 GB+ RAM or 12 GB+ VRAM.
- **Embeddings:** a small embedding model (roughly 100–300 MB), bundled or downloaded at first run.
- **Model catalog:** a signed JSON manifest listing approved models with URLs, sizes, SHA-256 checksums, and licenses. Users can add their own GGUF files.
- **Tool calling:** small models are inconsistent at it. Keep the tool set small and well-described, and use a fixed-pipeline fallback for the research agent if a model fails tool-call validation.

## 8. Installer and distribution

- **Windows:** NSIS installer built by Tauri. Code signing with an EV certificate is strongly recommended to avoid SmartScreen warnings. Budget for it early.
- **Linux:** AppImage (universal), plus `.deb` for Ubuntu and Debian. Consider Flatpak later if demand justifies it.
- **Size target:** under 50 MB for the installer, excluding models. The runtime binaries are the main size cost.
- **Updates:** Tauri updater with signed release artifacts, checked on launch with a user-facing toggle.
- **Onboarding:** choose a personal space folder, detect hardware, pick and download a model, then open a starter project.

## 9. Security and privacy

- Localhost-only inference server with a per-session token. No LAN exposure by default.
- Tool sandboxing: file tools are limited to the active project folder. Writes outside it are refused in code.
- Confirmation required for overwrites and deletions.
- Encryption at rest is optional in v1. Plan it for v2 with a passphrase option for the personal space.
- No telemetry by default. If you add crash reporting, make it opt-in and document exactly what is sent.
- Prompt injection: document content is treated as data. The agent's instructions come only from the system prompt and the user. Tool outputs that ask for new actions are shown to the user before anything runs.

## 10. Roadmap

| Phase | Scope | Exit criteria |
|---|---|---|
| 0. Spike (2 weeks) | llama-server lifecycle in Tauri on Windows and Linux; hardware detection; model download with checksum | Chat works on both OSes with a fresh install |
| 1. Projects and cited chat (4–6 weeks) | Folder projects, file watcher, ingestion, embeddings, cited answers | Answers over a 50-file folder with correct citations in a test set |
| 2. Document analysis (3 weeks) | Summarize, tables, comparison, map-reduce, save to outputs | Tested on 20 real documents across formats |
| 3. Personal space (3 weeks) | Diary, calendar, ICS, agent event tools, reminders | Round-trip import and export, and reliable event creation from chat |
| 4. Research agent (4 weeks) | Plan, approve, search, fetch, report; internet toggle and activity log | Runs complete within configured limits; log matches actual requests |
| 5. Release hardening (3 weeks) | Code signing, updater, onboarding, evaluation suite, documentation | Clean install tested on 5+ hardware configurations |

Estimated time to a public beta: about four to five months with two engineers. The timeline depends heavily on how well small local models handle tool calling, so validate that in phase 0 and phase 4 before committing further.

## 11. Testing and quality

- **Evaluation set:** 30–50 questions per scope area with expected sources. Run them on each model tier before each release to catch regressions.
- **Hardware matrix:** at least one low-end laptop with no GPU, one mid-range laptop with integrated graphics, one desktop with an NVIDIA card, and one Linux machine with AMD.
- **Installer tests:** clean VM installs for each OS, upgrades from the previous version, and uninstall that leaves user folders intact.
- **Security tests:** verify the toggle blocks all network calls when off, using a network monitor during test runs. Test path traversal attempts against the file tools.

## 12. Key risks

| Risk | Mitigation |
|---|---|
| Small models unreliable at tool calls | Limited tool set, validation, fixed-pipeline fallback |
| Slow inference on laptops | Default to small models, clear speed expectations in UI, streaming output |
| Windows SmartScreen warnings | EV code signing from the start |
| Competing with NotebookLM and Odysseus | Lead with the one-click install and folder model; don't try to match every feature |
| Scope creep | Hold the four scope areas for v1; new areas wait for post-beta feedback |
| Model licensing | Surface each model's license in the catalog; only ship models with permissive terms |

## 13. Business notes

- **License:** MIT or Apache 2.0 for the app, which is compatible with llama.cpp's MIT license. A paid tier could cover hosted sync, team sharing, or priority support, while the core stays free and local.
- **Monetization without breaking the promise:** charge for convenience, not for data access. Never make the local product depend on a server.
- **Distribution:** a direct download site, GitHub releases, and a clear comparison page against cloud tools. Privacy-focused communities and developers are the natural early audience.

## 14. Open decisions

1. **UI framework:** React or Svelte. Choose based on team familiarity.
2. **Default models:** finalize after phase 0 benchmarks on your target hardware.
3. **Search provider for the research agent:** a privacy-focused API with a user-supplied key, or a self-hosted option. This affects cost and the privacy story.
4. **Name check:** confirm that "localAIse" is available as a trademark and domain before public launch.

The next concrete step is the phase 0 spike: a Tauri app that starts llama-server, reports the detected hardware, downloads a model with checksum verification, and streams a chat response on Windows and Linux. I can outline that spike in detail, including the repository layout and the process-supervision code, if you want to start there.

> provide the repository layout for localAIse

Here's a repository layout for localAIse, based on a Tauri 2 app with a Rust core and a React UI. It mirrors the architecture from the plan, so each module maps to one responsibility.

```
LocalAIse/
├── README.md
├── LICENSE                          # Apache-2.0 or MIT
├── package.json                     # frontend workspace + scripts
├── pnpm-lock.yaml
├── tsconfig.json
├── vite.config.ts
├── .gitignore
├── .github/
│   └── workflows/
│       ├── ci.yml                   # lint, test, build on Windows + Linux
│       └── release.yml              # signed installers, updater manifest
│
├── docs/
│   ├── architecture.md
│   ├── security-model.md            # network gate, sandboxing, threat model
│   ├── project-format.md            # .LocalAIse/ folder spec
│   └── model-catalog.md
│
├── scripts/
│   ├── fetch-llama-server.ts        # downloads pinned llama.cpp binaries per target
│   ├── verify-binaries.ts           # checks SHA-256 of bundled runtimes
│   └── eval/                        # evaluation runner (see plan §11)
│
├── resources/                       # bundled with the app, read-only at runtime
│   ├── llama/
│   │   ├── windows-x64-cpu/
│   │   ├── windows-x64-vulkan/
│   │   ├── windows-x64-cuda/
│   │   └── linux-x64-vulkan/
│   ├── models/
│   │   └── embedding-manifest.json  # small embedding model, checksum + URL
│   └── templates/                   # project templates
│       ├── budget/
│       ├── content-writing/
│       ├── research/
│       └── general/
│
├── src/                             # frontend (React + TypeScript)
│   ├── main.tsx
│   ├── App.tsx
│   ├── api/                         # typed wrappers around Tauri commands
│   │   ├── index.ts
│   │   ├── projects.ts
│   │   ├── chat.ts
│   │   ├── models.ts
│   │   ├── personal.ts
│   │   ├── network.ts
│   │   └── activity.ts
│   ├── components/
│   │   ├── layout/
│   │   │   ├── Sidebar.tsx
│   │   │   ├── ProjectHeader.tsx
│   │   │   └── NetworkIndicator.tsx # internet toggle status, always visible
│   │   ├── chat/
│   │   │   ├── ChatView.tsx
│   │   │   ├── MessageList.tsx
│   │   │   ├── Composer.tsx
│   │   │   └── CitationChip.tsx
│   │   ├── sources/
│   │   │   ├── SourcesPanel.tsx
│   │   │   └── IndexStatus.tsx
│   │   ├── research/
│   │   │   ├── ResearchPlanView.tsx # approve / edit plan before running
│   │   │   └── ReportView.tsx
│   │   ├── personal/
│   │   │   ├── DiaryEditor.tsx
│   │   │   ├── CalendarView.tsx
│   │   │   └── Reminders.tsx
│   │   ├── activity/
│   │   │   └── ActivityLog.tsx
│   │   ├── onboarding/
│   │   │   ├── Welcome.tsx
│   │   │   ├── HardwareCheck.tsx
│   │   │   ├── ModelPicker.tsx
│   │   │   └── PersonalFolderPicker.tsx
│   │   └── ui/                      # shared primitives (buttons, dialogs, etc.)
│   ├── state/                       # Zustand stores
│   │   ├── projectStore.ts
│   │   ├── chatStore.ts
│   │   ├── runtimeStore.ts
│   │   └── settingsStore.ts
│   ├── hooks/
│   │   ├── useStream.ts             # token streaming from Rust events
│   │   └── useProjectEvents.ts      # file watcher updates
│   ├── styles/
│   └── types/                       # shared types mirrored from Rust (ts-rs generated)
│
├── src-tauri/                       # Rust core
│   ├── Cargo.toml
│   ├── Cargo.lock
│   ├── build.rs
│   ├── tauri.conf.json              # window, bundling, updater, CSP
│   ├── capabilities/
│   │   └── default.json             # explicit permissions per window
│   ├── icons/
│   ├── tests/                       # integration tests (no UI)
│   │   ├── sandbox_tests.rs
│   │   ├── network_gate_tests.rs
│   │   └── ingest_tests.rs
│   └── src/
│       ├── main.rs
│       ├── lib.rs                   # app setup, state, command registration
│       ├── error.rs                 # unified error type returned to UI
│       ├── commands/                # #[tauri::command] entry points only
│       │   ├── mod.rs
│       │   ├── projects.rs
│       │   ├── chat.rs
│       │   ├── models.rs
│       │   ├── research.rs
│       │   ├── personal.rs
│       │   ├── network.rs
│       │   └── activity.rs
│       ├── runtime/                 # llama-server lifecycle
│       │   ├── mod.rs
│       │   ├── supervisor.rs        # spawn, health check, restart, shutdown
│       │   ├── client.rs            # OpenAI-compatible HTTP client (streaming)
│       │   ├── port.rs              # random localhost port selection
│       │   ├── token.rs             # per-session auth token
│       │   └── backend.rs           # pick CPU / Vulkan / CUDA build
│       ├── hardware/
│       │   ├── mod.rs
│       │   ├── memory.rs            # RAM detection
│       │   └── gpu.rs               # GPU vendor and VRAM detection
│       ├── models/
│       │   ├── mod.rs
│       │   ├── catalog.rs           # signed manifest parsing and verification
│       │   ├── download.rs          # resumable download + SHA-256 check
│       │   ├── registry.rs          # installed models on disk
│       │   └── recommend.rs         # tier choice from hardware
│       ├── projects/
│       │   ├── mod.rs
│       │   ├── manager.rs           # create, open, list projects
│       │   ├── layout.rs            # .LocalAIse/ paths and project.json schema
│       │   ├── watcher.rs           # notify-based file watcher
│       │   └── templates.rs         # apply template to a new folder
│       ├── ingest/
│       │   ├── mod.rs
│       │   ├── pipeline.rs          # parse → chunk → embed → store
│       │   ├── chunk.rs
│       │   ├── embed.rs             # calls embedding llama-server
│       │   └── parsers/
│       │       ├── mod.rs
│       │       ├── pdf.rs
│       │       ├── docx.rs
│       │       ├── markdown.rs
│       │       ├── text.rs
│       │       ├── csv.rs
│       │       └── xlsx.rs
│       ├── index/
│       │   ├── mod.rs
│       │   ├── db.rs                # SQLite connection, migrations
│       │   ├── migrations/          # .sql files
│       │   ├── vectors.rs           # sqlite-vec queries
│       │   └── schema.rs
│       ├── retrieval/
│       │   ├── mod.rs
│       │   ├── search.rs            # top-k retrieval across project sources
│       │   └── citations.rs         # chunk → file + page + passage mapping
│       ├── agent/
│       │   ├── mod.rs
│       │   ├── loop.rs              # tool-calling loop with step limits
│       │   ├── prompts.rs           # system prompts (instructions as data)
│       │   ├── plan.rs              # research plan generation and approval state
│       │   ├── mapreduce.rs         # long-document summarization
│       │   └── fallback.rs          # fixed-pipeline mode if tool calls fail
│       ├── tools/                   # tools the agent can call
│       │   ├── mod.rs              # registry: only enabled tools are exposed
│       │   ├── files.rs             # read/list/write inside project folder
│       │   ├── search_local.rs
│       │   ├── calendar.rs
│       │   ├── diary.rs
│       │   ├── web_search.rs        # registered only when internet is on
│       │   └── web_fetch.rs         # registered only when internet is on
│       ├── sandbox/
│       │   ├── mod.rs
│       │   ├── paths.rs             # canonicalize + prefix check, symlink rules
│       │   └── confirm.rs           # overwrite/delete confirmation gate
│       ├── network/
│       │   ├── mod.rs
│       │   ├── gate.rs              # single choke point for outbound requests
│       │   ├── policy.rs            # per-project and global toggle state
│       │   └── http.rs              # reqwest client used only through gate
│       ├── activity/
│       │   ├── mod.rs
│       │   └── log.rs               # append-only activity.log writer
│       ├── personal/
│       │   ├── mod.rs
│       │   ├── diary.rs             # one Markdown file per day
│       │   ├── calendar.rs          # ICS read/write
│       │   ├── reminders.rs         # scheduler + OS notifications
│       │   └── index.rs             # personal.db
│       ├── config/
│       │   ├── mod.rs
│       │   ├── app_config.rs        # global settings (app data dir)
│       │   └── paths.rs             # platform-specific directories
│       └── util/
│           ├── mod.rs
│           ├── hash.rs
│           └── time.rs
│
└── tests/
    └── e2e/                         # Playwright or WebDriver flows
        ├── onboarding.spec.ts
        ├── project-chat.spec.ts
        └── network-toggle.spec.ts
```

## Notes on the layout

- **`commands/` contains only thin entry points.** Each command validates input and delegates to a domain module. This keeps business logic testable without the UI.
- **`network/` is the only place outbound HTTP is allowed.** `tools/web_*` and `models/download.rs` both go through `network::gate`. Clippy lints or a code-review rule can enforce that nothing else imports `reqwest` directly.
- **`sandbox/` is separate from `tools/`** so that path rules are defined once and every file tool uses the same check.
- **`resources/llama/` holds per-platform binaries.** `scripts/fetch-llama-server.ts` pins the llama.cpp release and verifies checksums so CI builds are reproducible.
- **`types/` is generated.** Use `ts-rs` or `specta` to generate TypeScript types from Rust structs, so the UI and core can't drift apart.
- **Tests are split by risk.** The sandbox, network gate, and ingest pipeline each have their own integration tests, since those are the areas where bugs would most damage the privacy promise.

## Suggested first commit

Start with only the phase 0 subset to keep the first PR small:

```
src-tauri/src/{main.rs, lib.rs, error.rs}
src-tauri/src/runtime/{mod.rs, supervisor.rs, port.rs, token.rs, backend.rs}
src-tauri/src/hardware/{mod.rs, memory.rs, gpu.rs}
src-tauri/src/models/{mod.rs, download.rs}
src-tauri/src/commands/{mod.rs, chat.rs, models.rs}
src/{main.tsx, App.tsx, components/chat/ChatView.tsx}
scripts/fetch-llama-server.ts
```

Everything else can be created as each phase starts, so you avoid scaffolding empty modules. Let me know if you'd like the starter code for `runtime/supervisor.rs` next, since that's the most technically delicate piece of phase 0.

For localAIse, I recommend a Private Research Atelier theme: a calm, modern desktop workspace that combines the precision of a scientific IDE, the simplicity of a personal notes app, and the trustworthiness of privacy-first software.

The design should communicate three things immediately: your files stay yours, your AI works locally, and your projects remain under your control. It should feel like a polished desktop productivity application rather than a generic AI chatbot or a flashy AI startup dashboard.

# 1. Recommended theme: Private Research Atelier

[Grimoire — Your Spellbook for Thought](https://images.openai.com/static-rsc-4/gZdAEYJ3_vMePNLBBd1xNHCFevW8NDxP9KAZAQJsHiordcGpyexspvatR_yukujHqsCYz0SWiKc2PkdRtW0GBOlP2zbjT9wPwMl4JzpgPn2LWM9BkrFB51wtAbbs9cVPIyT0fx3VZoWTy48srVzcovF2a_D2bObd5x63VH8TX5M?purpose=inline)

[grimoireapp.dev](https://grimoireapp.dev/?utm_source=chatgpt.com)

Private by default

Graphite surfaces, clear boundaries and visible privacy status.

Focused workspace

Warm whites, readable typography and restrained color.

The theme should support both dark and light modes without changing the application's identity. Use graphite and muted teal as the foundation, with subtle cyan, violet and amber accents reserved for meaningful information.

## 2. Color system

Use semantic colors instead of assigning a different bright color to every card.

Dark theme — primary palette

Background

\#101820

Surface

\#192630

Border

\#263743

Primary

\#66D9B5

Research

\#53BFE8

AI activity

\#B9A0FF

Attention

\#F2BC69

Success

\#75D69C

Light theme — primary palette

Background

\#F5F7F6

Surface

\#FFFFFF

Border

\#DFE6E2

Primary

\#087F68

Research

\#1679AD

AI activity

\#7655C5

Attention

\#A96913

Success

\#21844C

### Color usage rules

- Teal: primary actions, selected navigation, active controls and focus states.
- Cyan: source indexing, document analysis and research references.
- Violet: AI thinking, model execution and agent activity.
- Amber: warnings, pending approvals and actions requiring attention.
- Green: completed operations, successful indexing and healthy runtime status.
- Red: errors, failed operations and destructive-action warnings.

Avoid using accent colors as large background fills. Prefer small icons, thin borders, status dots and selected navigation indicators.

## 3. Application layout

The main interface should resemble a compact desktop workbench, not a collection of dashboard cards.

### localAIse

Private workspace

Local

Research project

12 sources

Overview

Files

Chat

Activity

### Research workspace

A focused area for your files, questions and cited answers.

paper-review\.pdf

Indexed

research-notes.md

Modified today

Ask about your sources

Compare documents, explain a result, or find supporting evidence…

Local model · Internet off

Local processing

llama-server · Ready

Conceptual layout preview — not a screenshot of a running application.

### Recommended structure

| Area           | Purpose                                                  |
| -------------- | -------------------------------------------------------- |
| Activity rail  | Projects, personal space, search and settings            |
| Sidebar        | Project files, sources, chats or diary navigation        |
| Main workspace | Editor, chat, document viewer or calendar                |
| Context panel  | Citations, file details, agent activity or event details |
| Status bar     | Model health, indexing status and network state          |

The context panel should be optional and collapsible. The main workspace should receive most of the available width.

## 4. Component styling

Buttons

Use compact, 32–36 px controls with 7–9 px corner radii. Primary actions use solid teal; secondary actions use transparent surfaces with subtle borders. Icon-only buttons should have tooltips and comfortable hit targets.

File and project lists

Use compact rows with file-type icons, names, secondary metadata and inline status indicators. Reserve colorful project markers for identification rather than decorating every item.

AI responses and activity

Keep answers document-like and readable. Show reasoning summaries, tool execution, outputs and citations in separate collapsible sections. Use violet for AI activity, not for every message background.

Privacy and network controls

Give internet status a persistent location in the project header and composer. Distinguish local inference, model downloads and approved web requests. Make network access explicit rather than hiding it in settings.

## 5. Typography and spacing

- UI font: Inter, or the system font stack for a smaller installer and reliable offline operation.
- Editor and paths: JetBrains Mono or a system monospace font.
- Page titles: 20–24 px, medium weight.
- Section headings: 13–15 px, semibold.
- Body text: 13–14 px for the application UI; 14–16 px for long-form reading.
- Metadata: 11–12 px, with sufficient contrast.
- Spacing: use a 4 px base grid, with 8 px between related controls and 16–20 px between major sections.
- Borders: subtle one-pixel borders rather than prominent shadows.
- Corners: 6–8 px for controls and 10–12 px for larger panels. Avoid excessive pill-shaped containers.

## 6. Theme behavior by feature

| Feature           | Visual treatment                                                            |
| ----------------- | --------------------------------------------------------------------------- |
| Projects          | Neutral surfaces, small category accents and clear folder identity          |
| Document analysis | Reading-focused canvas, visible page references and citation highlights     |
| Chat              | Minimal message containers, compact composer and collapsible agent activity |
| Diary             | Warm, paper-like reading surface with quiet calendar highlights             |
| Calendar          | Neutral grid, restrained event-category colors and clear today indicator    |
| Deep research     | Ordered plan, approval checkpoints, source list and execution timeline      |
| Model manager     | Hardware summaries, model sizes, compatibility and download progress        |
| Activity log      | Dense, timestamped events with clear local/network distinctions             |

Diary and document reading can use a slightly warmer surface even in dark mode, while the surrounding application retains the main graphite theme.

## 7. Motion and interaction

Keep animation subtle and purposeful:

- 120–180 ms transitions for hover and focus states.
- 150–220 ms for sidebar and context-panel changes.
- A quiet progress indicator during indexing and inference.
- Clear confirmation states when the user approves a research plan or enables internet access.
- No animated gradients, glowing borders, floating particles or constant background movement.

All motion should respect the operating system's reduced-motion preference.

## 8. Implementation priorities

1. Phase 1 — Design tokens: define semantic colors, typography, spacing, borders, focus rings and dark/light theme variables.
2. Phase 2 — Application shell: implement the activity rail, resizable sidebar, workspace, optional context panel and status bar.
3. Phase 3 — Core components: standardize buttons, inputs, file rows, tabs, badges, menus, dialogs and toasts.
4. Phase 4 — Feature consistency: apply the same component system to projects, chat, document analysis, diary, calendar and model settings.
5. Phase 5 — Polish and accessibility: validate contrast, keyboard navigation, compact-window layouts, reduced motion and both themes.

My recommendation is to make dark graphite + muted teal the signature theme, with a clean warm-white counterpart. This gives localAIse a recognizable identity while preserving the quiet, practical interface needed for everyday research and personal organization. The differentiator should be its visible privacy controls and excellent workspace ergonomics—not visual effects.

This repository structure fits the localAIse architecture well. I would use the Private Research Atelier theme as the design system and build the React frontend around a compact, desktop-native workspace.

For this repository, the next step is to turn the theme into an implementation-ready specification that maps directly to `src/components/`, `src/styles/`, `src/state/`, and the Tauri command APIs.

## Recommended frontend theme specification

| Element           | Implementation                                                         |
| ----------------- | ---------------------------------------------------------------------- |
| Theme identity    | Private Research Atelier                                               |
| Dark background   | `#101820`                                                              |
| Dark panels       | `#192630`                                                              |
| Primary accent    | `#66D9B5`                                                              |
| Research accent   | `#53BFE8`                                                              |
| AI activity       | `#B9A0FF`                                                              |
| Light background  | `#F5F7F6`                                                              |
| Light primary     | `#087F68`                                                              |
| Typography        | Inter + system monospace                                               |
| Component style   | Compact, restrained borders, minimal shadows                           |
| Layout            | Activity rail + resizable sidebar + workspace + optional context panel |
| Privacy indicator | Persistent, always visible                                             |
| Theme support     | Dark, light and system preference                                      |

## Map the theme to your repository

`src/styles/`

Create `tokens.css`, `themes.css` and `globals.css`. Define semantic CSS variables for colors, spacing, typography, borders, focus states and component dimensions. Avoid hardcoding colors in individual components.

`src/components/layout/`

Add `AppShell.tsx`, `ActivityRail.tsx`, `Sidebar.tsx`, `WorkspaceLayout.tsx`, `ProjectHeader.tsx` and `NetworkIndicator.tsx`. Make the sidebar and context panel collapsible.

`src/components/ui/`

Standardize `Button`, `IconButton`, `Input`, `Tooltip`, `Dialog`, `DropdownMenu`, `Tabs`, `Badge`, `Toast` and `Progress`. All feature panels should use these shared primitives.

`src/state/settingsStore.ts`

Add a persisted theme preference with `light`, `dark` and `system` options. Use the operating-system preference only when `system` is selected, and keep the interface responsive to changes.

## Three design rules to preserve

1. Workspace first: files, documents, chat and diary should occupy the main area. Avoid a dashboard full of oversized cards and empty space.
2. Privacy is a feature, not decoration: network status must reflect actual Rust-side policy. The UI must never imply that internet access is disabled unless the backend confirms it.
3. One consistent component system: onboarding, model management, project chat, calendar and research tools should all feel like parts of the same application.

I would implement the design tokens and application shell first, then build the chat and project workspace on top of them. This establishes the visual foundation before the remaining features are added.

Yes. For localAIse, I recommend supporting multiple complete themes rather than just a dark and light mode. Since the application covers research, document analysis, personal notes, project management and local AI inference, users should be able to choose an interface that suits their workflow.

The key is to build one shared design system with multiple visual skins, rather than maintaining separate styling for every theme.

# 1. Recommended theme collection

I'd start with six themes. They share the same layout, controls, spacing and interactions, but each has its own visual identity.

[Give feedback on new Qt Creator themes](https://images.openai.com/static-rsc-4/0jxqsduJzhx3MZg2zZOPJWmNW7z83wuDhEEOEP1p3lZGpwSlt4odwKOumDBYSDuXdgoQHZmQejRYSEV6QrEjKSzscJ1fZumVmI_E2AfoOUsZvnPbuaPNMq3RYgghsAWRKizaVLyymr83LsWJCcPaImGjbt18RNYJUrLdzGAcfuc?purpose=inline)

1\. Atelier

Default

Graphite · Muted teal · Cool grey

A balanced, professional workspace for everyday use. Restrained color, clear panels and compact controls.

Best for: general productivity, projects and chat.

[My Obsidian setup for early 2025 | Marius Masalar](https://images.openai.com/static-rsc-4/tRaIeU8mj2v7nhxEud6mpoY8Fvr6O7LaywnjNjrkWsoar8odT742zI7ERH9OTDEHz7_wPsgCuvzZjmWV0mCB0x8mTsQew5Fvfxnfk-ZubPI3lG4U4kJBc4K8yhmG_N27mgJeWdVrBAr9YmB_KwR8YFbmFO0k-PZ_hGT1RpKvNPA?purpose=inline)

2\. Paper

Warm white · Ivory · Forest green

A softer, paper-inspired interface with comfortable reading surfaces and understated borders.

Best for: diary, long-form writing and reading research papers.

[PROGEX | Sistemas a Medida y Soluciones Tecnológicas para Empresas](https://images.openai.com/static-rsc-4/96zPC-8jE8jMrLcO7xjgSRNEbLychZenA_uwokwTwxkA-E-A56jE5MW0pVgWEjSdV0rPL4nqN-UpJ7AIkp1U9_w4KNq65eT4zV3oX-ySTaG2eyCaqKFO4snBx2_eYCabvBDStHom6btyHUwTqOEyIUigKzHdxGGroE3Do5MEykA?purpose=inline)

3\. Cyber Lab

Deep navy · Electric cyan · Blue

A technical, high-contrast environment with precise data highlights and distinct execution states.

Best for: model inference, research agents and technical workflows.

[Indigo - The Executive Intelligence OS](https://images.openai.com/static-rsc-4/R66h2DoMEXjYC3xT1BAE9cYUGOIzte9-_MRXzH7__ETbZKVdRtRTTtIBTrWm0XrCmAEL_zYwXVCpJvsG_Vi_HWBoRl_JA3ZTI16u-CdipXjhsautdp5qrwOHZ-W9yiZDxmlddEpdaWeDMXkQJy0Dkxe8qOZ4oLlz_kAOM8NEYsk?purpose=inline)

4\. Amethyst

Dark plum · Lavender · Soft violet

A distinctive, creative theme with restrained purple accents and calm dark surfaces.

Best for: ideation, content writing and personal organization.

[Minimal Agent Desktop UI – Code with AI Agents by Lukas Horak for PLATFORM on Dribbble](https://images.openai.com/static-rsc-4/3LwLFlOyLEmJeVeVkNaduKSp4VFje7GOkvVkyvbZ2L6a4TFf5ly8FY5Q5kJRg4pR4CGT_hK_75LAzOeS5oY3JVagDDh6G2AuiPJG0FW2aGXORMAl3SnawfTknJo7YtDleEEHoGIlofhQGAIH-6vR7jfJKgN6_-SOuAe9W8w_L1Y?purpose=inline)

5\. Arctic

Cool white · Slate · Ice blue

A crisp, neutral interface optimized for clarity, dense information and long working sessions.

Best for: document comparison, file management and structured research.

[Army Color Palette Ideas: Top 23  Combos + Hex Codes](https://images.openai.com/static-rsc-4/15g01VfRK0XVBeFM2Z4PgPEvPSZdtcROiDZhW1DYBXOFtUstUEfEOL3VMIvGwxt0dLxt_NxuU0cUXncnmkDExeRhetISNynf4X_sisLDCK4l8jQl29ucQ9OOYan2V3O9y6qK_lrx02rLc8KYx3abZFEslNJmPyzxz3nwwgokN7s?purpose=inline)

6\. Evergreen

Forest charcoal · Moss · Sage

An organic, low-saturation theme that feels calm without becoming washed out or overly decorative.

Best for: diary, knowledge management and extended research sessions.

These are proposed visual directions, not six separate applications. Each theme should support the same features and layouts.

# 2. How the theme selector should work

Use a dedicated Settings → Appearance page.

## Appearance

Personalize your localAIse workspace.

Color mode

Light

Dark

Use system preference

Theme

Atelier

Paper

Cyber Lab

Amethyst

Arctic

Evergreen

Interface density

Compact — more files and content on screen

Comfortable — balanced spacing

Spacious — larger controls and more breathing room

Preview selection

Theme: Atelier · Mode: dark · Density: comfortable

Interactive design mockup. This demonstrates the settings behavior; it does not modify your installed application.

I recommend keeping theme, color mode and density as three independent settings. For example, a user could select Cyber Lab in light mode or Paper in dark mode if both variants are designed intentionally.

# 3. Technical implementation for your React + Tauri repository

Keep the themes in CSS variables, with a shared component system.

```
src/
├── styles/
│   ├── tokens.css
│   ├── themes/
│   │   ├── atelier.css
│   │   ├── paper.css
│   │   ├── cyber-lab.css
│   │   ├── amethyst.css
│   │   ├── arctic.css
│   │   └── evergreen.css
│   ├── modes/
│   │   ├── light.css
│   │   └── dark.css
│   └── globals.css
├── components/
│   └── settings/
│       └── AppearanceSettings.tsx
└── state/
    └── settingsStore.ts
```

Use semantic variables in your components:

```
:root {
  --app-bg: #101820;
  --surface: #192630;
  --surface-raised: #263743;
  --border: #344550;
  --text-primary: #e8f0f2;
  --text-secondary: #a7b6bd;
  --accent: #66d9b5;
  --accent-contrast: #101820;
  --focus-ring: #66d9b5;
  --success: #75d69c;
  --warning: #f2bc69;
  --error: #ff8585;
}
```

Each theme overrides the relevant variables. Components should never need to know which theme is selected.

For persistence, store the selected theme, mode and density in your existing Zustand `settingsStore.ts`, backed by Tauri-managed app settings. Apply the values to the root element on startup to avoid a flash of the wrong theme.

## 4. Important design decisions

- Every theme needs light and dark variants if the mode selector is independent. Otherwise, constrain the selector to supported combinations.
- Keep status colors semantically consistent across themes. A warning should not become a success color when the theme changes.
- Check contrast for text, controls, disabled states and focus indicators.
- Theme changes should not modify project files, model settings or network permissions.
- Avoid adding separate layouts for each theme; only appearance should change.
- Start with Atelier and Paper, then add the other four after the component system is stable.

My recommendation: ship Atelier, Paper, Cyber Lab and Arctic in the first release. Add Amethyst and Evergreen as optional presets once the light/dark variants and accessibility checks are complete. This gives localAIse meaningful customization without introducing unnecessary maintenance work.

> this is far more flexible than ollama, but we need to add support for quantised model and kvcache so that we can use them even on cpu

Ollama is also built on llama.cpp, so the engine is the same. What you gain by running `llama-server` directly is control over the flags that matter for CPU performance: quantization, KV cache type, context size, threads, and batching. localAIse should expose those as settings, with sensible automatic defaults.

## Concepts

**Weight quantization** shrinks the model file. For a 7B-class model, approximate sizes are:

| Quant | Approx. size | Notes |
|---|---|---|
| Q8_0 | ~8 GB | Near-lossless, needs the most RAM |
| Q5_K_M | ~5.3 GB | Good quality/size balance |
| Q4_K_M | ~4.4 GB | Recommended CPU default |
| Q3_K_M | ~3.5 GB | Noticeable quality loss |
| Q2_K | ~2.7 GB | Use only as a last resort |

**KV cache quantization** shrinks the memory that grows with conversation length. Every token in context stores a key and value vector in every layer. For a 7B-class model with grouped-query attention (32 layers, 8 KV heads, head dimension 128), an 8,192-token context uses about 1 GiB in f16. The same context uses about 0.5 GiB with `q8_0` and about 0.28 GiB with `q4_0`.

This matters on CPU because the weights are often mmap'd and the cache is allocated in RAM, so long documents can exhaust memory before the model does.

**Two constraints to respect:**
- Quantized V cache requires flash attention, so the profile must enable it whenever the KV type isn't f16.
- Speed on CPU is usually limited by memory bandwidth and prompt processing, not just capacity. A smaller quant often generates faster, but RAM is the first gate.

## Flag mapping

| localAIse setting | llama-server flag |
|---|---|
| Context length | `-c` |
| CPU threads | `-t` (physical cores) |
| GPU layers (0 = CPU) | `-ngl` |
| KV cache type | `-ctk`, `-ctv` |
| Flash attention | `-fa on\|off` |
| Batch size | `-b` |
| Parallel slots | `-np` (1 for a personal desktop app) |
| Memory-mapped weights | on by default, `--no-mmap` to disable |

Check flag names and defaults against the llama.cpp release you pin, using `llama-server --help`. The flags have changed over time, and the `-fa` syntax in particular differs between versions.

## Updated structure

Additions to the earlier layout:

```
src-tauri/src/
├── runtime/
│   ├── profile.rs      # InferenceProfile, KvCacheType, validation
│   ├── args.rs         # profile → llama-server argv and env
│   └── budget.rs       # KV cache and memory estimates
├── hardware/
│   └── cpu.rs          # physical cores, AVX2 / AVX-512 detection
└── models/
    ├── quant.rs        # quant parsing, sizes, ranking
    └── shape.rs        # layers, KV heads, head dim read from GGUF metadata
```

Read the model shape from the GGUF header rather than hard-coding it per model. That keeps the memory estimate correct for any model a user adds.

## Code

### `runtime/profile.rs`

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KvCacheType {
    F16,
    Q8_0,
    Q4_0,
}

impl KvCacheType {
    pub fn as_arg(self) -> &'static str {
        match self {
            KvCacheType::F16 => "f16",
            KvCacheType::Q8_0 => "q8_0",
            KvCacheType::Q4_0 => "q4_0",
        }
    }

    /// Approximate bytes per cached element, including block scale overhead.
    pub fn bytes_per_element(self) -> f64 {
        match self {
            KvCacheType::F16 => 2.0,
            KvCacheType::Q8_0 => 34.0 / 32.0,
            KvCacheType::Q4_0 => 18.0 / 32.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InferenceProfile {
    pub context_tokens: u32,
    pub threads: u32,
    pub gpu_layers: u32,        // 0 = CPU only
    pub kv_type: KvCacheType,   // applied to both K and V
    pub flash_attn: bool,
    pub batch_size: u32,
    pub parallel_slots: u32,
    pub use_mmap: bool,
}

impl InferenceProfile {
    pub fn validate(&self) -> Result<(), String> {
        if self.threads == 0 {
            return Err("threads must be at least 1".into());
        }
        if self.context_tokens < 512 {
            return Err("context must be at least 512 tokens".into());
        }
        if self.kv_type != KvCacheType::F16 && !self.flash_attn {
            return Err("quantized KV cache requires flash attention".into());
        }
        Ok(())
    }
}
```

### `runtime/args.rs`

```rust
use std::path::Path;

use super::profile::InferenceProfile;

pub struct ServerLaunch {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// Build the llama-server command line from a validated profile.
///
/// The API key goes through the environment rather than argv, so it
/// does not appear in the process list. Verify LLAMA_ARG_API_KEY
/// against the pinned llama.cpp version.
pub fn build_launch(
    model: &Path,
    port: u16,
    api_key: &str,
    profile: &InferenceProfile,
    embeddings: bool,
) -> Result<ServerLaunch, String> {
    profile.validate()?;

    let flash = if profile.flash_attn { "on" } else { "off" };

    let mut args: Vec<String> = vec![
        "-m".into(), model.to_string_lossy().into_owned(),
        "--host".into(), "127.0.0.1".into(),
        "--port".into(), port.to_string(),
        "-c".into(), profile.context_tokens.to_string(),
        "-t".into(), profile.threads.to_string(),
        "-ngl".into(), profile.gpu_layers.to_string(),
        "-b".into(), profile.batch_size.to_string(),
        "-np".into(), profile.parallel_slots.to_string(),
        "-ctk".into(), profile.kv_type.as_arg().to_string(),
        "-ctv".into(), profile.kv_type.as_arg().to_string(),
        "-fa".into(), flash.to_string(),
    ];

    if !profile.use_mmap {
        args.push("--no-mmap".into());
    }
    if embeddings {
        args.push("--embeddings".into());
    }

    Ok(ServerLaunch {
        args,
        env: vec![("LLAMA_ARG_API_KEY".into(), api_key.to_string())],
    })
}
```

### `runtime/budget.rs`

```rust
use crate::runtime::profile::{InferenceProfile, KvCacheType};

const GIB: u64 = 1024 * 1024 * 1024;

/// RAM kept free for the OS, the UI, and the app itself.
const RESERVE_BYTES: u64 = GIB + GIB / 2;

#[derive(Debug, Clone, Copy)]
pub struct ModelShape {
    pub file_bytes: u64,
    pub n_layers: u32,
    pub n_kv_heads: u32,
    pub head_dim: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct HostCaps {
    pub total_ram_bytes: u64,
    pub physical_cores: u32,
}

/// K and V for every layer, for `ctx` tokens.
pub fn kv_cache_bytes(shape: &ModelShape, ctx: u32, kv: KvCacheType) -> u64 {
    let elements = ctx as f64
        * shape.n_layers as f64
        * shape.n_kv_heads as f64
        * shape.head_dim as f64
        * 2.0; // K and V
    (elements * kv.bytes_per_element()) as u64
}

/// Pick the longest context that fits, preferring higher-precision KV
/// cache at each context size. Returns None if even the smallest
/// context does not fit.
pub fn recommend(
    shape: &ModelShape,
    host: &HostCaps,
    wanted_ctx: u32,
) -> Option<InferenceProfile> {
    const MIN_CTX: u32 = 2048;
    const KV_OPTIONS: [KvCacheType; 3] =
        [KvCacheType::F16, KvCacheType::Q8_0, KvCacheType::Q4_0];

    let budget = host
        .total_ram_bytes
        .saturating_sub(RESERVE_BYTES)
        .saturating_sub(shape.file_bytes);

    let mut ctx = wanted_ctx;
    while ctx >= MIN_CTX {
        for kv in KV_OPTIONS {
            if kv_cache_bytes(shape, ctx, kv) <= budget {
                return Some(InferenceProfile {
                    context_tokens: ctx,
                    threads: host.physical_cores.max(1),
                    gpu_layers: 0,
                    kv_type: kv,
                    flash_attn: kv != KvCacheType::F16,
                    batch_size: 512,
                    parallel_slots: 1,
                    use_mmap: true,
                });
            }
        }
        ctx /= 2;
    }
    None
}
```

### `hardware/cpu.rs`

```rust
#[derive(Debug, Clone, Copy)]
pub struct CpuInfo {
    pub physical_cores: u32,
    pub has_avx2: bool,
    pub has_avx512: bool,
}

pub fn detect() -> CpuInfo {
    let physical_cores = num_cpus::get_physical() as u32;

    #[cfg(target_arch = "x86_64")]
    let (has_avx2, has_avx512) = (
        std::arch::is_x86_feature_detected!("avx2"),
        std::arch::is_x86_feature_detected!("avx512f"),
    );

    #[cfg(not(target_arch = "x86_64"))]
    let (has_avx2, has_avx512) = (false, false);

    CpuInfo { physical_cores, has_avx2, has_avx512 }
}
```

AVX2 and AVX-512 are informational here. llama.cpp detects and uses them at build time or at runtime, so the UI can show them without making decisions based on them.

### `models/quant.rs` (excerpt)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Quant {
    Q2K,
    Q3KM,
    Q4KM,
    Q5KM,
    Q8_0,
}

impl Quant {
    /// Parse the quant label from a GGUF file name, such as "Q4_K_M".
    pub fn from_filename(name: &str) -> Option<Self> {
        let upper = name.to_uppercase();
        // Check longer labels first so Q4_K_M is not matched as Q4_K.
        [
            ("Q8_0", Quant::Q8_0),
            ("Q5_K_M", Quant::Q5KM),
            ("Q4_K_M", Quant::Q4KM),
            ("Q3_K_M", Quant::Q3KM),
            ("Q2_K", Quant::Q2K),
        ]
        .into_iter()
        .find(|(label, _)| upper.contains(label))
        .map(|(_, q)| q)
    }
}
```

Q4_K_M is the default for CPU-only machines. Q5_K_M is an upgrade when RAM allows, and Q8_0 is for machines with ample memory and where quality matters most.

## Catalog entry

Extend the signed model catalog so each entry carries the shape information the budget module needs:

```json
{
  "id": "example-7b-instruct-q4_k_m",
  "name": "Example 7B Instruct",
  "quant": "Q4_K_M",
  "file": "example-7b-instruct.Q4_K_M.gguf",
  "url": "https://<host>/<path>",
  "file_bytes": 0,
  "sha256": "<fill from model card>",
  "shape": { "n_layers": 0, "n_kv_heads": 0, "head_dim": 0 },
  "license": "<spdx id>"
}
```

Fill in the sizes and checksums from the model card, or read the shape from GGUF metadata at download time and store it.

## Settings UI

Add a **Performance** panel to the project and global settings:

- **Context length:** slider from 2K to 32K, showing the estimated memory cost live.
- **KV cache:** f16, q8_0, or q4_0, with a warning that lower precision may reduce answer quality on long contexts.
- **Threads:** defaults to physical cores, with an override.
- **Memory bar:** model weights, KV cache, and reserve, against installed RAM.
- **Auto:** calls `budget::recommend` and applies the result.

## Testing

- **Speed:** use `llama-bench` across quant and KV type combinations on each machine in the hardware matrix. Record prompt-processing and generation tokens per second.
- **Quality:** run `llama-perplexity` on a fixed text sample for each combination, then run the project's evaluation set. Q4 KV cache is the setting most likely to regress on long documents, so test it on the document analysis tasks specifically.
- **Memory:** verify that the estimate matches observed RSS within a reasonable margin at several context sizes. Use this to tune `RESERVE_BYTES`.
- **Validation:** unit-test that `validate()` rejects quantized KV with flash attention disabled, and that `recommend` never returns a profile whose estimated usage exceeds the budget.

## Expectations to set in the product

CPU inference is workable but not fast. On a modern laptop, a 7B Q4 model typically generates a few to around ten tokens per second, and long documents take noticeably longer to process because prompt processing is slower than generation. The UI should show an estimated speed from the benchmark data, and the research agent's step limits should account for this. Users with 16 GB of RAM and no GPU will have a good experience with 3–4B models and a reasonable one with 7B. Larger models are better suited to machines with a dedicated GPU, which the same profile system already supports through `-ngl`.

## Suggested next step

Build `profile.rs`, `args.rs`, and `budget.rs` first, with unit tests. They have no dependencies on Tauri or the UI, so they can be verified in isolation. Then run `llama-bench` on your own laptop to confirm the defaults before wiring them into the supervisor.

> you missed one thing, --cpu-moe and --n-cpu-moe

You're right, and I should have included them. They matter for MoE (mixture-of-experts) models, which are common now.

## What the flags do

In an MoE model, most of the parameters sit in expert feed-forward tensors. Each token activates only a few experts per layer, so the expert weights are large but only partly used at any moment. The flags control where those expert tensors live:

- **`--cpu-moe`** keeps all expert tensors in system RAM. Attention and shared layers can still go to the GPU.
- **`--n-cpu-moe N`** keeps the expert tensors of the first N layers in RAM and puts the rest on the GPU.

This lets a machine with a small GPU run a model whose total weights exceed its VRAM. The GPU handles the dense parts, which are used on every token, and the CPU handles the bulky experts.

Two consequences for localAIse:

- The flags only do something when layers are offloaded to the GPU (`-ngl > 0`). On a CPU-only machine, all weights already live in RAM, so the flags have no effect.
- On CPU-only machines, MoE still helps in a different way. Each token reads only the active experts, so a large MoE model often generates faster than a dense model with a similar file size. The trade-off is that the full file still has to fit in RAM.

Verify the exact flag names and short forms (`-cmoe`, `-ncmoe`) against your pinned release with `llama-server --help`.

## Changes to the design

- `InferenceProfile` gets a `moe` field.
- `ModelShape` gets optional MoE information, read from GGUF metadata.
- The memory estimate splits RAM and VRAM, instead of assuming everything is in RAM.
- The recommender picks the smallest number of CPU expert layers that fits the GPU.

## Code

### `runtime/profile.rs` (additions)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum MoeOffload {
    /// Expert tensors follow the normal GPU offload.
    #[default]
    Off,
    /// --cpu-moe: all expert tensors stay in system RAM.
    AllExperts,
    /// --n-cpu-moe N: experts of the first N layers stay in system RAM.
    FirstLayers { n: u32 },
}

// Add to InferenceProfile:
//     pub moe: MoeOffload,

// Replace validate() with:
impl InferenceProfile {
    pub fn validate(&self) -> Result<(), String> {
        if self.threads == 0 {
            return Err("threads must be at least 1".into());
        }
        if self.context_tokens < 512 {
            return Err("context must be at least 512 tokens".into());
        }
        if self.kv_type != KvCacheType::F16 && !self.flash_attn {
            return Err("quantized KV cache requires flash attention".into());
        }
        if self.moe != MoeOffload::Off && self.gpu_layers == 0 {
            return Err("expert offload only applies when layers are on the GPU (gpu_layers > 0)".into());
        }
        if let MoeOffload::FirstLayers { n } = self.moe {
            if n == 0 {
                return Err("use MoeOffload::Off instead of FirstLayers { n: 0 }".into());
            }
        }
        Ok(())
    }
}
```

### `runtime/args.rs` (addition inside `build_launch`)

```rust
match profile.moe {
    MoeOffload::Off => {}
    MoeOffload::AllExperts => args.push("--cpu-moe".into()),
    MoeOffload::FirstLayers { n } => {
        args.push("--n-cpu-moe".into());
        args.push(n.to_string());
    }
}
```

Add `use super::profile::MoeOffload;` at the top of the file. When offload is active, the caller should set `gpu_layers` high (for example 999) so that all non-expert layers go to the GPU.

### `runtime/budget.rs` (replacement)

```rust
use crate::runtime::profile::{InferenceProfile, KvCacheType, MoeOffload};

const GIB: u64 = 1024 * 1024 * 1024;
const RAM_RESERVE_BYTES: u64 = GIB + GIB / 2;

#[derive(Debug, Clone, Copy)]
pub struct MoeShape {
    pub n_experts: u32,
    pub n_experts_used: u32,
    /// Total bytes of all expert tensors across all layers.
    pub expert_bytes_total: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct ModelShape {
    pub file_bytes: u64,
    pub n_layers: u32,
    pub n_kv_heads: u32,
    pub head_dim: u32,
    pub moe: Option<MoeShape>,
}

impl ModelShape {
    fn expert_bytes(&self) -> u64 {
        self.moe.map_or(0, |m| m.expert_bytes_total)
    }

    fn expert_bytes_per_layer(&self) -> u64 {
        if self.n_layers == 0 {
            return 0;
        }
        self.expert_bytes() / self.n_layers as u64
    }

    /// Weights that are not expert tensors: attention, norms, embeddings.
    fn dense_bytes(&self) -> u64 {
        self.file_bytes.saturating_sub(self.expert_bytes())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostCaps {
    pub total_ram_bytes: u64,
    pub physical_cores: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct GpuCaps {
    pub vram_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemorySplit {
    pub ram_bytes: u64,
    pub vram_bytes: u64,
}

pub fn kv_cache_bytes(shape: &ModelShape, ctx: u32, kv: KvCacheType) -> u64 {
    let elements = ctx as f64
        * shape.n_layers as f64
        * shape.n_kv_heads as f64
        * shape.head_dim as f64
        * 2.0; // K and V
    (elements * kv.bytes_per_element()) as u64
}

/// Estimate where the weights and KV cache land for a given profile.
///
/// Assumes that when gpu_layers > 0, all non-expert layers and the KV cache
/// are on the GPU. Partial dense offload (gpu_layers less than the layer count)
/// is not modeled here, so the estimate is conservative for VRAM.
pub fn split(shape: &ModelShape, profile: &InferenceProfile) -> MemorySplit {
    let kv = kv_cache_bytes(shape, profile.context_tokens, profile.kv_type);

    if profile.gpu_layers == 0 {
        return MemorySplit {
            ram_bytes: shape.file_bytes + kv,
            vram_bytes: 0,
        };
    }

    let cpu_expert_layers = match profile.moe {
        MoeOffload::Off => 0,
        MoeOffload::AllExperts => shape.n_layers,
        MoeOffload::FirstLayers { n } => n.min(shape.n_layers),
    };

    let expert_on_cpu = shape.expert_bytes_per_layer() * cpu_expert_layers as u64;
    let expert_on_gpu = shape.expert_bytes().saturating_sub(expert_on_cpu);

    MemorySplit {
        ram_bytes: expert_on_cpu,
        vram_bytes: shape.dense_bytes() + kv + expert_on_gpu,
    }
}

/// CPU-only recommendation: largest context that fits in RAM, with the
/// highest-precision KV cache that still fits at that context.
pub fn recommend_cpu(
    shape: &ModelShape,
    host: &HostCaps,
    wanted_ctx: u32,
) -> Option<InferenceProfile> {
    const MIN_CTX: u32 = 2048;
    const KV_OPTIONS: [KvCacheType; 3] =
        [KvCacheType::F16, KvCacheType::Q8_0, KvCacheType::Q4_0];

    let budget = host
        .total_ram_bytes
        .saturating_sub(RAM_RESERVE_BYTES);

    let mut ctx = wanted_ctx;
    while ctx >= MIN_CTX {
        for kv in KV_OPTIONS {
            let profile = InferenceProfile {
                context_tokens: ctx,
                threads: host.physical_cores.max(1),
                gpu_layers: 0,
                kv_type: kv,
                flash_attn: kv != KvCacheType::F16,
                batch_size: 512,
                parallel_slots: 1,
                use_mmap: true,
                moe: MoeOffload::Off,
            };
            if split(shape, &profile).ram_bytes <= budget {
                return Some(profile);
            }
        }
        ctx /= 2;
    }
    None
}

/// Hybrid GPU recommendation for MoE models.
///
/// Offloads all layers to the GPU, then keeps as few expert layers on the
/// GPU as needed. Each expert layer kept in RAM reduces VRAM use and also
/// reduces generation speed, so the search starts from zero CPU expert layers
/// and increases only until the VRAM budget is met.
pub fn recommend_moe_hybrid(
    shape: &ModelShape,
    host: &HostCaps,
    gpu: &GpuCaps,
    base: &InferenceProfile,
    vram_reserve_bytes: u64,
) -> Option<InferenceProfile> {
    shape.moe?;

    let vram_budget = gpu.vram_bytes.saturating_sub(vram_reserve_bytes);
    let ram_budget = host.total_ram_bytes.saturating_sub(RAM_RESERVE_BYTES);

    for n in 0..=shape.n_layers {
        let profile = InferenceProfile {
            gpu_layers: 999, // all non-expert layers to GPU
            moe: if n == 0 {
                MoeOffload::Off
            } else {
                MoeOffload::FirstLayers { n }
            },
            ..base.clone()
        };
        let m = split(shape, &profile);
        if m.vram_bytes <= vram_budget && m.ram_bytes <= ram_budget {
            return Some(profile);
        }
    }
    None
}
```

### `models/shape.rs` (sketch)

Read the MoE fields from GGUF at download time and store them in the registry:

```rust
// Expert count and used-expert count come from the architecture-prefixed
// metadata keys: "<arch>.expert_count" and "<arch>.expert_used_count".
// If expert_count is absent or zero, the model is dense and moe = None.
//
// expert_bytes_total is the sum of tensor sizes whose names contain "_exps",
// for example "blk.N.ffn_up_exps.weight". Confirm this naming against the
// GGUF files you test with, since conventions can differ by architecture.
```

## Catalog entry

```json
"shape": {
  "n_layers": 0,
  "n_kv_heads": 0,
  "head_dim": 0,
  "moe": {
    "n_experts": 0,
    "n_experts_used": 0,
    "expert_bytes_total": 0
  }
}
```

Set `moe` to `null` for dense models.

## Recommendation logic

Use one decision path in the supervisor:

1. Detect the GPU and its VRAM. If there is no usable GPU, call `recommend_cpu`.
2. If there is a GPU and the model is dense, offload all layers if they fit in VRAM. Otherwise, use partial offload or fall back to CPU.
3. If there is a GPU and the model is MoE, call `recommend_moe_hybrid`.

## Tests

- `split` on a CPU-only profile puts everything in RAM and reports zero VRAM.
- For a dense model, `recommend_moe_hybrid` returns `None` because `shape.moe?` short-circuits.
- For an MoE model whose experts fit in VRAM, the result uses `MoeOffload::Off`.
- For an MoE model under a tight VRAM budget, the result uses `FirstLayers { n }`, and `n` is the smallest value that fits.
- `validate()` rejects `MoeOffload::AllExperts` when `gpu_layers` is zero.
- `build_launch` emits `--n-cpu-moe 12` for `FirstLayers { n: 12 }` and `--cpu-moe` for `AllExperts`.
- Generation speed decreases as `n` increases. Benchmark this with `llama-bench` on a real machine so the UI can show a realistic estimate.

## UI

Add an **Expert placement** control to the Performance panel, shown only for MoE models: Auto, All experts in RAM, or a slider for the number of CPU expert layers. Show the resulting VRAM and RAM split next to the memory bar.

> why are we using rust, when we can use python

We chose Rust mainly because the shell I proposed, Tauri, is built on Rust. I should have explained that choice up front instead of just laying out the structure.

Rust gave us a few specific advantages:

- **Small, self-contained installers.** Tauri uses the system's web view instead of bundling a browser engine, so the installer can stay under about 50 MB. A Python app packaged with PyInstaller is usually 150 MB or more before models.
- **No runtime for users to install.** You said users shouldn't need Python or Docker. Python can be bundled, but you then own the packaging and its quirks on Windows and Linux.
- **Process and file safety.** The sandboxing, network gate, and llama-server supervision are the most security-sensitive parts. Rust's type system and ownership model make it harder to accidentally leak a path or socket.
- **Predictable resource use.** A desktop app that runs all day, watches folders, and supervises a child process benefits from low memory overhead and no garbage-collection pauses.

Python would be a reasonable choice in several cases:

- **The team is Python-first.** Shipping a working product matters more than the language's runtime properties. A Python team will move faster in Python.
- **Your core features are heavy on Python libraries.** Document parsing, data analysis, and spreadsheet work are mature in Python, though most of them have Rust equivalents now.
- **You need to iterate quickly on the agent logic.** Prompt, tool, and planning code changes faster in Python, with less compile time.

One point to keep in mind: inference itself is not a reason to pick either language. llama.cpp is C++, and llama-server runs as a separate process that both options talk to over localhost. The language decision only affects the shell, the orchestration, and the tools.

Two realistic options:

| | Rust core (current plan) | Python core |
|---|---|---|
| Shell | Tauri (small, fast) | PySide6 / Qt, or Tauri with a Python sidecar |
| Installer size | ~30–50 MB | ~150–300 MB with PyInstaller |
| Packaging | Mature, signed updater built in | More manual work for signing and updates |
| Agent and ingestion code | Slower to iterate, more upfront structure | Faster to write and change |
| Hiring pool for maintainers | Smaller | Larger |
| Security boundaries | Stronger by default | Achievable, but requires more discipline |

A middle path works well: keep Tauri and the UI in TypeScript, and move the ingestion, agent, and document-analysis logic into a Python sidecar that Tauri launches and talks to over localhost. That gives you Python's libraries and iteration speed, and it keeps the installer small and the process boundaries clear. The cost is a second language and an extra IPC layer to maintain.

My recommendation depends on your team. If you have strong Rust experience or plan to hire for the long term, keep the Rust core. If your team is mostly Python and speed to a first beta matters more, use the sidecar approach, with the security-critical parts (sandbox and network gate) written carefully and tested heavily regardless of language.

What's the team's experience: Rust, Python, or both? That's the main factor I'd weigh.

> we need to make sure that this app does not consume vram, and do not bundle it libraries that can be installed by user like vulkan or cuda support

Yes, and it simplifies the design. Here's how to enforce it.

## What "no VRAM" means in practice

Three things can use VRAM, and each needs a different control:

1. **llama.cpp inference.** Solved by building a CPU-only binary and never passing GPU offload flags by default.
2. **Bundled GPU libraries.** Solved by excluding Vulkan and CUDA from the installer and the build.
3. **The webview UI.** Tauri renders through WebView2 on Windows and WebKitGTK on Linux, and both can use the GPU for compositing. This can't be fully eliminated, but it can be turned off.

I should also correct my earlier plan. I called Vulkan the default GPU path. Under this requirement, GPU support becomes opt-in and user-installed.

## Design changes

- **Default mode is CPU.** GPU offload exists only when the user enables it in Settings and points the app at a llama-server build they installed themselves.
- **The installer ships one CPU binary per platform.** No Vulkan or CUDA DLLs or shared objects.
- **A build-time check fails the release** if any GPU runtime library appears in the bundle.
- **A startup check verifies** that the running server sees no GPU device in CPU mode.
- **No GPU crates in Cargo.** This excludes `wgpu`, `ash`, and `nvml-wrapper`, so the app can't load GPU drivers even by accident.

## Build the CPU-only llama.cpp binary

`scripts/fetch-llama-server.ts` should build from the pinned source rather than download prebuilt binaries, because prebuilt releases include GPU backends. Use these CMake options (confirm the names against your pinned version):

```bash
cmake -B build \
  -DBUILD_SHARED_LIBS=OFF \
  -DGGML_CUDA=OFF \
  -DGGML_VULKAN=OFF \
  -DGGML_HIP=OFF \
  -DGGML_SYCL=OFF \
  -DGGML_OPENCL=OFF \
  -DGGML_METAL=OFF \
  -DLLAMA_CURL=OFF \
  -DGGML_NATIVE=OFF \
  -DGGML_CPU_ALL_VARIANTS=ON
cmake --build build --config Release --target llama-server
```

- `BUILD_SHARED_LIBS=OFF` links everything into one executable, so no extra DLLs are needed.
- `GGML_NATIVE=OFF` plus `GGML_CPU_ALL_VARIANTS=ON` builds several CPU instruction-set variants, so one binary runs on older laptops and uses AVX2 or AVX-512 where available. If your pinned version doesn't support these names, use a single portable build for now.
- `LLAMA_CURL=OFF` removes the model download code from llama-server. The app downloads models itself through the network gate, which keeps every outbound request in one place.

## Code

### `runtime/profile.rs` (replace `gpu_layers` with a device type)

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "device", rename_all = "snake_case")]
pub enum ComputeDevice {
    /// Default. No GPU is used or allocated.
    #[default]
    Cpu,
    /// Only reachable when GPU mode is enabled and a user-installed
    /// llama-server build has been selected.
    UserGpu { gpu_layers: u32 },
}

impl ComputeDevice {
    pub fn is_gpu(&self) -> bool {
        matches!(self, ComputeDevice::UserGpu { .. })
    }
}

// In InferenceProfile, replace `gpu_layers: u32` with:
//     pub device: ComputeDevice,

impl InferenceProfile {
    pub fn validate(&self) -> Result<(), String> {
        if self.threads == 0 {
            return Err("threads must be at least 1".into());
        }
        if self.context_tokens < 512 {
            return Err("context must be at least 512 tokens".into());
        }
        if self.kv_type != KvCacheType::F16 && !self.flash_attn {
            return Err("quantized KV cache requires flash attention".into());
        }
        if self.moe != MoeOffload::Off && !self.device.is_gpu() {
            return Err("expert offload requires GPU mode".into());
        }
        Ok(())
    }
}
```

### `runtime/args.rs` (CPU enforcement)

```rust
use super::profile::{ComputeDevice, MoeOffload};

// Inside build_launch, replace the -ngl line with:
let gpu_layers = match profile.device {
    ComputeDevice::Cpu => 0,
    ComputeDevice::UserGpu { gpu_layers } => gpu_layers,
};
args.push("-ngl".into());
args.push(gpu_layers.to_string());

// For CPU mode, also ask llama-server not to enumerate any device.
// Verify that `--device none` exists in your pinned version.
if matches!(profile.device, ComputeDevice::Cpu) {
    args.push("--device".into());
    args.push("none".into());
}

// Add to the env list so CUDA runtimes, if present on the user's system,
// are never initialized in CPU mode.
let mut env = vec![("LLAMA_ARG_API_KEY".into(), api_key.to_string())];
if matches!(profile.device, ComputeDevice::Cpu) {
    env.push(("CUDA_VISIBLE_DEVICES".into(), "".into()));
}
```

Replace the earlier `Ok(ServerLaunch { ... })` return with one that uses `env`. The MoE block from before stays unchanged, but `validate()` now rejects it in CPU mode.

### `runtime/backend.rs`

```rust
use std::path::PathBuf;

pub enum Backend {
    /// Shipped inside the installer. CPU only.
    BundledCpu { binary: PathBuf },
    /// Selected by the user in Settings. Used only when GPU mode is on.
    /// The app never downloads or bundles these.
    UserInstalled { binary: PathBuf },
}

impl Backend {
    pub fn binary(&self) -> &PathBuf {
        match self {
            Backend::BundledCpu { binary } | Backend::UserInstalled { binary } => binary,
        }
    }

    pub fn is_user_installed(&self) -> bool {
        matches!(self, Backend::UserInstalled { .. })
    }
}

/// The supervisor uses this to choose a binary. GPU mode without a
/// user-selected binary is an error, not a silent fallback to a
/// different build.
pub fn resolve(
    bundled_cpu: PathBuf,
    user_gpu_binary: Option<PathBuf>,
    gpu_mode_enabled: bool,
) -> Result<Backend, String> {
    if !gpu_mode_enabled {
        return Ok(Backend::BundledCpu { binary: bundled_cpu });
    }
    match user_gpu_binary {
        Some(binary) if binary.exists() => Ok(Backend::UserInstalled { binary }),
        Some(_) => Err("selected GPU llama-server binary was not found".into()),
        None => Err("GPU mode is on but no GPU llama-server build is selected".into()),
    }
}
```

### `runtime/guard.rs` (startup check)

```rust
use std::path::Path;
use std::process::Command;

use crate::runtime::profile::ComputeDevice;

/// Runs `llama-server --list-devices` and checks the result against the
/// selected mode. Verify the output format against your pinned version;
/// the parsing below assumes each device appears on its own line.
pub fn verify_device_mode(binary: &Path, device: &ComputeDevice) -> Result<(), String> {
    let output = Command::new(binary)
        .arg("--list-devices")
        .env("CUDA_VISIBLE_DEVICES", "")
        .output()
        .map_err(|e| format!("could not run llama-server: {e}"))?;

    let text = String::from_utf8_lossy(&output.stdout);
    let gpu_lines: Vec<&str> = text
        .lines()
        .filter(|l| {
            let l = l.to_lowercase();
            l.contains("cuda") || l.contains("vulkan") || l.contains("gpu")
        })
        .collect();

    match device {
        ComputeDevice::Cpu if !gpu_lines.is_empty() => Err(format!(
            "CPU mode, but llama-server reports GPU devices: {}",
            gpu_lines.join("; ")
        )),
        _ => Ok(()),
    }
}
```

Run this once per binary, and cache the result. If it fails in CPU mode, the app should refuse to start the server and show an error that names the binary.

### `hardware/gpu.rs` (remove from the default path)

The app no longer needs GPU detection to choose a mode. If you want a read-only hint in Settings, such as "a GPU was found, GPU mode is available," use OS-level queries that don't initialize a graphics API:

- **Windows:** DXGI adapter enumeration. It reports adapters without creating a device.
- **Linux:** read `/sys/class/drm/*/device/vendor` and the PCI class code.

Do not use Vulkan, CUDA, or `wgpu` for this, because initializing any of them can load the driver and allocate memory.

### `deny.toml` (cargo-deny)

```toml
[bans]
multiple-versions = "warn"
deny = [
  { name = "wgpu" },
  { name = "ash" },
  { name = "nvml-wrapper" },
  { name = "cudarc" },
  { name = "vulkano" },
]
```

This makes the ban enforceable in CI, not just a convention.

### `scripts/check-bundle.ts` (release gate)

```ts
import { readdirSync, statSync } from "node:fs";
import { join } from "node:path";

// Libraries that must never appear in the installer.
const FORBIDDEN = [
  /vulkan/i,          // vulkan-1.dll, libvulkan.so
  /^cudart/i,         // cudart64_*.dll
  /^cublas/i,         // cublas64_*.dll
  /^cublasLt/i,
  /nvcuda/i,          // nvcuda.dll (driver, should never be bundled)
  /libcuda/i,         // libcuda.so
  /libcudart/i,       // libcudart.so
  /ggml-cuda/i,
  /ggml-vulkan/i,
];

function walk(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? walk(path) : [path];
  });
}

const bundleDir = process.argv[2];
if (!bundleDir) {
  console.error("usage: check-bundle.ts <unpacked-bundle-dir>");
  process.exit(2);
}

const offenders = walk(bundleDir).filter((p) =>
  FORBIDDEN.some((re) => re.test(p.split(/[\\/]/).pop() ?? ""))
);

if (offenders.length > 0) {
  console.error("GPU libraries found in bundle:");
  for (const f of offenders) console.error("  " + f);
  process.exit(1);
}
console.log("bundle is CPU-only");
```

Also check the binary's own dependencies, since a static build can still link system libraries. On Windows, run `dumpbin /dependents` and on Linux run `ldd`, and fail if either lists a GPU library.

### `src-tauri/tauri.conf.json` (resources and webview)

```json
{
  "bundle": {
    "resources": [
      "resources/llama/cpu/*"
    ]
  },
  "app": {
    "windows": [
      {
        "label": "main",
        "title": "localAIse",
        "additionalBrowserArguments": "--disable-gpu"
      }
    ]
  }
}
```

Remove `resources/llama/*-vulkan` and `*-cuda` from the repository entirely.

`--disable-gpu` makes WebView2 render in software. That keeps the UI off the GPU, at a small cost in rendering smoothness. Make it a setting if you want users to choose, but default it to on to match your promise. On Linux, WebKitGTK's GPU compositing can be disabled with environment variables such as `WEBKIT_DISABLE_COMPOSITING_MODE=1`. Test the effect on your UI before shipping, because it can affect scrolling and animation.

## What changes for users

- Default performance is CPU-only, so generation is slower than GPU inference. The earlier guidance applies: 3–4B models on 16 GB laptops, and 7B on 32 GB machines.
- MoE expert placement (`--cpu-moe`, `--n-cpu-moe`) only does something in GPU mode, so it moves behind the GPU setting. On CPU, a MoE model still loads entirely into RAM, but generation is faster than a dense model of similar size because only the active experts are read per token.
- Users who want GPU speed can install a GPU build of llama.cpp themselves, point the app to it, and enable GPU mode. The app then reports exactly which device it's using.

## Tests

- The release build fails if `check-bundle` finds a GPU library.
- `cargo deny check bans` fails if a GPU crate is added.
- In CPU mode, `verify_device_mode` returns an error when a fake GPU line appears in test output.
- `validate()` rejects MoE offload without GPU mode.
- `build_launch` in CPU mode emits `-ngl 0` and never emits GPU device flags.
- On a clean Windows and Linux VM with no GPU drivers installed, the app starts, loads a model, and generates text.
- On a machine with a GPU, the process list and GPU usage monitor show no VRAM allocation during CPU-mode chat.

Once you've confirmed the flag names against your pinned llama.cpp version, the next step is to update `fetch-llama-server.ts` to build the CPU-only binary and add the bundle check to CI.
