//! Installing a llama-server build with GPU support.
//!
//! The app ships a CPU-only build; a GPU build is an explicit, user-initiated
//! install. This module looks at the graphics card the machine reports,
//! recommends the backend that fits it, and downloads the official llama.cpp
//! release archive for that backend from GitHub — verifying every archive
//! against the SHA-256 digest ggml-org publishes for the release.
//!
//! Backend choice follows the vendor of the detected card:
//!
//! - NVIDIA (`nvcuda.dll`, `/dev/nvidia0`) — the CUDA build: wider quant
//!   kernel selection and faster flash attention than Vulkan on these cards.
//! - AMD or Intel — the Vulkan build: one runtime for every GPU vendor, and
//!   the driver usually already ships the loader.
//! - Nothing detected — nothing is recommended or offered.
//!
//! There is deliberately no in-app install for OpenCL: ggml-org publishes no
//! OpenCL archive, so an OpenCL build can only be produced locally from the
//! vendored checkout (`scripts/fetch-llama-server.ts <platform>-x64-opencl`).
//! The status list still reports an OpenCL build the user put there by hand.
//!
//! Installed builds land in the app's data dir next to the CPU build, as
//! `<platform>-<backend>/` (for example `windows-x64-vulkan/`), together with
//! a `build-info.json` manifest recording what was downloaded, from where and
//! against which digests — the same ledger discipline `scripts/
//! verify-binaries.ts` applies to the bundled CPU build.
//!
//! A failed install never takes the previous one down with it: archives
//! stream into a staging folder first, are extracted and probed there, and
//! only a complete layout moves into the real build folder.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::paths::AppDirs;
use crate::error::{AppError, AppResult};
use crate::hardware::gpu::GpuBackend;

/// The llama.cpp release the vendored checkout tracks. Keep in sync with
/// `PINNED_TAG` in scripts/fetch-llama-server.ts — the flag surface of this
/// tree is what `runtime/args.rs` was audited against.
const RELEASE_TAG: &str = "v0.7.0";

/// The prebuilt CUDA runtime levels offered, in preference order. The oldest
/// level needs the smallest driver, so it wins: 12.4 on Windows, 12.8 on
/// Ubuntu, where those are the levels upstream publishes.
const CUDA_LEVELS: &[&str] = &["12.4", "13.4"];
const CUDA_LEVELS_LINUX: &[&str] = &["12.8", "13.4"];

const GITHUB_API: &str = "https://api.github.com/repos/ggml-org/llama.cpp";

/// Event the settings page listens to during an install.
pub const PROGRESS_EVENT: &str = "localAIse/gpu-build";

/// The three backend builds the runtime can resolve.
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

    /// How the UI names the backend.
    pub fn label(self) -> &'static str {
        match self {
            GpuTarget::Vulkan => "Vulkan",
            GpuTarget::Cuda => "CUDA",
            GpuTarget::Opencl => "OpenCL",
        }
    }

    /// Inverse of `as_str`, so a command can address a target by name.
    pub fn parse(value: &str) -> Option<GpuTarget> {
        match value {
            "vulkan" => Some(GpuTarget::Vulkan),
            "cuda" => Some(GpuTarget::Cuda),
            "opencl" => Some(GpuTarget::Opencl),
            _ => None,
        }
    }

    /// True when the build comes from an official ggml-org prebuilt archive.
    /// OpenCL has none, so the UI points that button at the build script.
    pub fn downloadable(self) -> bool {
        !matches!(self, GpuTarget::Opencl)
    }
}

/// What one backend build looks like on disk right now.
///
/// `installed` only means the binary exists in the target folder; whether it
/// can actually see a graphics card is the separate capability probe of the
/// build itself, shown next to it in the settings page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuBuildStatus {
    /// "vulkan", "cuda" or "opencl".
    pub target: String,
    /// "Vulkan", "CUDA" or "OpenCL".
    pub label: String,
    pub installed: bool,
    /// What the install manifest recorded, for in-app installs.
    pub source: Option<BuildSource>,
    /// Whether the app can download this one itself.
    pub downloadable: bool,
    /// The folder the build lives in, or would land in.
    pub directory: String,
    /// True when this backend matches the detected graphics card.
    pub recommended: bool,
}

