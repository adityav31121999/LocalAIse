//! Installing a llama-server build with GPU support.
//!
//! The app ships a CPU-only build; a GPU build is an explicit, user-initiated
//! install. This module manages runtime directory resolution, backend downloading,
//! verifying against SHA-256 digests, and dynamic switching.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};

use crate::config::paths::AppDirs;
use crate::error::{AppError, AppResult};
use crate::hardware::gpu::GpuBackend;

/// Pinned llama.cpp tag.
pub const PINNED_TAG: &str = "b11678";

/// Event the settings and onboarding pages listen to during install.
pub const PROGRESS_EVENT: &str = "backend://progress";

static CANCEL_FLAG: AtomicBool = AtomicBool::new(false);

pub fn cancel_current_install() {
    CANCEL_FLAG.store(true, Ordering::SeqCst);
}

pub fn reset_cancel_flag() {
    CANCEL_FLAG.store(false, Ordering::SeqCst);
}

pub fn is_install_cancelled() -> bool {
    CANCEL_FLAG.load(Ordering::SeqCst)
}

/// The backend targets the runtime can resolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GpuTarget {
    Vulkan,
    Cuda,
    Opencl,
}

impl GpuTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            GpuTarget::Vulkan => "vulkan",
            GpuTarget::Cuda => "cuda",
            GpuTarget::Opencl => "opencl",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            GpuTarget::Vulkan => "Vulkan",
            GpuTarget::Cuda => "CUDA",
            GpuTarget::Opencl => "OpenCL",
        }
    }

    pub fn parse(value: &str) -> Option<GpuTarget> {
        match value.to_lowercase().as_str() {
            "vulkan" => Some(GpuTarget::Vulkan),
            "cuda" => Some(GpuTarget::Cuda),
            "opencl" => Some(GpuTarget::Opencl),
            _ => None,
        }
    }

    pub fn downloadable(self) -> bool {
        !matches!(self, GpuTarget::Opencl)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuBuildStatus {
    pub target: String,
    pub label: String,
    pub installed: bool,
    pub source: Option<BuildSource>,
    pub downloadable: bool,
    pub directory: String,
    pub recommended: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BuildSource {
    pub tag: String,
    pub platform: String,
    pub backend: String,
    pub installed_files: Vec<String>,
    pub installed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeMetadata {
    pub tag: String,
    pub installed_backends: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BackendManifest {
    pub tag: String,
    pub entries: Vec<BackendEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BackendEntry {
    pub platform: String,
    pub backend: String,
    pub files: Vec<ManifestFile>,
    pub keep: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestFile {
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

pub fn recommend(backend: &GpuBackend) -> Option<GpuTarget> {
    match backend {
        GpuBackend::Cuda => Some(GpuTarget::Cuda),
        GpuBackend::Vulkan => Some(GpuTarget::Vulkan),
        GpuBackend::Opencl => Some(GpuTarget::Opencl),
        _ => None,
    }
}

pub fn platform_prefix() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows-x64"
    } else if cfg!(target_os = "linux") {
        "linux-x64"
    } else {
        "macos-aarch64"
    }
}

pub fn executable_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "llama-server.exe"
    } else {
        "llama-server"
    }
}

/// Resolves the writable runtime dir:
/// - In dev (cfg!(debug_assertions)): resources/llama/<platform>
/// - In prod: app_local_data_dir()/runtime/llama/<tag>/
pub fn runtime_dir(app: &AppHandle) -> AppResult<PathBuf> {
    if cfg!(debug_assertions) {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let path = repo.join("resources").join("llama").join(platform_prefix());
        if path.is_dir() {
            return Ok(path);
        }
    }

    let local_data = app
        .path()
        .app_local_data_dir()
        .or_else(|_| app.path().app_data_dir())
        .map_err(|e| AppError::msg(format!("failed to resolve local data directory: {e}")))?;

    let dir = local_data
        .join("LocalAIse")
        .join("runtime")
        .join("llama")
        .join(PINNED_TAG);

    Ok(dir)
}

/// The active flat directory where binaries and DLLs live.
pub fn platform_dir(dirs: &AppDirs) -> PathBuf {
    dirs.runtime.clone()
}

pub fn build_dir(dirs: &AppDirs, _target: GpuTarget) -> PathBuf {
    platform_dir(dirs)
}

/// Ensures the writable runtime directory exists and has the base CPU files copied from resources.
/// If runtime.json is missing or tag differs:
/// 1. Copy bundled CPU files from resources into `<runtime_dir>.tmp`
/// 2. Carry over any previously installed GPU backends if applicable
/// 3. Write runtime.json
/// 4. Rename .tmp to runtime_dir
/// 5. Clean up old tag directories under runtime/llama/
pub fn ensure_runtime(app: &AppHandle) -> AppResult<PathBuf> {
    let final_dir = runtime_dir(app)?;
    let runtime_json_path = final_dir.join("runtime.json");

    let needs_update = if !final_dir.is_dir() || !runtime_json_path.is_file() {
        true
    } else {
        match std::fs::read_to_string(&runtime_json_path) {
            Ok(content) => match serde_json::from_str::<RuntimeMetadata>(&content) {
                Ok(meta) => meta.tag != PINNED_TAG,
                Err(_) => true,
            },
            Err(_) => true,
        }
    };

    if !needs_update {
        return Ok(final_dir);
    }

    // Locate bundled CPU resources
    let bundled_res_dir = app
        .path()
        .resource_dir()
        .map_err(|e| AppError::msg(format!("failed to get resource dir: {e}")))?
        .join("bundle-llama")
        .join(platform_prefix());

    let mut candidate_source = bundled_res_dir;
    if !candidate_source.is_dir() {
        // Fallback for dev mode / unbundled run
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let dev_bundle = repo.join("src-tauri").join("bundle-llama").join(platform_prefix());
        if dev_bundle.is_dir() {
            candidate_source = dev_bundle;
        } else {
            candidate_source = repo.join("resources").join("llama").join(format!("{}-cpu", platform_prefix()));
            if !candidate_source.is_dir() {
                candidate_source = repo.join("resources").join("llama").join(platform_prefix());
            }
        }
    }

    if let Some(parent) = final_dir.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let tmp_dir = final_dir.with_extension("tmp");
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir)
        .map_err(|e| AppError::msg(format!("cannot create temp runtime dir {}: {e}", tmp_dir.display())))?;

    if candidate_source.is_dir() {
        for entry in std::fs::read_dir(&candidate_source)
            .map_err(|e| AppError::msg(format!("cannot read source dir {}: {e}", candidate_source.display())))?
            .filter_map(|e| e.ok())
        {
            let src = entry.path();
            if src.is_file() {
                let dst = tmp_dir.join(entry.file_name());
                let _ = std::fs::copy(&src, &dst);
            }
        }
    }

    let meta = RuntimeMetadata {
        tag: PINNED_TAG.to_string(),
        installed_backends: Vec::new(),
    };
    if let Ok(meta_bytes) = serde_json::to_vec_pretty(&meta) {
        let _ = std::fs::write(tmp_dir.join("runtime.json"), meta_bytes);
    }

    // If final_dir exists, remove it before renaming
    let _ = std::fs::remove_dir_all(&final_dir);
    std::fs::rename(&tmp_dir, &final_dir)
        .map_err(|e| AppError::msg(format!("failed to move runtime folder into place: {e}")))?;

    // Clean up old tag folders under runtime/llama/
    if let Some(parent) = final_dir.parent() {
        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.filter_map(|e| e.ok()) {
                let p = entry.path();
                if p.is_dir() && p != final_dir && !p.to_string_lossy().ends_with(".tmp") {
                    let _ = std::fs::remove_dir_all(&p);
                }
            }
        }
    }

    Ok(final_dir)
}

pub fn backend_dll_installed(dirs: &AppDirs, target: GpuTarget) -> bool {
    let ext = if cfg!(target_os = "windows") { "dll" } else { "so" };
    let prefix = if cfg!(target_os = "windows") { "ggml-" } else { "libggml-" };
    let dll_name = format!("{}{}.{}", prefix, target.as_str(), ext);
    platform_dir(dirs).join(&dll_name).is_file() || resources_backend_dll(target).is_some()
}

pub fn installed_binary(dirs: &AppDirs, _target: GpuTarget) -> Option<PathBuf> {
    let exe = executable_name();
    let runtime_path = platform_dir(dirs).join(exe);
    if runtime_path.is_file() {
        return Some(runtime_path);
    }

    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let resources = repo
        .join("resources")
        .join("llama")
        .join(platform_prefix())
        .join(exe);
    resources.is_file().then_some(resources)
}

fn resources_backend_dll(target: GpuTarget) -> Option<PathBuf> {
    let ext = if cfg!(target_os = "windows") { "dll" } else { "so" };
    let prefix = if cfg!(target_os = "windows") { "ggml-" } else { "libggml-" };
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    // Try per-backend folder first (documented dev layout), then flat folder.
    for dir in [
        repo.join("resources")
            .join("llama")
            .join(format!("{}-{}", platform_prefix(), target.as_str())),
        repo.join("resources").join("llama").join(platform_prefix()),
    ] {
        let path = dir.join(format!("{}{}.{}", prefix, target.as_str(), ext));
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

pub fn status(dirs: &AppDirs, detected: &GpuBackend) -> Vec<GpuBuildStatus> {
    let recommended = recommend(detected);
    let order = [GpuTarget::Cuda, GpuTarget::Vulkan, GpuTarget::Opencl];

    order
        .iter()
        .map(|target| GpuBuildStatus {
            target: target.as_str().to_string(),
            label: target.label().to_string(),
            installed: backend_dll_installed(dirs, *target),
            source: manifest(dirs, *target).ok(),
            downloadable: target.downloadable(),
            directory: platform_dir(dirs).to_string_lossy().into_owned(),
            recommended: Some(*target) == recommended,
        })
        .collect()
}

fn manifest(dirs: &AppDirs, target: GpuTarget) -> AppResult<BuildSource> {
    let path = platform_dir(dirs).join(format!("build-info-{}.json", target.as_str()));
    let text = std::fs::read_to_string(&path)
        .map_err(|_| AppError::msg(format!("{} has no install manifest", target.label())))?;
    serde_json::from_str(&text)
        .map_err(|error| AppError::msg(format!("the install manifest is unreadable: {error}")))
}

fn emit_progress(app: &AppHandle, target: GpuTarget, stage: &str, bytes: u64, total: u64) {
    use tauri::Emitter;
    let _ = app.emit(
        PROGRESS_EVENT,
        serde_json::json!({
            "target": target.as_str(),
            "stage": stage,
            "bytes": bytes,
            "total": total,
        }),
    );
    // Also emit legacy event for backward compatibility
    let _ = app.emit(
        "localAIse/gpu-build",
        serde_json::json!({
            "target": target.as_str(),
            "stage": stage,
            "done": bytes,
            "total": total,
        }),
    );
}

/// Load the embedded backends.json manifest
pub fn load_manifest() -> AppResult<BackendManifest> {
    let manifest_str = include_str!("../../backends.json");
    serde_json::from_str(manifest_str)
        .map_err(|e| AppError::msg(format!("failed to parse backends.json: {e}")))
}

/// Removes an installed GPU backend:
/// Deletes the matching DLLs and build-info-<backend>.json, updates runtime.json.
pub fn remove(dirs: &AppDirs, target: GpuTarget) -> AppResult<()> {
    let p_dir = platform_dir(dirs);
    let ext = if cfg!(target_os = "windows") { "dll" } else { "so" };
    let prefix = if cfg!(target_os = "windows") { "ggml-" } else { "libggml-" };

    let primary_dll = p_dir.join(format!("{}{}.{}", prefix, target.as_str(), ext));
    let _ = std::fs::remove_file(primary_dll);

    let info_path = p_dir.join(format!("build-info-{}.json", target.as_str()));
    if let Ok(content) = std::fs::read_to_string(&info_path) {
        if let Ok(source) = serde_json::from_str::<BuildSource>(&content) {
            for f in source.installed_files {
                let _ = std::fs::remove_file(p_dir.join(f));
            }
        }
    }
    let _ = std::fs::remove_file(info_path);

    let runtime_json_path = p_dir.join("runtime.json");
    if let Ok(content) = std::fs::read_to_string(&runtime_json_path) {
        if let Ok(mut meta) = serde_json::from_str::<RuntimeMetadata>(&content) {
            meta.installed_backends.retain(|b| b != target.as_str());
            if let Ok(b) = serde_json::to_vec_pretty(&meta) {
                let _ = std::fs::write(&runtime_json_path, b);
            }
        }
    }

    Ok(())
}

/// Installs a GPU backend:
/// 1. Download files to tmp/ with range resume & 3 retries
/// 2. Emit progress
/// 3. Verify SHA-256
/// 4. Extract to temp and keep only files matching glob patterns
/// 5. Run llama-server --list-devices to probe
/// 6. Update runtime.json and build-info-<backend>.json
pub async fn install(
    app: &AppHandle,
    dirs: &AppDirs,
    target: GpuTarget,
) -> AppResult<PathBuf> {
    reset_cancel_flag();
    emit_progress(app, target, "resolving", 0, 0);

    let manifest = load_manifest()?;
    let current_p = platform_prefix();
    let entry = manifest
        .entries
        .into_iter()
        .find(|e| e.platform == current_p && e.backend == target.as_str())
        .ok_or_else(|| {
            AppError::msg(format!(
                "No backend configuration found in backends.json for {} on {}",
                target.label(),
                current_p
            ))
        })?;

    let tmp_dir = dirs.base.join("tmp");
    std::fs::create_dir_all(&tmp_dir)
        .map_err(|e| AppError::msg(format!("failed to create tmp dir {}: {e}", tmp_dir.display())))?;

    let client = reqwest::Client::builder()
        .user_agent("localAIse-BackendInstaller/0.1.0")
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .map_err(|e| AppError::msg(format!("failed to initialize HTTP client: {e}")))?;

    let mut downloaded_files = Vec::new();

    for file_spec in &entry.files {
        if is_install_cancelled() {
            return Err(AppError::msg("installation cancelled by user"));
        }

        let file_name = file_spec
            .url
            .split('/')
            .last()
            .unwrap_or("archive.zip")
            .to_string();
        let dest = tmp_dir.join(&file_name);

        download_with_retry(
            app,
            &client,
            &file_spec.url,
            &dest,
            &file_spec.sha256,
            target,
        )
        .await?;

        downloaded_files.push((file_name, dest));
    }

    if is_install_cancelled() {
        return Err(AppError::msg("installation cancelled by user"));
    }

    emit_progress(app, target, "extracting", 0, 0);

    let staging_extract = tmp_dir.join(format!("staging-{}", target.as_str()));
    let _ = std::fs::remove_dir_all(&staging_extract);
    std::fs::create_dir_all(&staging_extract)
        .map_err(|e| AppError::msg(format!("failed to create extract dir: {e}")))?;

    for (_, archive_path) in &downloaded_files {
        let name_str = archive_path.to_string_lossy().to_string();
        if name_str.ends_with(".zip") {
            extract_zip(archive_path, &staging_extract)?;
        } else {
            extract_tar_gz(archive_path, &staging_extract)?;
        }
    }

    let p_dir = platform_dir(dirs);
    std::fs::create_dir_all(&p_dir)
        .map_err(|e| AppError::msg(format!("cannot create platform dir {}: {e}", p_dir.display())))?;

    let mut staged_files_to_copy = Vec::new();
    collect_matching_files(&staging_extract, &entry.keep, &mut staged_files_to_copy);

    if staged_files_to_copy.is_empty() {
        let _ = std::fs::remove_dir_all(&staging_extract);
        return Err(AppError::msg(format!(
            "No files matching keep criteria {:?} found in archive",
            entry.keep
        )));
    }

    emit_progress(app, target, "verifying", 0, 0);

    // Copy staged files to a backup/test location first or directly with rollback capability
    let mut copied_files = Vec::new();
    let mut copy_error: Option<String> = None;

    for (src_path, filename) in &staged_files_to_copy {
        let dst_path = p_dir.join(filename);
        if let Err(e) = std::fs::copy(src_path, &dst_path) {
            copy_error = Some(format!("failed copying {filename}: {e}"));
            break;
        }
        copied_files.push(filename.clone());
    }

    let _ = std::fs::remove_dir_all(&staging_extract);

    if let Some(err) = copy_error {
        // Rollback
        for f in &copied_files {
            let _ = std::fs::remove_file(p_dir.join(f));
        }
        return Err(AppError::msg(err));
    }

    // Probe with llama-server --list-devices
    let binary = p_dir.join(executable_name());
    let probe_result = probe_backend_device(&binary, target);

    if let Err(probe_err) = probe_result {
        // Rollback on probe failure
        for f in &copied_files {
            let _ = std::fs::remove_file(p_dir.join(f));
        }
        return Err(AppError::msg(format!(
            "verification failed for {}: {probe_err}. Backend rolled back.",
            target.label()
        )));
    }

    // Success: Update runtime.json and build-info-<backend>.json
    let build_source = BuildSource {
        tag: manifest.tag.clone(),
        platform: entry.platform.clone(),
        backend: target.as_str().to_string(),
        installed_files: copied_files,
        installed_at: crate::util::time::now_iso(),
    };
    let _ = std::fs::write(
        p_dir.join(format!("build-info-{}.json", target.as_str())),
        serde_json::to_vec_pretty(&build_source).unwrap_or_default(),
    );

    let runtime_json_path = p_dir.join("runtime.json");
    let mut meta = match std::fs::read_to_string(&runtime_json_path) {
        Ok(c) => serde_json::from_str::<RuntimeMetadata>(&c).unwrap_or(RuntimeMetadata {
            tag: manifest.tag,
            installed_backends: Vec::new(),
        }),
        Err(_) => RuntimeMetadata {
            tag: manifest.tag,
            installed_backends: Vec::new(),
        },
    };
    if !meta.installed_backends.contains(&target.as_str().to_string()) {
        meta.installed_backends.push(target.as_str().to_string());
    }
    let _ = std::fs::write(&runtime_json_path, serde_json::to_vec_pretty(&meta).unwrap_or_default());

    emit_progress(app, target, "done", 100, 100);
    Ok(binary)
}

fn probe_backend_device(binary: &Path, target: GpuTarget) -> Result<(), String> {
    if !binary.is_file() {
        return Err("launcher binary missing".to_string());
    }

    let mut cmd = std::process::Command::new(binary);
    cmd.arg("--list-devices");

    #[cfg(target_os = "linux")]
    {
        if let Some(parent) = binary.parent() {
            cmd.env("LD_LIBRARY_PATH", parent);
        }
    }

    let output = cmd
        .output()
        .map_err(|e| format!("cannot run --list-devices: {e}"))?;

    let text = String::from_utf8_lossy(&output.stdout);
    let target_prefix = match target {
        GpuTarget::Cuda => "cuda",
        GpuTarget::Vulkan => "vulkan",
        GpuTarget::Opencl => "opencl",
    };

    let has_device = text
        .lines()
        .filter_map(crate::runtime::backend::parse_device_line)
        .any(|(backend, _)| backend.to_lowercase().contains(target_prefix));

    if !has_device {
        return Err(format!(
            "llama-server did not detect any device matching {}",
            target.label()
        ));
    }

    Ok(())
}

fn collect_matching_files(dir: &Path, patterns: &[String], out: &mut Vec<(PathBuf, String)>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                collect_matching_files(&path, patterns, out);
            } else if path.is_file() {
                let name = entry.file_name().to_string_lossy().to_string();
                if matches_any_glob(&name, patterns) {
                    out.push((path, name));
                }
            }
        }
    }
}

