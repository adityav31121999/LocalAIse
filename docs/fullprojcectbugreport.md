# LocalAIse — Full Project Bug Report

> Reviewed: all Rust backend modules, TypeScript build scripts, and `tauri.conf.json`.  
> Total bugs found: **18** across **14 files**.

---

## Legend

| Colour | Meaning |
|--------|---------|
| 🔴 Critical | Wrong data written, panic-path, or data loss |
| 🟠 High | Reliability failure, race condition, or security gap |
| 🟡 Medium | Logic error, degraded feature, or silent bad state |
| 🟢 Low | Minor correctness, style, or missing guard |

---

## 🔴 CRITICAL

---

### BUG-01 — `manifest.tag` used after manifest is consumed
**File:** [`runtime/gpu_build.rs`](file:///d:/LocalAIse/src-tauri/src/runtime/gpu_build.rs#L572-L594)  
**Lines:** L573, L587, L591

**Root cause:**  
`manifest` is consumed by `.into_iter().find(...)` at L446. Rust partially moves the struct, but the `manifest.tag` referenced at L573, L587, and L591 comes from the *consumed* manifest — not the entry's actual tag. If the manifest's top-level `tag` ever drifts from what an entry was installed under (e.g. during a manifest migration), `runtime.json` will record the **wrong tag**, causing `ensure_runtime` to endlessly re-stage the CPU files on every launch.

**Fix Plan:**
```rust
// Before the into_iter() call:
let manifest_tag = manifest.tag.clone();
let entry = manifest.entries.into_iter().find(...)?;

// Replace all 3 subsequent uses of manifest.tag with manifest_tag.
```

---

### BUG-02 — `analyze_document`: file path not sandboxed before reading
**File:** [`commands/research.rs`](file:///d:/LocalAIse/src-tauri/src/commands/research.rs#L54-L56)  
**Lines:** L54–56

**Root cause:**  
```rust
let target_path = root.join(&request.file_name);
let doc_text = parse_file(&target_path)...?;
```
`request.file_name` is a frontend-supplied string joined directly onto the project root with no path traversal check. A crafty `file_name` of `"../../state.json"` or `"../../models/secret.gguf"` would read arbitrary files outside the project folder.

**Fix Plan:**  
Apply `crate::sandbox::paths::ensure_within(&root, Path::new(&request.file_name))?` before building `target_path`, exactly as `commands/projects.rs` does for `read_document`.

---

### BUG-03 — `compare` action path not sandboxed either
**File:** [`commands/research.rs`](file:///d:/LocalAIse/src-tauri/src/commands/research.rs#L90-L93)  
**Lines:** L90–93

**Root cause:**  
The `compare` branch constructs `second_path` from `request.compare_with` the same way — with no `ensure_within` check. An attacker-controlled `compare_with` path traverses out of the project folder.

**Fix Plan:**  
Same as BUG-02: apply `ensure_within` to `second_name` before building `second_path`.

---

## 🟠 HIGH

---

### BUG-04 — `ensure_runtime` silently swallows CPU copy errors
**File:** [`runtime/gpu_build.rs`](file:///d:/LocalAIse/src-tauri/src/runtime/gpu_build.rs#L255-L265)  
**Lines:** L263

**Root cause:**  
```rust
let _ = std::fs::copy(&src, &dst);
```
Each file copy error inside the staging loop is silently discarded. If any bundled CPU binary fails to copy (disk full, permissions, AV quarantine), `ensure_runtime` returns `Ok(final_dir)` with an **incomplete runtime directory**. The app will then start without the required binary and show a cryptic "llama-server not found" error with no hint of the real cause.

**Fix Plan:**  
```rust
std::fs::copy(&src, &dst)
    .map_err(|e| AppError::msg(format!("failed to stage {}: {e}", src.display())))?;
```

---

### BUG-05 — Double `metadata().unwrap()` — TOCTOU & panic
**File:** [`runtime/gpu_build.rs`](file:///d:/LocalAIse/src-tauri/src/runtime/gpu_build.rs#L695)  
**Line:** L695

**Root cause:**  
```rust
emit_progress(app, target, "downloading",
    dest.metadata().unwrap().len(),   // syscall 1
    dest.metadata().unwrap().len());  // syscall 2 — identical result
```
Two separate `metadata()` syscalls with `.unwrap()`. On Windows with real-time antivirus, the file can be deleted between the two calls, causing a panic. Both values are always identical, making the second call wasted.

**Fix Plan:**  
```rust
let file_len = dest.metadata()
    .map_err(|e| AppError::msg(format!("cannot stat cached file: {e}")))?
    .len();
emit_progress(app, target, "downloading", file_len, file_len);
```

---

### BUG-06 — Watcher thread leaks forever — no shutdown signal
**File:** [`projects/watcher.rs`](file:///d:/LocalAIse/src-tauri/src/projects/watcher.rs#L20-L28)  
**Lines:** L20–28

**Root cause:**  
```rust
let _ = std::thread::Builder::new()
    .name("LocalAIse-watch".into())
    .spawn(move || loop {
        std::thread::sleep(SCAN_INTERVAL);
        watch_once(&handle);
    });
```
The watcher runs in an infinite loop with no stop signal, no channel, and no join handle saved. On `RunEvent::Exit` the `on_event` handler stops the supervisor, but the watcher thread keeps sleeping and calling `watch_once`. On Windows, OS process teardown will kill it, but on Linux/macOS the thread may hold a reference to the `AppHandle` long enough to cause a use-after-free or zombie state.

**Fix Plan:**  
Use a shared `Arc<AtomicBool>` stop-flag set in `on_event` at `RunEvent::Exit`, and check it at the top of the loop before calling `watch_once`.

---

### BUG-07 — `scan_sources` opens a new DB connection per file in the inner loop
**File:** [`projects/manager.rs`](file:///d:/LocalAIse/src-tauri/src/projects/manager.rs#L627-L633)  
**Lines:** L627–633

**Root cause:**  
```rust
let index_db = crate::index::db::IndexDb::new(&root);
let project_index = index_db.load();
let status = if project_index.indexed_files.contains_key(&name) { ...
```
`IndexDb::new` and `load()` are called **once per source file** inside a loop that may iterate up to 4,000 times. If the index is backed by SQLite, this opens and closes 4,000 connections per `scan_sources` call.

**Fix Plan:**  
Hoist the `IndexDb::new` + `index_db.load()` calls outside the walker loop (before `let mut files = Vec::new()`), then use the single loaded index inside the loop.

---

### BUG-08 — `check-bundle.ts`: `ggml-cuda` pattern also matches legitimate CPU DLLs
**File:** [`scripts/check-bundle.ts`](file:///d:/LocalAIse/scripts/check-bundle.ts#L13-L16)  
**Lines:** L13–16

**Root cause:**  
```ts
/ggml-cuda/i,
/ggml-vulkan/i,
/ggml-opencl/i,
```
The `ggml-cuda` pattern also matches `ggml-cuda-avx.dll`, `ggml-cuda-no-vmm.dll`, etc. — legitimate CPU dispatch DLLs with "cuda" in their name — causing false positives. Conversely, the pattern `/opencl/i` is broad enough to match any file whose path contains "opencl" (e.g. a vendor README named `opencl_notes.txt`).

**Fix Plan:**  
Anchor patterns to the filename only (already done with `.pop()?`) and tighten to exact backend filenames:
```ts
/^ggml-cuda\.dll$/i,
/^ggml-vulkan\.dll$/i,
/^ggml-opencl\.dll$/i,
/^nvcuda\.dll$/i,
/^libcuda\.so/i,
```
Remove the broad `/opencl/i` and `/vulkan/i` blanket patterns that also catch CPU SIMD variant DLLs.

---

## 🟡 MEDIUM

---

### BUG-09 — Redundant `invalidate` + `clear` on capability cache
**File:** [`commands/gpu.rs`](file:///d:/LocalAIse/src-tauri/src/commands/gpu.rs#L83-L86)  
**Lines:** L83–86

**Root cause:**  
```rust
cache.invalidate(&binary);   // removes one key
cache.clear();               // wipes the whole map — makes invalidate a no-op
```
The call to `invalidate` is dead code. `clear()` unconditionally removes everything anyway.

**Fix Plan:**  
Remove `cache.invalidate(&binary);` and keep only `cache.clear();`.

---

### BUG-10 — `probe_flash_attn` returns `Some(false)` for irrelevant "flash" mentions
**File:** [`runtime/backend.rs`](file:///d:/LocalAIse/src-tauri/src/runtime/backend.rs#L165-L178)  
**Lines:** L170–177

**Root cause:**  
```rust
if !help.contains("flash") {
    return None;   // not mentioned at all → unknown
}
Some(
    help.contains("flash-attn") || help.contains("flash_attn") || ...
)
```
If the help output contains "flash" (e.g. "flashable firmware", "flash storage") but none of the three sub-keywords, `Some(false)` is returned — telling the UI "this build does NOT support flash attention" when the reality is "we don't know."

**Fix Plan:**  
```rust
fn probe_flash_attn(binary: &Path) -> Option<bool> {
    let output = Command::new(binary).arg("--help").output().ok()?;
    let help = format!("{}\n{}", ...).to_lowercase();
    let has = help.contains("flash-attn")
        || help.contains("flash_attn")
        || help.contains("flash attention");
    if !has && !help.contains("flash") {
        return None; // build was asked but gave no hint
    }
    Some(has)
}
```

---

### BUG-11 — ICS `generate_ics` doesn't escape special characters in event fields
**File:** [`personal/calendar.rs`](file:///d:/LocalAIse/src-tauri/src/personal/calendar.rs#L106-L128)  
**Lines:** L110–123

**Root cause:**  
```rust
out.push_str(&format!("SUMMARY:{}\r\n", e.title));
out.push_str(&format!("LOCATION:{}\r\n", loc));
```
`SUMMARY`, `LOCATION`, and `UID` fields are written raw. A title or location containing a `:`, `;`, `\n`, or `\r\n` will produce **malformed ICS** that other calendar apps will refuse or misparse. RFC 5545 requires backslash-escaping `,`, `;`, `\`, and `\n` in these fields.

**Fix Plan:**  
Add an `ics_escape` function:
```rust
fn ics_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
     .replace(';', "\\;")
     .replace(',', "\\,")
     .replace('\n', "\\n")
     .replace('\r', "")
}
```
Apply it to `e.title`, `loc`, and any other text property before writing.

---

### BUG-12 — `parse_ics` assigns hardcoded `"2026-10-18"` fallback date to titleless events
**File:** [`personal/calendar.rs`](file:///d:/LocalAIse/src-tauri/src/personal/calendar.rs#L160)  
**Line:** L160

**Root cause:**  
```rust
start_at: if start_at.is_empty() { "2026-10-18".to_string() } else { start_at.clone() },
```
Any VEVENT that lacks a `DTSTART` line gets the hardcoded date `"2026-10-18"`. This date will eventually be in the past, making every malformed imported event silently appear as a years-old event. The fix was presumably meant as a placeholder but was never replaced.

**Fix Plan:**  
Fall back to `crate::util::time::now_iso()[..10]` (today's date) or return an error for events without a start date:
```rust
start_at: if start_at.is_empty() { 
    crate::util::time::now_iso()[..10].to_string() 
} else { 
    start_at.clone() 
},
```

---

### BUG-13 — `tauri.conf.json`: both platform resources bundled in every build
**File:** [`src-tauri/tauri.conf.json`](file:///d:/LocalAIse/src-tauri/tauri.conf.json#L33-L36)  
**Lines:** L34–35

**Root cause:**  
```json
"resources": [
    "../resources/llama/windows-x64/*",
    "../resources/llama/linux-x64/*"
]
```
Both platform folders are included in every build target, so a Windows NSIS installer ships Linux binaries and vice-versa. The Linux folder glob will silently be skipped if absent during a Windows build (Tauri ignores missing globs), but when both exist the installer is unnecessarily doubled in size.

**Fix Plan:**  
Point resources only at the `bundle-llama/<platform>/*` folder produced by `stage-bundle.ts` (which is already platform-filtered), or use two separate `tauri.conf.json` overrides per platform in CI.

---

### BUG-14 — `run_deep_research` has inconsistent line endings (mixed CRLF/LF)
**File:** [`commands/research.rs`](file:///d:/LocalAIse/src-tauri/src/commands/research.rs#L192-L207)  
**Lines:** L191–207

**Root cause:**  
Lines 1–191 of `research.rs` use `\r\n` (CRLF, as emitted by Windows text editors), but lines 192–207 use `\n` (LF). This means the file is **half-CRLF, half-LF**, which causes `git diff` noise and can confuse some parsers. The `.gitattributes` presumably normalises this on checkout but mixed endings inside one file still indicate a commit error.

**Fix Plan:**  
Normalise the file to a single line ending style. Set `.gitattributes` rule `*.rs text=auto eol=lf` and re-commit.

---

### BUG-15 — `set_diary` uses immutable `registry` borrow but writes a change
**File:** [`commands/projects.rs`](file:///d:/LocalAIse/src-tauri/src/commands/projects.rs#L222-L225) & [`projects/manager.rs`](file:///d:/LocalAIse/src-tauri/src/projects/manager.rs#L482-L487)  

**Root cause:**  
`set_diary` in `manager.rs` takes `registry: &Registry` (immutable). It calls `paths.write(&file)?` to update `project.json` on disk, but it **never calls `registry.upsert(updated)` or `registry.save()`**. The `diary_enabled` toggle is written to the project file but not persisted to `state.json`. If the project is re-registered (e.g. after a restart with a stale `state.json`), the registry will not reflect the diary toggle.

**Fix Plan:**  
Change `set_diary`'s signature to take `registry: &mut Registry, dirs: &AppDirs` (matching the other setters), persist the updated record, and save the registry exactly as `set_chat_mode` does.

---

## 🟢 LOW

---

### BUG-16 — `stage-bundle.ts`: subdirectories silently skipped without warning
**File:** [`scripts/stage-bundle.ts`](file:///d:/LocalAIse/scripts/stage-bundle.ts#L57-L60)  
**Lines:** L57–60

**Root cause:**  
```ts
if (statSync(src).isFile()) {
    copyFileSync(src, dst);
    count++;
}
// Directories silently ignored
```
If a future llama.cpp release ships resources in subdirectories (shader caches, locale files), they are silently dropped from the bundle with no diagnostic.

**Fix Plan:**  
```ts
} else if (statSync(src).isDirectory()) {
    console.warn(`[stage-bundle] Skipping directory: ${file}`);
}
```

---

### BUG-17 — `matches_any_glob` `*suffix` semantics undocumented and untested
**File:** [`runtime/gpu_build.rs`](file:///d:/LocalAIse/src-tauri/src/runtime/gpu_build.rs#L661-L680)  
**Lines:** L670–674

**Root cause:**  
For patterns like `"*cuda.dll"`, `parts[0]` is `""` and `lower_name.starts_with("")` is always `true`, making the pattern a pure suffix match. This is probably intentional but is not documented or tested. A typo like `"*"` (single wildcard) would match every filename.

**Fix Plan:**  
Add unit tests and a comment:
```rust
// "*suffix" patterns: prefix is empty, so only the suffix is checked.
assert!(matches_any_glob("ggml-cuda.dll", &["*cuda.dll".into()]));
assert!(!matches_any_glob("ggml-vulkan.dll", &["*cuda.dll".into()]));
assert!(!matches_any_glob("anything", &["*".into()])); // pure * should not match everything
```

---

### BUG-18 — `client.rs:complete` has a formatting artifact (missing newline before `let http`)
**File:** [`runtime/client.rs`](file:///d:/LocalAIse/src-tauri/src/runtime/client.rs#L96)  
**Line:** L96

**Root cause:**  
```rust
pub async fn complete(serving: &Serving, model: &str, messages: &[ChatMessage]) -> AppResult<String> {    let http = client(RESPONSE_TIMEOUT)?;
```
The function body starts on the same line as the signature (a stray join from a prior edit). This compiles fine but is a formatting error that `rustfmt` will rewrite differently, and it will appear as a diff whenever `cargo fmt` is run.

**Fix Plan:**  
Insert a newline so the body opens on L97 as `    let http = client(RESPONSE_TIMEOUT)?;`.

---

## Summary Table

| ID | Severity | File | Short Description |
|----|----------|------|-------------------|
| 01 | 🔴 Critical | `runtime/gpu_build.rs` | Consumed `manifest.tag` written to wrong metadata |
| 02 | 🔴 Critical | `commands/research.rs` | `analyze_document` file path not sandboxed |
| 03 | 🔴 Critical | `commands/research.rs` | `compare` second file path not sandboxed |
| 04 | 🟠 High | `runtime/gpu_build.rs` | CPU copy errors silently swallowed in `ensure_runtime` |
| 05 | 🟠 High | `runtime/gpu_build.rs` | Double `.unwrap()` on `metadata()` — panic & TOCTOU |
| 06 | 🟠 High | `projects/watcher.rs` | Watcher thread has no shutdown path |
| 07 | 🟠 High | `projects/manager.rs` | DB opened per file in `scan_sources` (≤4000x) |
| 08 | 🟠 High | `scripts/check-bundle.ts` | GPU pattern matches CPU variant DLLs — false positives |
| 09 | 🟡 Medium | `commands/gpu.rs` | Redundant `invalidate` before `clear` |
| 10 | 🟡 Medium | `runtime/backend.rs` | `probe_flash_attn` returns wrong tri-state |
| 11 | 🟡 Medium | `personal/calendar.rs` | ICS special chars not escaped — malformed output |
| 12 | 🟡 Medium | `personal/calendar.rs` | Hardcoded `"2026-10-18"` fallback date in ICS parser |
| 13 | 🟡 Medium | `tauri.conf.json` | Both platform resources bundled in every installer |
| 14 | 🟡 Medium | `commands/research.rs` | Mixed CRLF/LF line endings in same file |
| 15 | 🟡 Medium | `projects/manager.rs` | `set_diary` skips `registry.save()` — toggle not persisted |
| 16 | 🟢 Low | `scripts/stage-bundle.ts` | Subdirectories silently skipped |
| 17 | 🟢 Low | `runtime/gpu_build.rs` | Glob `*suffix` semantics undocumented |
| 18 | 🟢 Low | `runtime/client.rs` | Missing newline before function body |

---

> **Priority recommendation:** Fix BUG-02 and BUG-03 first (path traversal / security), then BUG-01 (tag corruption), then BUG-15 (silent data loss on diary toggle), then the reliability issues BUG-04–07 before next release.