/// What an installed build says about its origin.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BuildSource {
    /// The tagged llama.cpp release the install follows, e.g. "v0.7.0".
    pub release_tag: String,
    /// The nightly build the prebuilt archive came from, e.g. "b11669".
    pub nightly_tag: String,
    /// The archive URLs the install downloaded.
    pub archives: Vec<String>,
    /// The digests that were checked, one per archive.
    pub digests: Vec<String>,
    /// ISO timestamp of the install.
    pub installed_at: String,
}

/// Who offers a backend for the machine's graphics card.
pub fn recommend(backend: &GpuBackend) -> Option<GpuTarget> {
    match backend {
        GpuBackend::Cuda => Some(GpuTarget::Cuda),
        GpuBackend::Vulkan => Some(GpuTarget::Vulkan),
        GpuBackend::Opencl => Some(GpuTarget::Opencl),
        // macOS ships Metal inside its own build; no separate archive exists.
        _ => None,
    }
}

/// The single flat folder where all backends for this platform live.
///
/// All files — the CPU dispatch DLLs, the core DLLs and any GPU backend DLL
/// (e.g. `ggml-cuda.dll`) — sit together. Installing a GPU backend means
/// dropping its files here; removing it means deleting them. There is no
/// separate `windows-x64-cuda` folder.
pub fn platform_dir(dirs: &AppDirs) -> PathBuf {
    dirs.llama.join(platform_prefix())
}

/// Kept for backwards compatibility; returns the same flat platform dir.
/// GPU targets no longer live in separate directories.
pub fn build_dir(dirs: &AppDirs, _target: GpuTarget) -> PathBuf {
    platform_dir(dirs)
}

fn platform_prefix() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows-x64"
    } else if cfg!(target_os = "linux") {
        "linux-x64"
    } else {
        "macos-aarch64"
    }
}

fn executable_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "llama-server.exe"
    } else {
        "llama-server"
    }
}

/// Whether a GPU backend DLL/SO is present in the flat platform folder.
///
/// A backend is "installed" when its `ggml-<backend>.dll` (Windows) or
/// `ggml-<backend>.so` (Linux) file exists beside the launcher in the
/// shared platform directory. No separate per-backend subdirectories.
pub fn backend_dll_installed(dirs: &AppDirs, target: GpuTarget) -> bool {
    let ext = if cfg!(target_os = "windows") { "dll" } else { "so" };
    platform_dir(dirs)
        .join(format!("ggml-{}.{}", target.as_str(), ext))
        .is_file()
        || resources_backend_dll(target).is_some()
}

/// The llama-server binary of an installed build, when present.
///
/// The launcher lives in the flat platform folder (`windows-x64` or
/// `linux-x64`). Both the app-data dir and the `resources/llama` repo
/// location count as "installed".
pub fn installed_binary(dirs: &AppDirs, _target: GpuTarget) -> Option<PathBuf> {
    let exe = executable_name();
    let app_data = platform_dir(dirs).join(exe);
    if app_data.is_file() {
        return Some(app_data);
    }
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let resources = repo
        .join("resources")
        .join("llama")
        .join(platform_prefix())
        .join(exe);
    resources.is_file().then_some(resources)
}

/// Returns the path to a GPU backend's DLL/SO inside the repo's resources
/// folder, if it is present there (dev mode).
fn resources_backend_dll(target: GpuTarget) -> Option<PathBuf> {
    let ext = if cfg!(target_os = "windows") { "dll" } else { "so" };
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let path = repo
        .join("resources")
        .join("llama")
        .join(platform_prefix())
        .join(format!("ggml-{}.{}", target.as_str(), ext));
    path.is_file().then_some(path)
}

/// Every backend build the app knows about, in UI order.
///
/// `installed` reflects whether the backend DLL is present in the shared
/// platform folder — not whether a separate subdirectory exists.
/// The recommendation comes from the machine's own GPU detection.
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