fn matches_any_glob(filename: &str, patterns: &[String]) -> bool {
    let lower_name = filename.to_lowercase();
    for pat in patterns {
        let lower_pat = pat.to_lowercase();
        if lower_pat.ends_with('*') {
            let prefix = &lower_pat[..lower_pat.len() - 1];
            if lower_name.starts_with(prefix) {
                return true;
            }
        } else if lower_pat.contains('*') {
            let parts: Vec<&str> = lower_pat.split('*').collect();
            if parts.len() == 2 && lower_name.starts_with(parts[0]) && lower_name.ends_with(parts[1]) {
                return true;
            }
        } else if lower_name == lower_pat {
            return true;
        }
    }
    false
}

async fn download_with_retry(
    app: &AppHandle,
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    expected_sha256: &str,
    target: GpuTarget,
) -> AppResult<()> {
    let mut attempts = 0;
    let max_attempts = 3;

    loop {
        attempts += 1;
        match download_resumable(app, client, url, dest, target).await {
            Ok(_) => {
                // Verify SHA-256
                let computed = compute_sha256(dest)?;
                if computed.eq_ignore_ascii_case(expected_sha256) {
                    return Ok(());
                } else {
                    let _ = std::fs::remove_file(dest);
                    if attempts >= max_attempts {
                        return Err(AppError::msg(format!(
                            "SHA-256 verification failed for {url}. Expected {expected_sha256}, got {computed}."
                        )));
                    }
                }
            }
            Err(e) => {
                if is_install_cancelled() {
                    return Err(AppError::msg("installation cancelled by user"));
                }
                if attempts >= max_attempts {
                    return Err(AppError::msg(format!(
                        "download failed after {max_attempts} attempts: {e}"
                    )));
                }
                tokio::time::sleep(std::time::Duration::from_millis(500 * attempts as u64)).await;
            }
        }
    }
}

