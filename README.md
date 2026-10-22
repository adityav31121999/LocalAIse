# localAIse

A private, local AI workspace. Chat with your documents, run a research agent and keep a
personal diary with reminders — with everything running on your own machine, no internet
required, and no data leaving the folder you point it at.

## How it works

- **Tauri 2 app** — a Rust core (`src-tauri`) with a React UI (`src`). No Node server runs at
  runtime; the UI talks to Rust over IPC.
- **llama.cpp runtime** — dynamic backend dispatch architecture pinned to
  llama.cpp nightly build tag **b6742** (`vendor/llama.cpp`, `resources/llama/<platform>`). The server
  launches per selected model with an API key, streaming tokens to the UI over the `chat://token`
  event.
- **Dynamic Hardware & Backend Loading** — the base bundle ships a universal CPU build with
  runtime microarchitecture dispatch tiers (SSE4.2, AVX2, AVX-512 variants chosen by `cpuid` at
  startup). The runtime non-intrusively detects the host GPU vendor (CUDA for NVIDIA, Vulkan for
  AMD/Intel, OpenCL as fallback) and downloads the matching backend on first launch or from
  Hardware settings. All backends share **one flat platform folder** (`resources/llama/windows-x64`
  or `resources/llama/linux-x64`): GPU backends drop their `ggml-<backend>.dll` next to the CPU
  files and are discovered automatically at startup. See `docs/backend.md` for the full
  architecture.

## Features

- **Projects** — point localAIse at any folder; files inside are parsed and indexed into a
  per-project JSON index, with a background watcher re-indexing changed files every 15 s.
  Each project is plain folders — `data/` for sources, `chats/` for sessions, `diary/` for
  daily pages, `outputs/` for exports, plus any folder you create — described by a
  `project.json` manifest at the root (see `docs/project-format.md`).
- **Cited chat** — answers stream in token by token with source citations that answer only
  from the project index (BM25-style keyword retrieval). Every conversation is saved as
  JSON in `chats/` and restored when you reopen the project; "New chat" starts a fresh
  session with a completely fresh model cache.
- **Per-project system prompt** — the chat's system instructions can be customised for each
  project from the composer; an empty prompt keeps the built-in privacy default.
- **Project diary** — opt in per project to get a `diary/` folder of Markdown pages, one
  file per date; the same one-page-per-day format is used by the personal-space diary.
- **Research agent** — a plan/report agent that walks your indexed sources; web access is
  per-project and off by default, and every network call is approved and logged.
- **Personal space** — diary pages, calendar events, reminders and a natural-language search
  across all of it.
- **Model manager** — installed GGUF models plus a curated catalog with SHA-friendly,
  progress-tracked downloads. Selecting a model or profile relaunches the server cleanly.
- **Inference profiles** — context window, KV-cache quantization (`f16`/`q8_0`/`q4_0` with
  automatic flash attention), thread count (auto = one fewer than your logical cores), batch
  size and slots; every profile is validated in Rust before it reaches llama-server. MoE
  models support `--cpu-moe` / `--n-cpu-moe` expert offload, and GPU offload (`-ngl`,
  `--n-cpu-moe` layer splits) unlocks through a GPU build: installed from the official
  llama.cpp release in the Hardware settings, or a build you point the app at.
- **Themes** — six visual themes, each with light/dark mode and three densities, all editable
  purely through CSS layers in `src/styles`.
- **Activity log** — every tool call, network request and index write is recorded per project.

## Getting started

Prerequisites: Node 20+, Rust (stable, MSVC toolchain on Windows), CMake for building the
bundled llama-server.

```powershell
npm install
npm run fetch-llama-server                            # CPU build for the current platform
npm run fetch-llama-server -- windows-x64 cuda        # add CUDA backend (NVIDIA)
npm run fetch-llama-server -- windows-x64 vulkan      # add Vulkan backend (AMD/Intel)
npm run fetch-llama-server -- windows-x64 opencl      # add OpenCL backend
npm run fetch-llama-server -- linux-x64 cuda          # CUDA for Linux
npm run verify-binaries      # SHA-256 checks the bundled binaries (see scripts/verify-binaries.ts)
npm run dev                  # UI-only dev mode (fixtures, no Rust core)
npm start                    # cargo tauri dev — full desktop app while developing
npm run build                # typecheck + production UI bundle
npm run tauri build          # desktop installer (NSIS / AppImage / deb)
```

All backends for a platform share the same flat output folder (`resources/llama/windows-x64` or
`resources/llama/linux-x64`). Installing a GPU backend adds only its `ggml-<backend>.dll` and
any runtime DLLs alongside the existing CPU files — no separate subdirectories.

The model manager can download quantized GGUF models for you; on first run the app walks
through onboarding: welcome → hardware check → starter model → personal folder.

## Layout

| Path | Contents |
| --- | --- |
| `src-tauri` | Rust core: commands, retrieval index, supervisor, tool sandbox, activity log |
| `src` | React UI: stores, API wrappers, components, theme CSS layers |
| `docs` | `firstplan.md`, `architecture.md`, `backend.md`, `security-model.md`, `project-format.md`, `model-catalog.md`, `phases.md` |
| `.github/workflows` | CI (check, clippy, tests, deny, bundle scan) and release workflow |
| `scripts` | `fetch-llama-server.ts`, `verify-binaries.ts`, `check-bundle.ts` |

## Security model

The app's network surface is the model catalog download and, per project and only with your
approval, the research agent's web tools. File tools are sandboxed to the open project plus
your personal folder; path escapes are rejected in Rust, not the UI. See
`docs/security-model.md` for the details.