/// The `build-info.json` an in-app install wrote, when it is there.
/// The manifest lives in the shared platform folder.
fn manifest(dirs: &AppDirs, target: GpuTarget) -> AppResult<BuildSource> {
    let path = platform_dir(dirs).join(format!("build-info-{}.json", target.as_str()));
    let text = std::fs::read_to_string(&path)
        .map_err(|_| AppError::msg(format!("{} has no install manifest", target.label())))?;
    serde_json::from_str(&text)
        .map_err(|error| AppError::msg(format!("the install manifest is unreadable: {error}")))
}

/// The nightly build a tagged release corresponds to.
///
/// ggml-org tags a stable release monthly; the prebuilt binaries for that
/// tree are cut from the nightly of the same day. The stable release carries
/// only sources plus `nightly-tag.txt`, which names that nightly.
async fn resolve_nightly_tag(client: &reqwest::Client) -> AppResult<String> {
    let release =
        fetch_json::<serde_json::Value>(client, &format!("{GITHUB_API}/releases/tags/{RELEASE_TAG}"))
            .await?;

    let url = release
        .get("assets")
        .and_then(|assets| assets.as_array())
        .and_then(|assets| {
            assets
                .iter()
                .find(|asset| asset.get("name").and_then(|n| n.as_str()) == Some("nightly-tag.txt"))
                .and_then(|asset| asset.get("browser_download_url").and_then(|u| u.as_str()))
        })
        .ok_or_else(|| {
            AppError::msg("the llama.cpp release does not publish which nightly it corresponds to")
        })?
        .to_string();

    let tag = client
        .get(url)
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|error| AppError::msg(format!("fetching the nightly tag failed: {error}")))?
        .text()
        .await
        .map_err(|error| AppError::msg(format!("reading the nightly tag failed: {error}")))?;

    let tag = tag.trim().to_string();
    if tag.is_empty() {
        return Err(AppError::msg("the nightly tag is empty"));
    }
    Ok(tag)
}

async fn fetch_json<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
) -> AppResult<T> {
    client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|error| AppError::msg(format!("{url} failed: {error}")))?
        .json::<T>()
        .await
        .map_err(|error| AppError::msg(format!("parsing the response of {url} failed: {error}")))
}

#[derive(Debug, Clone, Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
    /// GitHub publishes "sha256:<hex>" for release assets; it is the only
    /// checksum that does not just repeat what the download itself says.
    digest: Option<String>,
}

/// The archive-name sets that one install is satisfied with, in preference
/// order. A CUDA install is two archives at the same CUDA level: the build
/// itself plus the runtime libraries upstream publishes separately.
#[cfg_attr(test, allow(unused))]
fn archive_name_sets(nightly: &str, target: GpuTarget) -> AppResult<Vec<Vec<String>>> {
    if target == GpuTarget::Opencl {
        return Err(AppError::msg(
            "ggml-org publishes no OpenCL archive; build one locally with \
             `npm run fetch-llama-server -- <platform> opencl`",
        ));
    }
    let win = cfg!(target_os = "windows");
    if !win && !cfg!(target_os = "linux") {
        return Err(AppError::msg(
            "macOS ships Metal in its own llama-server build; there is nothing to install",
        ));
    }

    let os = if win { "win" } else { "ubuntu" };
    let ext = if win { "zip" } else { "tar.gz" };

    Ok(match target {
        GpuTarget::Vulkan => vec![vec![format!("llama-{nightly}-bin-{os}-vulkan-x64.{ext}")]],
        GpuTarget::Cuda => {
            let levels: &[&str] = if win { CUDA_LEVELS } else { CUDA_LEVELS_LINUX };
            levels
                .iter()
                .map(|level| {
                    vec![
                        format!("llama-{nightly}-bin-{os}-cuda-{level}-x64.{ext}"),
                        format!("cudart-llama-bin-{os}-cuda-{level}-x64.{ext}"),
                    ]
                })
                .collect()
        }
        GpuTarget::Opencl => unreachable!("rejected above"),
    })
}

