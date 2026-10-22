//! Which llama.cpp build to run, and what it can do.
//!
//! The installer ships one build per platform: CPU only. GPU builds are not
//! bundled, but they can be installed from inside the app
//! (`runtime::gpu_build` downloads the official prebuilt llama.cpp archives
//! for Vulkan and CUDA) or built locally and pointed at in settings. Even on
//! machines with GPUs, the default binary therefore cannot allocate VRAM.
//!
//! The `Auto` device resolves against both facts: the hardware has to
//! report a GPU, and the build in use has to be able to offload to it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

use crate::config::app_config::Settings;
use crate::config::paths::AppDirs;
use crate::runtime::budget::{auto_gpu_layers, ModelShape};
use crate::runtime::gpu_build::{backend_dll_installed, installed_binary, platform_dir, GpuTarget};
use crate::runtime::profile::{ComputeDevice, InferenceProfile};

fn platform_prefix() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows-x64"
    } else {
        "linux-x64"
    }
}

fn executable_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "llama-server.exe"
    } else {
        "llama-server"
    }
}

/// A GPU the runtime can see.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuDevice {
    /// The adapter name the server reports.
    pub name: String,
    /// "CUDA", "Vulkan" or "Metal".
    pub backend: String,
    /// VRAM the adapter reports, when it says so.
    pub vram_gb: Option<f32>,
}

/// What a llama-server build can do.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    /// True when the build lists at least one GPU device.
    pub supports_gpu: bool,
    pub devices: Vec<GpuDevice>,
    /// False when the probe could not run at all.
    pub probed: bool,
    /// Whether the build exposes the flash-attention flag.
    ///
    /// `None` when the build could not be asked — the help output was empty
    /// or the process refused to run it. The UI says so rather than pretending
    /// the feature is there.
    pub flash_attn: Option<bool>,
}

impl ServerCapabilities {
    /// The largest VRAM the build can see, in bytes.
    pub fn best_vram_bytes(&self) -> Option<u64> {
        self.devices
            .iter()
            .filter_map(|device| device.vram_gb)
            .fold(None, |best: Option<f32>, vram| match best {
                Some(current) if current >= vram => Some(current),
                _ => Some(vram),
            })
            .map(|gigabytes| (gigabytes * 1024.0 * 1024.0 * 1024.0) as u64)
    }

    /// The backend the build would use, e.g. "CUDA".
    pub fn backend_name(&self) -> Option<&str> {
        self.devices.first().map(|device| device.backend.as_str())
    }
}

/// Cached capability probe, one entry per llama-server binary.
#[derive(Debug, Default)]
pub struct CapabilityCache {
    entries: HashMap<PathBuf, ServerCapabilities>,
}

impl CapabilityCache {
    /// The capabilities of a build, probing it the first time only.
    pub fn get_or_probe(&mut self, binary: &Path) -> ServerCapabilities {
        if let Some(found) = self.entries.get(binary) {
            return found.clone();
        }
        let probed = detect_capabilities(binary);
        self.entries.insert(binary.to_path_buf(), probed.clone());
        probed
    }

    /// Forgets a cached probe, so the next question re-runs the binary.
    pub fn invalidate(&mut self, binary: &Path) {
        self.entries.remove(binary);
    }