async fn download_resumable(
    app: &AppHandle,
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    target: GpuTarget,
) -> AppResult<()> {
    use std::io::{Seek, SeekFrom, Write};

    let mut start_pos: u64 = 0;
    if dest.is_file() {
        start_pos = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
    }

    let mut req = client.get(url);
    if start_pos > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={start_pos}-"));
    }

    let response = req
        .send()
        .await
        .map_err(|e| AppError::msg(format!("failed to send download request to {url}: {e}")))?;

    let status = response.status();
    let is_range_resumed = status == reqwest::StatusCode::PARTIAL_CONTENT;

    if !status.is_success() && !is_range_resumed {
        return Err(AppError::msg(format!("server returned status {status} for {url}")));
    }

    let content_len = response.content_length().unwrap_or(0);
    let total_len = if is_range_resumed {
        start_pos + content_len
    } else {
        content_len
    };

    let mut file = if is_range_resumed {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(dest)
            .map_err(|e| AppError::msg(format!("cannot open file for resume: {e}")))?;
        f.seek(SeekFrom::End(0))
            .map_err(|e| AppError::msg(format!("cannot seek to end of file: {e}")))?;
        f
    } else {
        start_pos = 0;
        std::fs::File::create(dest)
            .map_err(|e| AppError::msg(format!("cannot create file {}: {e}", dest.display())))?
    };

    let mut stream = response;
    let mut downloaded = start_pos;
    let mut last_reported_mib = downloaded / (1024 * 1024);

    while let Some(chunk) = stream
        .chunk()
        .await
        .map_err(|e| AppError::msg(format!("network error during chunk download: {e}")))?
    {
        if is_install_cancelled() {
            return Err(AppError::msg("installation cancelled by user"));
        }
        file.write_all(&chunk)
            .map_err(|e| AppError::msg(format!("failed writing chunk to disk: {e}")))?;
        downloaded += chunk.len() as u64;

        let cur_mib = downloaded / (1024 * 1024);
        if cur_mib > last_reported_mib || downloaded == total_len {
            last_reported_mib = cur_mib;
            emit_progress(app, target, "downloading", downloaded, total_len);
        }
    }

    Ok(())
}