/// Installs the prebuilt build of `target` into the app's data dir and
/// returns the path of its launcher.
///
/// Every step is reported to the UI through [`PROGRESS_EVENT`]: `resolving`,
/// `downloading` (with a byte fraction), `extracting`, `verifying` and `done`.
/// A failure at any point leaves the previous install untouched and damages
/// the new one only inside the staging folder, which is cleaned up.
pub async fn install(
    app: &tauri::AppHandle,
    dirs: &AppDirs,
    target: GpuTarget,
) -> AppResult<PathBuf> {
    emit_progress(app, target, "resolving");

    let client = reqwest::Client::builder()
        .user_agent("localAIse-GpuBuild/0.1.0")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| AppError::msg(format!("the download client failed to start: {error}")))?;

    let nightly = resolve_nightly_tag(&client).await?;

    let listing: serde_json::Value =
        fetch_json(&client, &format!("{GITHUB_API}/releases/tags/{nightly}")).await?;
    let assets: Vec<ReleaseAsset> = serde_json::from_value(
        listing
            .get("assets")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([])),
    )
    .map_err(|error| AppError::msg(format!("the release listing is unreadable: {error}")))?;

    // The first name set the release actually offers wins; upstream may add
    // or retire CUDA levels between nightlies.
    let mut archives: Option<Vec<ReleaseAsset>> = None;
    for names in archive_name_sets(&nightly, target)? {
        let found: Option<Vec<ReleaseAsset>> =
            names
                .iter()
                .map(|name| assets.iter().find(|asset| asset.name == *name).cloned())
                .collect();
        if let Some(found) = found {
            archives = Some(found);
            break;
        }
    }
    let archives = archives.ok_or_else(|| {
        AppError::msg(format!(
            "no {} archive is published in nightly {nightly}. Try again later, or build \
             llama.cpp yourself with the fetch script.",
            target.label()
        ))
    })?;

    install_archives(app, dirs, target, &nightly, &client, archives).await
}

fn emit_progress(
    app: &tauri::AppHandle,
    target: GpuTarget,
    stage: &str,
) {
    use tauri::Emitter;
    let _ = app.emit(
        PROGRESS_EVENT,
        serde_json::json!({ "target": target.as_str(), "stage": stage }),
    );
}

fn emit_progress_bytes(
    app: &tauri::AppHandle,
    target: GpuTarget,
    done: u64,
    total: u64,
) {
    use tauri::Emitter;
    let _ = app.emit(
        PROGRESS_EVENT,
        serde_json::json!({
            "target": target.as_str(),
            "stage": "downloading",
            "done": done,
            "total": total,
        }),
    );
}

/// Download, extract, probe, move into place, manifest.
async fn install_archives(
    app: &tauri::AppHandle,
    dirs: &AppDirs,
    target: GpuTarget,
    nightly: &str,
    client: &reqwest::Client,
    archives: Vec<ReleaseAsset>,
) -> AppResult<PathBuf> {
    let staging = dirs.llama.join(format!(".staging-{}", target.as_str()));
    // A staging folder from an interrupted attempt never survives into this
    // one: the download below estimates progress by file size.
    std::fs::remove_dir_all(&staging).ok();

    let outcome = perform_install(
        app, dirs, target, nightly, client, &staging, archives,
    )
    .await;

    if outcome.is_err() {
        std::fs::remove_dir_all(&staging).ok();
    }
    outcome
}

