# LocalAIse — Bug Report & Fix Plan

Files reviewed:
- [`lib.rs`](file:///d:/LocalAIse/src-tauri/src/lib.rs)
- [`config/paths.rs`](file:///d:/LocalAIse/src-tauri/src/config/paths.rs)
- [`commands/gpu.rs`](file:///d:/LocalAIse/src-tauri/src/commands/gpu.rs)
- [`runtime/gpu_build.rs`](file:///d:/LocalAIse/src-tauri/src/runtime/gpu_build.rs)
- [`runtime/backend.rs`](file:///d:/LocalAIse/src-tauri/src/runtime/backend.rs)
- [`scripts/stage-bundle.ts`](file:///d:/LocalAIse/scripts/stage-bundle.ts)
- [`tauri.conf.json`](file:///d:/LocalAIse/src-tauri/tauri.conf.json)

---

## Bug 1 — `install_gpu_build` uses stale `manifest.tag` after `load_manifest()` is moved (gpu_build.rs L572–593) 🔴 Logic

**Location:** [`gpu_build.rs` L573, L587, L591](file:///d:/LocalAIse/src-tauri/src/runtime/gpu_build.rs#L572-L594)

**Problem:**  
`manifest` is consumed by `into_iter().find(...)` at L446, so `manifest.tag` is moved. The code later references `manifest.tag` again when constructing `BuildSource` (L573) and `RuntimeMetadata` fallbacks (L587 & L591). In Rust this compiles only because `entry` borrows individual fields and `manifest.tag` is a `String` that was only partially moved — **but the `manifest` variable itself is consumed**. The `tag` on the `RuntimeMetadata` fallbacks (L587, L591) comes from the *consumed* manifest, while the `entry` already holds the real tag. This creates an inconsistency: if the manifest's top-level `tag` differs from what the entry was installed under, the wrong tag is written to `runtime.json`.

**Fix Plan:**
1. Extract `manifest.tag` into a local `String` *before* calling `into_iter()`:
```rust
let manifest_tag = manifest.tag.clone();
let entry = manifest.entries.into_iter().find(...)?;
```
2. Replace all three subsequent uses of `manifest.tag` with `manifest_tag`.

---

## Bug 2 — Double `invalidate` + `clear` on the capability cache is redundant but can mask future bugs (gpu.rs L83–86) 🟡 Logic

**Location:** [`commands/gpu.rs` L83–86](file:///d:/LocalAIse/src-tauri/src/commands/gpu.rs#L83-L86)

**Problem:**  
After a successful GPU install, `cache.invalidate(&binary)` is called and then immediately `cache.clear()`. `clear()` empties the *entire* map, making the preceding `invalidate` a no-op. This is harmless today, but if `invalidate` ever has side effects (e.g., persisting state) the redundant call can cause subtle bugs, and the intent ("only invalidate the new binary") is at odds with what's actually done ("wipe everything").

**Fix Plan:**
- Decide on intent and remove one call:
  - If the goal is to invalidate **only** the newly installed binary, keep `invalidate` and remove `clear`.
  - If the goal is to flush **all** stale probes (e.g., a GPU build may change multiple binary behaviour), keep `clear` and remove `invalidate`.
- The current GPU architecture uses a single flat runtime dir, so clearing everything is the safer default. Recommend **removing `invalidate` and keeping `clear`** only.

```diff
-        cache.invalidate(&binary);
         cache.clear();
```

---

## Bug 3 — `ensure_runtime` silently swallows CPU copy errors (gpu_build.rs L263) 🟡 Reliability

**Location:** [`gpu_build.rs` L263](file:///d:/LocalAIse/src-tauri/src/runtime/gpu_build.rs#L255-L265)

**Problem:**  
Individual file copy errors inside the source-dir loop are suppressed with `let _ = std::fs::copy(...)`. If a bundled CPU binary fails to copy (permissions, disk full, etc.) the function still returns `Ok(final_dir)`, the runtime directory will be incomplete, and the user will see a cryptic "llama-server binary not found" error much later.

**Fix Plan:**
1. Propagate the copy error immediately:
```rust
std::fs::copy(&src, &dst)
    .map_err(|e| AppError::msg(format!("failed to copy {} to runtime: {e}", src.display())))?;
```
2. Alternatively, collect errors and surface them at the end of the loop so the first failure is still reported.

---

## Bug 4 — `download_resumable` calls `dest.metadata()` twice, risking a TOCTOU race (gpu_build.rs L695) 🟡 Race Condition

**Location:** [`gpu_build.rs` L695](file:///d:/LocalAIse/src-tauri/src/runtime/gpu_build.rs#L692-L697)

**Problem:**  
```rust
emit_progress(app, target, "downloading",
    dest.metadata().unwrap().len(),   // first call
    dest.metadata().unwrap().len());  // second call (identical)
```
Two separate `metadata()` syscalls with `.unwrap()` between a hash-check pass and a `return Ok(())`. If the file is deleted between the two calls (unlikely but possible on Windows with antivirus), either call panics. Both values are always identical, making the second call wasted work.

**Fix Plan:**
```rust
let file_len = dest.metadata()
    .map_err(|e| AppError::msg(format!("cannot stat cached file: {e}")))?
    .len();
emit_progress(app, target, "downloading", file_len, file_len);
```

---

## Bug 5 — `probe_flash_attn` returns `None` when "flash" appears but neither sub-keyword does (backend.rs L170–177) 🟡 Logic

**Location:** [`backend.rs` L170–177](file:///d:/LocalAIse/src-tauri/src/runtime/backend.rs#L165-L178)

**Problem:**  
```rust
if !help.contains("flash") {
    return None;   // "flash" not mentioned at all → unknown
}
Some(
    help.contains("flash-attn")
    || help.contains("flash_attn")
    || help.contains("flash attention"),
)
```
If the help output mentions "flash" (e.g., "flashable firmware") but none of the three sub-keywords, the function returns `Some(false)` — meaning "the build was probed and flash-attn is NOT supported". This is wrong; it should return `None` (unknown) or be tightened to match only the expected pattern.

**Fix Plan:**
```rust
fn probe_flash_attn(binary: &Path) -> Option<bool> {
    let output = Command::new(binary).arg("--help").output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let help = format!("{stdout}\n{stderr}").to_lowercase();
    // Check for known flash-attention flag patterns
    let has_flash_attn = help.contains("flash-attn")
        || help.contains("flash_attn")
        || help.contains("flash attention");
    // If none of the specific patterns are found, return None (unknown)
    if !help.contains("flash") {
        return None;
    }
    Some(has_flash_attn)
}
```

---

## Bug 6 — `tauri.conf.json` bundles both Windows and Linux resources for every platform (tauri.conf.json L34–35) 🟡 Packaging

**Location:** [`tauri.conf.json` L33–36](file:///d:/LocalAIse/src-tauri/tauri.conf.json#L33-L36)

**Problem:**  
```json
"resources": [
    "../resources/llama/windows-x64/*",
    "../resources/llama/linux-x64/*"
]
```
Both platform folders are included in **every** build target. A Windows NSIS installer will bundle the Linux binaries and vice-versa, bloating the installer unnecessarily. On a platform where one of the folders doesn't exist, Tauri may error or warn at build time.

**Fix Plan:**  
Use Tauri's per-target `bundle.resources` override (Tauri v2 supports conditional resources per target). At minimum, split into two separate build configurations or CI jobs that set the resource glob per platform:
```json
// In CI: set TAURI_RESOURCES env or use platform-specific tauri.conf override
"resources": ["../resources/llama/windows-x64/*"]  // Windows build
"resources": ["../resources/llama/linux-x64/*"]    // Linux build
```
Alternatively, gate resources using the `stage-bundle.ts` script (already in the repo) and change tauri.conf.json to only reference `bundle-llama/<platform>/*` which is already filtered by the script.

---

## Bug 7 — `stage-bundle.ts` silently skips subdirectories (stage-bundle.ts L57–60) 🟢 Minor

**Location:** [`stage-bundle.ts` L57–60](file:///d:/LocalAIse/scripts/stage-bundle.ts#L55-L61)

**Problem:**  
The staging loop calls `statSync(src).isFile()` and only copies files, silently skipping any subdirectories in the source folder. If future llama.cpp releases ship resources in subdirectories (e.g., locale files, shader caches), they will be silently dropped without any warning.

**Fix Plan:**  
Add a log line when a directory entry is encountered:
```typescript
if (statSync(src).isFile()) {
    copyFileSync(src, dst);
    count++;
} else if (statSync(src).isDirectory()) {
    console.warn(`[stage-bundle] Skipping directory (not recursing): ${file}`);
}
```
Or implement recursive copying if subdirectory support is needed.

---

## Bug 8 — `matches_any_glob` mishandles patterns with `*` in the middle when suffix is empty (gpu_build.rs L672) 🟢 Minor

**Location:** [`gpu_build.rs` L672](file:///d:/LocalAIse/src-tauri/src/runtime/gpu_build.rs#L661-L680)

**Problem:**  
For a pattern like `"ggml-cuda*"` (trailing `*`), the `ends_with('*')` branch handles it correctly. But for a pattern like `"ggml-*-cuda"`, the split-on-`*` branch checks `parts.len() == 2` — that's fine — but if `parts[1]` is `""` (i.e., the `*` is truly at the end and the first branch missed it because `ends_with('*')` already short-circuits), the logic is correct. **However**, the check `lower_name.ends_with(parts[1])` with `parts[1] == ""` always returns `true`, so `"ggml-foo-cuda"` would match `"ggml-*-cuda"` correctly, but `"anything-at-all"` with prefix `""` and suffix `"cuda"` would also match `"*cuda"` patterns — the prefix check `starts_with(parts[0])` where `parts[0] == ""` is always true. This means `"*cuda"` matches any filename ending in `"cuda"`, which may be the intended behaviour, but is worth confirming.

**Fix Plan:**  
This is a design ambiguity, not a crash bug. Add a comment or unit test that explicitly documents the expected behaviour for `"*suffix"` patterns. Add a test case:
```rust
assert!(matches_any_glob("ggml-cuda.dll", &["*cuda.dll".into()]));
assert!(!matches_any_glob("ggml-vulkan.dll", &["*cuda.dll".into()]));
```

---

## Summary Table

| # | Severity | File | Issue |
|---|----------|------|-------|
| 1 | 🔴 Logic | `gpu_build.rs` | Moved `manifest.tag` written to wrong metadata locations |
| 2 | 🟡 Logic | `commands/gpu.rs` | Redundant `invalidate` + `clear` on capability cache |
| 3 | 🟡 Reliability | `gpu_build.rs` | CPU file copy errors silently swallowed in `ensure_runtime` |
| 4 | 🟡 Race/Panic | `gpu_build.rs` | Double `.unwrap()` on `metadata()` with TOCTOU risk |
| 5 | 🟡 Logic | `backend.rs` | `probe_flash_attn` returns `Some(false)` for irrelevant "flash" mentions |
| 6 | 🟡 Packaging | `tauri.conf.json` | Both platform resources bundled in every build |
| 7 | 🟢 Minor | `stage-bundle.ts` | Subdirectories silently skipped with no warning |
| 8 | 🟢 Minor | `gpu_build.rs` | Glob `*suffix` matching semantics undocumented / untested |
