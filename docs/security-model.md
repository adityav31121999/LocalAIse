# Security model

The product promise: **your files, notes and research stay on your machine.**
Everything here exists to make that promise load-bearing rather than cosmetic.

## Network control

- Every project starts with internet **off**. Requests from any agent tool go
  through `network::gate`, which refuses them unless the project's toggle is
  on, and writes time, URL, purpose and choosing tool to `activity.log`.
- `network::policy` blocks private ranges (SSRF shield) and non-HTTP schemes
  regardless of the toggle. Only the exact llama-server loopback host is
  always allowed.
- Model downloads are the one other outbound path: user-initiated, catalog-
  listed, checksummed, and never bundled with a web tool.
- The webview gets no general network access; the CSP allows only `self` and
  the IPC bridge.

## Sandboxing

- File tools can only act inside the active project folder. `sandbox::paths`
  validates every path component by component, refuses traversal and absolute
  escapes, and follows symlinks to their real location before approving.
- Agent writes land in `outputs/` so user files are never silently changed;
  overwrites and deletions pass `sandbox::confirm`, which refuses until the
  caller confirms on the user's behalf.
- The web view never receives open filesystem access — every read and write
  crosses a Tauri command that validates first.

## Inference isolation

- llama-server runs as a child process bound to `127.0.0.1` on a random port,
  with a random per-session token delivered by environment variable. There is
  no LAN listening port in any mode.
- The app **bundles** a CPU-only llama-server build — the installers never
  carry GPU runtimes. GPU inference is an explicit, user-initiated install:
  the hardware settings can download the official prebuilt llama.cpp build
  for the card this machine reports (CUDA on NVIDIA, Vulkan elsewhere),
  taking the archive from ggml-org's release, checking its SHA-256 against
  the digest pinned in `src-tauri/backends.json` and recording the story in
  `build-info.json` next to the build. (Upstream publishes no checksum
  files for these archives, so the app pins digests itself; the pinned tag
  is covered by `docs/backend.md`'s re-audit checklist.) These installs live
  in the app's data dir, never inside the installers, and the release bundle
  stays scanned by `scripts/check-bundle.ts`. GPU programs (`wgpu`, `ash`,
  `nvml-wrapper`, `cudarc`, `vulkano`) stay banned in the app's own code
  (`deny.toml`).
- CPU-only runs pin zero devices explicitly: the launcher passes
  `--device none` plus empty `CUDA_VISIBLE_DEVICES`,
  `GGML_VK_VISIBLE_DEVICES` and `GGML_OPENCL_VISIBLE_DEVICES`, so on any
  installed build — CPU or GPU — a **CPU run sees no GPU initialization —
  zero VRAM use by default**. GPU offload needs an explicit device setting.
- The webview renders with `--disable-gpu` (software compositing) to keep the
  UI off the GPU as well.

## Prompt injection

Document content is data. The system prompt and the user's messages carry the
instructions; text pulled from sources, web pages, or tool outputs is shown in
labeled context blocks rather than executed as directives. Tool calls that
would modify state require approval before they run.

## Data at rest

- Projects and the personal space are plain folders and files the user owns;
  nothing is encrypted today and nothing invisible is added. Sources, chats,
  diary pages and outputs are ordinary files in the project folder; deleting
  `project.json` and `.LocalAIse/` forgets the app's view of a project without
  touching the user's files.
- Encryption at rest (passphrase-protected personal space) is planned for v2
  precisely because it must not depend on a server.

## Telemetry

None. If crash reporting ever lands it will be opt-in, and this file will
document exactly what leaves the machine.

## Threat notes

| Threat | Mitigation in place |
|---|---|
| Agent writes outside the project | component-validated joins, `outputs/` policy, tests in `tests/sandbox_tests.rs` |
| Toggle "off" but requests leave anyway | single choke point (`gate::fetch_url`), policy unit tests, audit log written even for refused requests' attempts |
| GPU runtime leaks in | cargo-deny bans, bundle scan, `--device none`, guard's `--list-devices` cross-check |
| Model files substituted | SHA-256 recorded at build (`verify-binaries.ts`), enforced in release CI |
| API key exposure | token generated per session, passed by env, never logged |