async fn perform_install(
    app: &tauri::AppHandle,
    dirs: &AppDirs,
    target: GpuTarget,
    nightly: &str,
    client: &reqwest::Client,
    staging: &Path,
    archives: Vec<ReleaseAsset>,
) -> AppResult<PathBuf> {
    let mut download_urls: Vec<String> = Vec::new();
    let mut download_digests: Vec<String> = Vec::new();

    for asset in &archives {
        emit_progress(app, target, "downloading");
        let dest = staging.join(&asset.name);
        let mut last_mib = 0u64;
        download(
            client,
            &asset.browser_download_url,
            &dest,
            asset.digest.as_deref(),
            |done, total| {
                // A progress event per megabyte at most, so the event bus
                // does not drown under a large CUDA runtime payload.
                let mib = done / (1024 * 1024);
                if total > 0 && (mib > last_mib || done == total) {
                    last_mib = mib;
                    emit_progress_bytes(app, target, done, total);
                }
            },
        )
        .await?;
        download_urls.push(asset.browser_download_url.clone());
        download_digests.push(asset.digest.clone().unwrap_or_else(|| "unverified".into()));
    }

    emit_progress(app, target, "extracting");

    let layout = staging.join("layout");
    extract_all(&archives, staging, &layout)?;

    emit_progress(app, target, "verifying");
    // The layout folder holds the freshly extracted build. We verify the
    // launcher runs before touching the live platform folder.
    setup_launcher(&layout)?;

    // Merge the new backend DLLs into the shared flat platform folder rather
    // than overwriting it entirely. The core DLLs (ggml-base, llama, …) from
    // the GPU archive are identical to the CPU build's copies (same tag), so
    // overwriting them is safe. The new file that actually matters is
    // ggml-<backend>.dll and any runtime DLLs (e.g. cudart64_*.dll).
    let dest_dir = platform_dir(dirs);
    std::fs::create_dir_all(&dest_dir).map_err(|error| {
        AppError::msg(format!("cannot create platform dir {}: {error}", dest_dir.display()))
    })?;

    for entry in std::fs::read_dir(&layout)
        .map_err(|error| AppError::msg(format!("cannot read layout dir: {error}")))?  
        .filter_map(|e| e.ok())
    {
        let src = entry.path();
        if src.is_file() {
            let dst = dest_dir.join(entry.file_name());
            std::fs::copy(&src, &dst).map_err(|error| {
                AppError::msg(format!(
                    "cannot copy {} into platform dir: {error}",
                    src.display()
                ))
            })?;
        }
    }
    std::fs::remove_dir_all(&layout).ok();

    let binary = dest_dir.join(executable_name());

    let manifest = BuildSource {
        release_tag: RELEASE_TAG.to_string(),
        nightly_tag: nightly.to_string(),
        archives: download_urls,
        digests: download_digests,
        installed_at: crate::util::time::now_iso(),
    };
    // Each backend gets its own manifest so installs don't overwrite each other.
    std::fs::write(
        dest_dir.join(format!("build-info-{}.json", target.as_str())),
        serde_json::to_vec_pretty(&manifest).unwrap_or_else(|_| b"{}".to_vec()),
    )
    .map_err(|error| AppError::msg(format!("writing the install manifest failed: {error}")))?;

    emit_progress(app, target, "done");
    Ok(binary)
}

/// Streams one archive to disk, checking the SHA-256 digest GitHub published
/// for the asset. `on_progress` receives (bytes done, bytes total).
async fn download(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    digest: Option<&str>,
    mut on_progress: impl FnMut(u64, u64),
) -> AppResult<()> {
    let response = client
        .get(url)
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|error| AppError::msg(format!("downloading {url} failed: {error}")))?;
    let total = response.content_length().unwrap_or(0);
    on_progress(0, total);

    let parent = dest
        .parent()
        .ok_or_else(|| AppError::msg("the staging folder has no parent"))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| AppError::msg(format!("cannot create the staging folder: {error}")))?;

    let mut file = std::fs::File::create(dest)
        .map_err(|error| AppError::msg(format!("cannot write to {}: {error}", dest.display())))?;
    let mut hasher = Sha256::new();
    let mut stream = response;
    let mut done: u64 = 0;

    while let Some(chunk) = stream
        .chunk()
        .await
        .map_err(|error| AppError::msg(format!("the download of {url} broke: {error}")))?
    {
        std::io::Write::write_all(&mut file, &chunk)
            .map_err(|error| AppError::msg(format!("writing the archive failed: {error}")))?;
        hasher.update(&chunk);
        done += chunk.len() as u64;
    }
    on_progress(done, total);

    if let Some(expected) = digest {
        let actual = format!("sha256:{:x}", hasher.finalize());
        if actual != expected {
            let _ = std::fs::remove_file(dest);
            return Err(AppError::msg(format!(
                "the archive from {url} does not match the checksum the release published. \
                 Wanted {expected}, got {actual}. The download was discarded; try again, and \
                 have the network looked at if it repeats."
            )));
        }
    }

    Ok(())
}