    /// Forgets every cached probe, so the next questions start fresh.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// The capabilities of the build that would run for these settings.
///
/// The probe is cached per binary, so the server is only asked once.
pub fn capabilities_for(
    dirs: &AppDirs,
    settings: &Settings,
    cache: &mut CapabilityCache,
) -> ServerCapabilities {
    match resolve_server(dirs, settings) {
        Some(binary) => cache.get_or_probe(&binary),
        None => ServerCapabilities::default(),
    }
}

/// Asks a build which devices it can use.
///
/// The probe reads the server's own device list, so a build that cannot see
/// a GPU says so instead of the app guessing from the driver files.
pub fn detect_capabilities(binary: &Path) -> ServerCapabilities {
    let Ok(output) = Command::new(binary).arg("--list-devices").output() else {
        return ServerCapabilities::default();
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut devices = Vec::new();

    for line in text.lines() {
        let Some((backend, name)) = parse_device_line(line) else {
            continue;
        };
        devices.push(GpuDevice {
            name,
            backend: backend.to_string(),
            vram_gb: parse_vram(line),
        });
    }

    ServerCapabilities {
        supports_gpu: !devices.is_empty(),
        devices,
        probed: true,
        flash_attn: probe_flash_attn(binary),
    }
}
/// Asks a build whether it knows the flash-attention flag.
///
/// The answer is a tri-state: `Some(true)` and `Some(false)` are the build's
/// own words, `None` means the build could not be asked.
fn probe_flash_attn(binary: &Path) -> Option<bool> {
    let output = Command::new(binary).arg("--help").output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let help = format!("{stdout}\n{stderr}").to_lowercase();
    if !help.contains("flash") {
        return None;
    }
    Some(help.contains("flash-attn") || help.contains("flash_attn"))
}

/// Reads a device line such as "CUDA0: NVIDIA GeForce RTX 4090 (VRAM: 24564 MiB)".
///
/// Returns the backend and the adapter name. Lines that merely mention a
/// backend — a header, or a sentence saying no device was found — are not
/// device lines and are skipped: their label does not name a device slot
/// like `CUDA0`.
pub fn parse_device_line(line: &str) -> Option<(&'static str, String)> {
    let trimmed = line.trim();
    let (label, rest) = trimmed.split_once(':')?;
    let label = label.trim();

    let (backend, slot) = if let Some(slot) = label.strip_prefix("CUDA") {
        ("CUDA", slot)
    } else if let Some(slot) = label.strip_prefix("Vulkan") {
        ("Vulkan", slot)
    } else if let Some(slot) = label.strip_prefix("OpenCL") {
        ("OpenCL", slot)
    } else {
        let slot = label.strip_prefix("Metal")?;
        ("Metal", slot)
    };

    // A device names its slot: "CUDA0". Prose names a topic: "CUDA backends".
    // Only a bare or numbered slot survived the doc-level prose filter.
    if !(slot.is_empty() || slot.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }

    let name = rest.trim();
    if name.is_empty() {
        return None;
    }

    Some((backend, name.to_string()))
}

/// Reads the VRAM a device line reports, in gigabytes.
fn parse_vram(line: &str) -> Option<f32> {
    let lowered = line.to_lowercase();
    let start = lowered.find("vram")? + "vram".len();
    let rest = &line[start..];

    // Skip to the number: the label is followed by a colon or a space.
    let digits_start = rest.find(|c: char| c.is_ascii_digit())?;
    let tail = &rest[digits_start..];

    let mut number = String::new();
    for ch in tail.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            number.push(ch);
        } else {
            break;
        }
    }
    let value: f32 = number.parse().ok()?;
    if value <= 0.0 {
        return None;
    }

    let tail = tail[number.len()..].to_lowercase();
    if tail.contains("gib") || tail.contains("gb") {
        Some(value)
    } else if tail.contains("mib") || tail.contains("mb") {
        Some(value / 1024.0)
    } else {
        None
    }
}

/// The CPU-capable candidates, in order.
///
/// A single flat platform folder holds all files (CPU dispatch tiers plus
/// any GPU backend DLLs). The app-data dir is checked first, then the
/// `resources/llama/<platform>` repo folder (dev mode), then the vendored
/// llama.cpp checkout build output.
fn cpu_candidates(dirs: &AppDirs) -> Vec<PathBuf> {
    let executable = executable_name();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");

    vec![
        platform_dir(dirs).join(executable),
        repo.join("resources").join("llama").join(platform_prefix()).join(executable),
        repo.join("vendor/llama.cpp/build/bin").join(executable),
        repo.join("vendor/llama.cpp/build/bin/Release").join(executable),
    ]
}

/// The GPU-capable server binary, when any accelerated backend DLL is present.
///
/// All GPU backends share the same launcher (`llama-server`) in the flat
/// platform folder. The presence of `ggml-cuda.dll`, `ggml-vulkan.dll` or
/// `ggml-opencl.dll` next to it determines which backends are active at
/// runtime (the `--list-devices` probe confirms this). CUDA wins when
/// multiple backends are installed on the same machine.
fn gpu_candidates(dirs: &AppDirs) -> Vec<PathBuf> {
    let has_any_gpu = [GpuTarget::Cuda, GpuTarget::Vulkan, GpuTarget::Opencl]
        .iter()
        .any(|t| backend_dll_installed(dirs, *t));
    if has_any_gpu {
        installed_binary(dirs, GpuTarget::Cuda)
            .into_iter()
            .collect()
    } else {
        vec![]
    }
}

/// The first candidate that exists on disk.
fn resolve_from(candidates: Vec<PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|candidate| candidate.is_file())
}

/// The candidates of the preferred build, in order: an explicit user
/// selection first — it is always what runs — then an installed GPU build,
/// then the CPU build. This is what an `Auto` or `GPU` device asks for, and
/// what the capability probe describes.
pub fn resolve_server(dirs: &AppDirs, settings: &Settings) -> Option<PathBuf> {
    resolve_from(best_candidates_with(dirs, settings, true))
}

