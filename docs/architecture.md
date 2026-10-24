# Architecture

localAIse is a Tauri 2 desktop app. The web UI owns appearance and interaction
only; the Rust core owns every file, process and network call. One process, one
SQLite-free per-project index, and one supervised llama-server per session.

## Layering

```
React UI (src/)          state, rendering, keyboard. No filesystem contact.
      │  Tauri IPC
Rust commands            thin entry points, input validation only.
      │
 Domain modules: runtime supervisor, projects, ingest, retrieval,
                 agent loop, personal space, activity log
      │
 llama-server (child process, localhost)    per-project app state on disk
```

Rules that keep the layering honest:

- `src-tauri/src/commands/` holds `#[tauri::command]` functions only; each one
  delegates to a domain module. Business logic stays testable without a window.
- Outbound HTTP exists in exactly two places: `network/` for project web
  access (gated by the per-project toggle) and `models/download.rs` for
  user-initiated model downloads. Both audit to the activity log.
- The sandbox (`sandbox/`) is separate from the tools (`tools/`) so path rules
  are defined once for every caller.

## Inference stack

`runtime/` owns the llama-server process:

- `profile.rs` — the complete flag surface as a validated struct: context,
  KV-cache quantization (`f16/q8_0/q4_0` via `-ctk`/`-ctv`), flash attention,
  threads, batching, mmap, `-ngl` GPU layers, and MoE expert placement
  (`--cpu-moe`, `--n-cpu-moe`) behind an explicit GPU-mode opt-in.
- `args.rs` — the only place llama-server flags are spelled out. The API key
  travels via `LLAMA_API_KEY` in the environment, never argv.
- `budget.rs` — KV-cache and weights math, read from GGUF metadata
  (`models/shape.rs`), so the app can recommend a context length that actually
  fits before the server starts.
- `guard.rs` — `--list-devices` cross-check: in CPU mode the server must see
  no GPU device.
- `supervisor.rs` — spawn, health check, restart on profile change, and a
  clean shutdown on app exit.

The default build of llama-server shipped with localAIse is CPU-only, pinned to
llama.cpp v0.7.0 (`scripts/fetch-llama-server.ts`). Since v0.7.0 the CPU dispatch
tiers ship as runtime libraries (`ggml-cpu-<variant>.dll` next to the binary,
loaded by ggml's backend registry), so the bundle directory copies the exe plus
its `ggml*.dll` runtime set — still with every GPU backend compiled out. Vulkan
and CUDA builds are never bundled.

GPU builds come in two user-initiated ways:

- `runtime/gpu_build.rs` — the hardware settings can install the official
  prebuilt llama.cpp archive that matches the machine's detected card (CUDA on
  NVIDIA, Vulkan elsewhere), from ggml-org's release, SHA-256-checked against
  the digest pinned in `src-tauri/backends.json` (upstream publishes no
  checksum files) and recorded in a `build-info.json` manifest. Installs land
  flat in the app-data `llama/` folder next to the CPU build — one shared
  platform folder, with only the backend's own libraries added. There is no
  official OpenCL archive, so OpenCL comes only from a local build.
- `scripts/fetch-llama-server.ts windows-x64-|linux-x64-{vulkan,cuda,opencl}`
  — local builds from the vendored checkout for machines the in-app install
  cannot serve (no official archive, exotic toolchain, CUDA version pinned).

- `backend.rs` — resolves the build per device: a CPU run uses the CPU build,
  a GPU-eligible run prefers an installed GPU build. `Auto` settles against
  the build's own `--list-devices` answer and hands the run back to the CPU
  build when nothing fits in VRAM.
- `args.rs` — the only place llama-server flags are spelled out. The API key
  travels via `LLAMA_API_KEY` in the environment, never argv. A CPU run pins
  every backend's device list empty (`--device none`,
  `CUDA_VISIBLE_DEVICES`, `GGML_VK_VISIBLE_DEVICES`,
  `GGML_OPENCL_VISIBLE_DEVICES`), so no build mode touches VRAM.

## Storage

Per-project content lives directly in the project folder: `project.json`
(the manifest), `data/` (sources), `chats/` (one JSON file per session),
`diary/` (one Markdown page per day, opt-in), `outputs/`, plus any folder the
user creates. Only app state sits under `.LocalAIse/` (`index.json`,
`activity.log`); deleting that folder resets the app's view of the project
without touching user files. The personal space (diary, calendar.ics) stays in
a folder of its own. Global settings and the model registry live in the
platform app-data folder. Formats are documented in
[project-format.md](./project-format.md).

## Retrieval

`ingest/` parses supported formats (PDF, DOCX, Markdown, TXT, CSV, XLSX),
chunks them with overlap, and stores chunks per project. `retrieval/` scores
chunks against the question and produces citations (file + excerpt) that the
UI attaches to every answer. The embedding-manifest resource defines the
optional local embedding model for stronger matching.

## Events

The Rust side emits events to the UI instead of polling:

- `chat://token` — streaming deltas per project while an answer generates. Deltas
  with `reasoning: true` belong to the model's thinking trace, not the answer.
- `model-download` — progress for downloads from the catalog.