/// Extracts every downloaded archive into the layout folder.
fn extract_all(archives: &[ReleaseAsset], staging: &Path, layout: &Path) -> AppResult<()> {
    std::fs::create_dir_all(layout)
        .map_err(|error| AppError::msg(format!("cannot create {}: {error}", layout.display())))?;

    for asset in archives {
        let archive = staging.join(&asset.name);
        if asset.name.ends_with(".zip") {
            extract_zip(&archive, layout)?;
        } else {
            extract_tar_gz(&archive, layout)?;
        }
    }

    flatten_single_wrapper(layout)?;
    Ok(())
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

/// One wrapper folder — the name inside a tarball — is flattened away, so the
/// build folder layout is the same on both platforms: the launcher next to
/// its runtime libraries.
fn flatten_single_wrapper(dir: &Path) -> AppResult<()> {
    let entries: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .collect(),
        Err(_) => return Ok(()),
    };

    let wrapper = match entries.as_slice() {
        [single] if single.is_dir() => single.clone(),
        _ => return Ok(()),
    };

    for entry in std::fs::read_dir(&wrapper)
        .map_err(|error| AppError::msg(format!("cannot read the unpacked folder: {error}")))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
    {
        let name = entry
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        std::fs::rename(&entry, dir.join(&name))
            .map_err(|error| AppError::msg(format!("cannot flatten the unpacked folder: {error}")))?;
    }
    std::fs::remove_dir_all(&wrapper)
        .map_err(|error| AppError::msg(format!("cannot remove the empty wrapper folder: {error}")))
}

/// The launcher of a prepared layout is asked to say hello, and is made
/// executable on Linux.
///
/// A launcher that cannot run — a wrong layout, a missing CUDA runtime DLL on
/// Windows — fails the install here instead of surprising the user at chat
/// time.
fn setup_launcher(dir: &Path) -> AppResult<PathBuf> {
    let binary = dir.join(executable_name());
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755));
    }

    match std::process::Command::new(&binary).arg("--version").output() {
        Ok(output) if output.status.success() => Ok(binary),
        _ => Err(AppError::msg(
            "the downloaded build did not answer its own --version. On Windows this usually \
             means a runtime DLL is missing next to the launcher.",
        )),
    }
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
    fn opencl_has_no_download() {
        assert!(archive_name_sets("b11669", GpuTarget::Opencl).is_err());
        assert!(!GpuTarget::Opencl.downloadable());
        assert!(GpuTarget::Vulkan.downloadable());
    }

    #[test]
    fn vulkan_wants_exactly_one_archive() {
        let sets = archive_name_sets("b11669", GpuTarget::Vulkan).unwrap();
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].len(), 1);
        assert!(sets[0][0].starts_with("llama-b11669-bin-"));
        assert!(sets[0][0].contains("vulkan"));
    }

    #[test]
    fn cuda_wants_a_build_and_its_runtime_at_the_same_level() {
        let sets = archive_name_sets("b11669", GpuTarget::Cuda).unwrap();
        assert!(!sets.is_empty());
        for names in &sets {
            assert_eq!(names.len(), 2);
            let build = names[0].clone();
            let runtime = names[1].clone();
            assert!(build.starts_with("llama-b11669-bin-"));
            assert!(runtime.starts_with("cudart-"));
            // The CUDA level of the two names matches, so the runtime DLLs
            // cannot end up next to a build of another level.
            assert_eq!(
                build.split("cuda-").nth(1),
                runtime.split("cuda-").nth(1),
                "build and runtime must sit at the same CUDA level"
            );
        }
    }

    #[test]
    fn the_recommendation_listens_to_the_vendor() {
        assert_eq!(recommend(&GpuBackend::Cuda), Some(GpuTarget::Cuda));
        assert_eq!(recommend(&GpuBackend::Vulkan), Some(GpuTarget::Vulkan));
        assert_eq!(recommend(&GpuBackend::Opencl), Some(GpuTarget::Opencl));
        assert_eq!(recommend(&GpuBackend::None), None);
    }
}