/// The candidates of a run that must stay on the CPU.
///
/// A GPU build would still work — the launcher pins `--device none` — but the
/// CPU build is the one with the tuned per-microarchitecture dispatch
/// variants, so a CPU-only run always picks from the CPU candidates.
fn resolve_cpu_server(dirs: &AppDirs, settings: &Settings) -> Option<PathBuf> {
    resolve_from(best_candidates_with(dirs, settings, false))
}

/// Candidate list with an optional GPU head start.
fn best_candidates_with(dirs: &AppDirs, settings: &Settings, prefer_gpu: bool) -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Some(path) = &settings.llama_server_path {
        candidates.push(path.clone());
    }
    if prefer_gpu {
        candidates.extend(gpu_candidates(dirs));
    }
    candidates.extend(cpu_candidates(dirs));

    candidates
}

/// What the runtime will actually run: which binary, and on what device.
#[derive(Debug, Clone)]
pub struct RuntimePlan {
    /// The llama-server binary to start.
    pub server: PathBuf,
    /// The device the profile resolves to — never `Auto`.
    pub device: ComputeDevice,
    /// The profile with its device resolved.
    pub profile: InferenceProfile,
    /// What the chosen build can do.
    pub capabilities: ServerCapabilities,
}

/// Resolves the binary and the device for a profile.
///
/// The binary is chosen by the device the profile asks for: a CPU run uses
/// the CPU build, anything that may use the GPU prefers an installed GPU
/// build. `Auto` then settles against what that build reports, and when it
/// settles to the CPU the tuned CPU build runs even though a GPU build is
/// installed — the binary and the device pair up the honest way, so the
/// startup check can stay strict.
///
/// An explicit GPU request without any GPU-capable build installed is
/// refused: a silent CPU fallback would be dishonest about what the user
/// asked for.
pub fn plan(
    dirs: &AppDirs,
    settings: &Settings,
    profile: &InferenceProfile,
    shape: Option<&ModelShape>,
    cache: &mut CapabilityCache,
) -> Result<RuntimePlan, String> {
    if let Some(path) = &settings.llama_server_path {
        if !path.is_file() {
            return Err("the selected llama-server build no longer exists on disk".to_string());
        }
    }

    let Some(mut server) = resolve_server(dirs, settings) else {
        return Err(
            "no llama-server build found on disk. The installer ships one; \
             when running from source, build it with `npm run fetch-llama-server`."
                .to_string(),
        );
    };

    let mut capabilities = cache.get_or_probe(&server);
    let mut device = resolve_device(profile, &capabilities, shape);

    // Without a user-pinned build the app is free to hand a CPU run to the
    // tuned CPU build — when the preferred build settles below the GPU, or
    // the device setting was CPU all along, and the two are not the same file.
    if settings.llama_server_path.is_none() {
        let wants_cpu = matches!(profile.device, ComputeDevice::Cpu);
        let settles_cpu = device == ComputeDevice::Cpu;
        if wants_cpu || settles_cpu {
            if let Some(cpu) = resolve_cpu_server(dirs, settings) {
                if cpu != server {
                    capabilities = cache.get_or_probe(&cpu);
                    server = cpu;
                    device = resolve_device(profile, &capabilities, shape);
                }
            }
        }
    }

    if matches!(profile.device, ComputeDevice::UserGpu { .. }) && !capabilities.supports_gpu {
        return Err(
            "GPU mode is on but the build in use reports no GPU device. Install a Vulkan or \
             CUDA build in the hardware settings, or set the device back to CPU."
                .to_string(),
        );
    }

    let mut resolved = profile.clone();
    resolved.device = device;

    Ok(RuntimePlan {
        server,
        device,
        profile: resolved,
        capabilities: capabilities.clone(),
    })
}