fn compute_sha256(path: &Path) -> AppResult<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)
        .map_err(|e| AppError::msg(format!("cannot open file to compute sha256: {e}")))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|e| AppError::msg(format!("cannot read file buffer for sha256: {e}")))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn extract_zip(archive: &Path, dest: &Path) -> AppResult<()> {
    let file = std::fs::File::open(archive)
        .map_err(|error| AppError::msg(format!("cannot open {}: {error}", archive.display())))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|error| AppError::msg(format!("cannot read {}: {error}", archive.display())))?;
    zip.extract(dest)
        .map_err(|error| AppError::msg(format!("cannot unpack {}: {error}", archive.display())))
}

fn extract_tar_gz(archive: &Path, dest: &Path) -> AppResult<()> {
    let file = std::fs::File::open(archive)
        .map_err(|error| AppError::msg(format!("cannot open {}: {error}", archive.display())))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(decoder);
    tar.unpack(dest)
        .map_err(|error| AppError::msg(format!("cannot unpack {}: {error}", archive.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_names_follow_the_platform() {
        if cfg!(target_os = "windows") {
            assert_eq!(executable_name(), "llama-server.exe");
        } else {
            assert_eq!(executable_name(), "llama-server");
        }
    }

    #[test]
    fn targets_round_trip_through_their_names() {
        for target in [GpuTarget::Vulkan, GpuTarget::Cuda, GpuTarget::Opencl] {
            assert_eq!(GpuTarget::parse(target.as_str()), Some(target));
        }
        assert_eq!(GpuTarget::parse("hip"), None);
    }

    #[test]
    fn glob_matching_works() {
        assert!(matches_any_glob("ggml-cuda.dll", &["ggml-cuda.dll".into()]));
        assert!(matches_any_glob("cudart64_12.dll", &["cudart64_*.dll".into()]));
        assert!(matches_any_glob("cublas64_12.dll", &["cublas*.dll".into()]));
        assert!(!matches_any_glob("vulkan-1.dll", &["ggml-cuda.dll".into()]));
    }
}