/// The device a profile runs on once `Auto` is settled.
pub fn resolve_device(
    profile: &InferenceProfile,
    capabilities: &ServerCapabilities,
    shape: Option<&ModelShape>,
) -> ComputeDevice {
    match profile.device {
        ComputeDevice::Auto => {
            // The GPU is only usable when the build in use can offload to it.
            if !capabilities.supports_gpu {
                return ComputeDevice::Cpu;
            }
            let Some(vram) = capabilities.best_vram_bytes() else {
                return ComputeDevice::Cpu;
            };
            let Some(shape) = shape else {
                // Without a model shape the layer count is unknown, so ask
                // the server to offload as much as fits and let it decide.
                return ComputeDevice::UserGpu { gpu_layers: 999 };
            };
            match auto_gpu_layers(shape, profile, vram) {
                Some(layers) if layers > 0 => ComputeDevice::UserGpu { gpu_layers: layers },
                _ => ComputeDevice::Cpu,
            }
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_lines_are_recognized() {
        let (backend, name) =
            parse_device_line("  CUDA0: NVIDIA GeForce RTX 4090 (VRAM: 24564 MiB)").unwrap();
        assert_eq!(backend, "CUDA");
        assert!(name.starts_with("NVIDIA GeForce RTX 4090"));
    }

    #[test]
    fn headers_and_prose_are_not_device_lines() {
        assert!(parse_device_line("Available devices:").is_none());
        assert!(parse_device_line("no GPU devices found").is_none());
        assert!(parse_device_line("").is_none());
    }

    #[test]
    fn opencl_lines_are_device_lines() {
        let (backend, name) = parse_device_line("OpenCL0: Intel(R) Arc A770 Graphics").unwrap();
        assert_eq!(backend, "OpenCL");
        assert!(name.starts_with("Intel"));
        // A line that only names the backend in prose is still not a device.
        assert!(parse_device_line("OpenCL backends: none").is_none());
    }

    #[test]
    fn vram_is_read_in_gigabytes() {
        assert_eq!(
            parse_vram("CUDA0: NVIDIA GeForce RTX 4090 (VRAM: 24564 MiB)"),
            Some(24564.0 / 1024.0)
        );
        assert_eq!(parse_vram("CUDA0: NVIDIA GeForce RTX 4090 (VRAM: 24.0 GiB)"), Some(24.0));
        assert_eq!(parse_vram("CUDA0: NVIDIA GeForce RTX 4090"), None);
    }

    #[test]
    fn auto_resolves_to_the_cpu_when_the_build_has_no_gpu() {
        let profile = InferenceProfile::desktop_default();
        let device = resolve_device(&profile, &ServerCapabilities::default(), None);
        assert_eq!(device, ComputeDevice::Cpu);
    }

    #[test]
    fn auto_resolves_to_the_gpu_when_the_build_reports_one() {
        let profile = InferenceProfile::desktop_default();
        let capabilities = ServerCapabilities {
            supports_gpu: true,
            devices: vec![GpuDevice {
                name: "NVIDIA GeForce RTX 4090".into(),
                backend: "CUDA".into(),
                vram_gb: Some(24.0),
            }],
            probed: true,
            flash_attn: Some(true),
        };
        let device = resolve_device(&profile, &capabilities, None);
        assert_eq!(device, ComputeDevice::UserGpu { gpu_layers: 999 });
    }

    #[test]
    fn an_explicit_cpu_request_stays_on_the_cpu() {
        let mut profile = InferenceProfile::desktop_default();
        profile.device = ComputeDevice::Cpu;
        let capabilities = ServerCapabilities {
            supports_gpu: true,
            devices: vec![GpuDevice {
                name: "NVIDIA".into(),
                backend: "CUDA".into(),
                vram_gb: Some(24.0),
            }],
            probed: true,
            flash_attn: Some(true),
        };
        assert_eq!(resolve_device(&profile, &capabilities, None), ComputeDevice::Cpu);
    }

    #[test]
    fn resolution_prefers_the_gpu_build_but_knows_the_cpu_build() {
        let root = std::env::temp_dir().join(format!("localaise-backend-{}", std::process::id()));
        let llama_dir = root.join("llama");
        std::fs::create_dir_all(&llama_dir).unwrap();

        // Two fake builds: one GPU one in place, one CPU build next to it.
        let gpu_dir = llama_dir.join(format!("{}-vulkan", platform_prefix()));
        let cpu_dir = llama_dir.join(format!("{}-cpu", platform_prefix()));
        std::fs::create_dir_all(&gpu_dir).unwrap();
        std::fs::create_dir_all(&cpu_dir).unwrap();
        std::fs::write(gpu_dir.join(executable_name()), "x").unwrap();
        std::fs::write(cpu_dir.join(executable_name()), "x").unwrap();

        let dirs = crate::config::paths::AppDirs {
            base: root.clone(),
            state_file: root.join("state.json"),
            models: root.join("models"),
            llama: llama_dir,
            default_projects: root.join("projects"),
            default_personal: root.join("personal"),
        };
        let settings = Settings::default();

        // The preferred question answers with the GPU build; the CPU-only
        // question with the CPU build.
        assert_eq!(
            resolve_server(&dirs, &settings),
            Some(gpu_dir.join(executable_name()))
        );
        assert_eq!(
            resolve_cpu_server(&dirs, &settings),
            Some(cpu_dir.join(executable_name()))
        );

        std::fs::remove_dir_all(&root).ok();
    }
}
