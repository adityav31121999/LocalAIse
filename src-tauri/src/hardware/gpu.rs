//! GPU detection: probe for CUDA, Vulkan, OpenCL and Metal availability.
//!
//! Recommends the optimal backend based on detected hardware, drivers,
//! and discrete vs integrated GPU preference.

use serde::Serialize;
use std::sync::OnceLock;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    pub name: String,
    pub vram_gb: Option<f32>,
    pub backend: GpuBackend,
}

#[cfg(target_os = "windows")]
use winreg::enums::{HKEY_LOCAL_MACHINE, RegType};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub enum GpuBackend {
    #[default]
    None,
    Cuda,
    Vulkan,
    Opencl,
    Metal,
}

impl GpuBackend {
    pub fn as_str(&self) -> &'static str {
        match self {
            GpuBackend::None => "CPU only",
            GpuBackend::Cuda => "CUDA",
            GpuBackend::Vulkan => "Vulkan",
            GpuBackend::Opencl => "OpenCL",
            GpuBackend::Metal => "Metal",
        }
    }
}

/// The recommended backend for the current hardware.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuRecommendation {
    pub vendor: String,
    pub device_name: String,
    pub backend: String,
    pub reason: String,
    pub warnings: Vec<String>,
}

/// Heuristically detect the best GPU backend available on this system.
///
/// Detected once per process: the answer shells out to `nvidia-smi` (and
/// `wmic` as a fallback), which can take seconds, and neither the adapters
/// nor the drivers change while the app runs.
pub fn detect() -> GpuInfo {
    static DETECTED: OnceLock<GpuInfo> = OnceLock::new();
    DETECTED.get_or_init(detect_once).clone()
}

fn detect_once() -> GpuInfo {
    let rec = recommend_gpu();
    let backend = match rec.backend.as_str() {
        "cuda" => GpuBackend::Cuda,
        "vulkan" => GpuBackend::Vulkan,
        "opencl" => GpuBackend::Opencl,
        "metal" => GpuBackend::Metal,
        _ => GpuBackend::None,
    };

    let mut info = GpuInfo {
        name: rec.device_name,
        vram_gb: None,
        backend,
    };

    #[cfg(target_os = "windows")]
    {
        if let Some((bytes, _)) = adapter_info_windows() {
            info.vram_gb = Some(bytes as f32 / (1024.0 * 1024.0 * 1024.0));
        } else if info.backend == GpuBackend::Cuda {
            info.vram_gb = vram_from_nvidia_smi();
        }
    }

    #[cfg(target_os = "linux")]
    {
        if info.backend == GpuBackend::Cuda {
            info.vram_gb = vram_from_nvidia_smi();
        }
    }

    info
}

/// Full GPU recommendation logic considering vendor, dGPU vs iGPU, and driver version.
pub fn recommend_gpu() -> GpuRecommendation {
    static RECOMMENDED: OnceLock<GpuRecommendation> = OnceLock::new();
    RECOMMENDED.get_or_init(recommend_once).clone()
}

fn recommend_once() -> GpuRecommendation {
    let mut warnings = Vec::new();

    #[cfg(target_os = "windows")]
    {
        let has_cuda = std::path::Path::new(r"C:\Windows\System32\nvcuda.dll").exists()
            || std::path::Path::new(r"C:\Windows\SysWOW64\nvcuda.dll").exists();
        let has_vulkan = std::path::Path::new(r"C:\Windows\System32\vulkan-1.dll").exists()
            || std::path::Path::new(r"C:\Windows\SysWOW64\vulkan-1.dll").exists();
        let has_opencl = std::path::Path::new(r"C:\Windows\System32\OpenCL.dll").exists()
            || std::path::Path::new(r"C:\Windows\SysWOW64\OpenCL.dll").exists();

        let mut adapters = list_adapters_windows();
        if adapters.is_empty() {
            if let Ok(out) = std::process::Command::new("wmic")
                .args(["path", "win32_VideoController", "get", "Name"])
                .output()
            {
                let text = String::from_utf8_lossy(&out.stdout);
                for line in text.lines() {
                    let t = line.trim();
                    if !t.is_empty() && t != "Name" {
                        adapters.push(AdapterRecord { name: t.to_string(), vram_bytes: 0 });
                    }
                }
            }
        }
        let dgpu = adapters.iter().find(|a| is_discrete_gpu(&a.name));
        let best_adapter = dgpu.or_else(|| adapters.first());

        if has_cuda {
            let driver_ver = nvidia_driver_version();
            // CUDA 12.4 requires >= 550.58 on Windows
            if let Some(ref ver) = driver_ver {
                if let Some(major) = ver.split('.').next().and_then(|m| m.parse::<u32>().ok()) {
                    if major < 550 {
                        warnings.push(format!(
                            "NVIDIA driver version ({ver}) is older than recommended (550.58+). Please update driver if CUDA fails."
                        ));
                    }
                }
            }

            let name = best_adapter
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "NVIDIA Graphics Device".to_string());

            return GpuRecommendation {
                vendor: "NVIDIA".to_string(),
                device_name: name,
                backend: "cuda".to_string(),
                reason: "NVIDIA discrete GPU detected with CUDA support.".to_string(),
                warnings,
            };
        }

        if has_vulkan {
            let name = best_adapter
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "GPU (Vulkan Compatible)".to_string());
            let vendor = if name.to_lowercase().contains("amd") || name.to_lowercase().contains("radeon") {
                "AMD"
            } else if name.to_lowercase().contains("intel") {
                "Intel"
            } else {
                "Generic"
            };

            return GpuRecommendation {
                vendor: vendor.to_string(),
                device_name: name,
                backend: "vulkan".to_string(),
                reason: format!("{vendor} GPU detected with Vulkan support."),
                warnings,
            };
        }

        if has_opencl {
            let name = best_adapter
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "GPU (OpenCL Compatible)".to_string());
            return GpuRecommendation {
                vendor: "OpenCL".to_string(),
                device_name: name,
                backend: "opencl".to_string(),
                reason: "OpenCL runtime detected.".to_string(),
                warnings,
            };
        }
    }

    #[cfg(target_os = "linux")]
    {
        let has_cuda = std::path::Path::new("/dev/nvidia0").exists()
            || std::path::Path::new("/usr/lib/x86_64-linux-gnu/libcuda.so.1").exists()
            || std::path::Path::new("/usr/lib64/libcuda.so.1").exists();

        let has_vulkan = std::path::Path::new("/dev/dri/renderD128").exists()
            || std::path::Path::new("/usr/lib/x86_64-linux-gnu/libvulkan.so.1").exists()
            || std::path::Path::new("/usr/lib64/libvulkan.so.1").exists();

        let has_opencl = std::path::Path::new("/etc/OpenCL/vendors").exists()
            || std::path::Path::new("/usr/lib/x86_64-linux-gnu/libOpenCL.so.1").exists()
            || std::path::Path::new("/usr/lib64/libOpenCL.so.1").exists();

        if has_cuda {
            let driver_ver = nvidia_driver_version();
            if let Some(ref ver) = driver_ver {
                if let Some(major) = ver.split('.').next().and_then(|m| m.parse::<u32>().ok()) {
                    if major < 550 {
                        warnings.push(format!(
                            "NVIDIA driver version ({ver}) is older than 550.x. Consider updating your driver."
                        ));
                    }
                }
            }

            return GpuRecommendation {
                vendor: "NVIDIA".to_string(),
                device_name: "NVIDIA GPU (CUDA)".to_string(),
                backend: "cuda".to_string(),
                reason: "NVIDIA GPU detected with CUDA support on Linux.".to_string(),
                warnings,
            };
        }

        if has_vulkan {
            return GpuRecommendation {
                vendor: "AMD/Intel".to_string(),
                device_name: "GPU (Vulkan)".to_string(),
                backend: "vulkan".to_string(),
                reason: "Vulkan device driver detected on Linux.".to_string(),
                warnings,
            };
        }

        if has_opencl {
            return GpuRecommendation {
                vendor: "Generic".to_string(),
                device_name: "GPU (OpenCL)".to_string(),
                backend: "opencl".to_string(),
                reason: "OpenCL runtime detected on Linux.".to_string(),
                warnings,
            };
        }
    }

    #[cfg(target_os = "macos")]
    {
        return GpuRecommendation {
            vendor: "Apple".to_string(),
            device_name: "Apple Silicon GPU (Metal)".to_string(),
            backend: "metal".to_string(),
            reason: "Apple Metal acceleration available natively.".to_string(),
            warnings,
        };
    }

    // Fallback: CPU only
    GpuRecommendation {
        vendor: "CPU".to_string(),
        device_name: "CPU Fallback".to_string(),
        backend: "none".to_string(),
        reason: "No supported discrete or integrated GPU detected. Running on CPU.".to_string(),
        warnings,
    }
}

#[allow(dead_code)]
fn is_discrete_gpu(name: &str) -> bool {
    let lower = name.to_lowercase();
    (lower.contains("geforce")
        || lower.contains("rtx")
        || lower.contains("gtx")
        || lower.contains("radeon rx")
        || lower.contains("arc a")
        || lower.contains("discrete")
        || lower.contains("quadro")
        || lower.contains("tesla"))
        && !lower.contains("graphics 6")
        && !lower.contains("uhd")
        && !lower.contains("iris")
}

#[derive(Debug)]
#[allow(dead_code)]
struct AdapterRecord {
    pub name: String,
    pub vram_bytes: u64,
}

#[cfg(target_os = "windows")]
fn list_adapters_windows() -> Vec<AdapterRecord> {
    use winreg::RegKey;

    let class_key = r"SYSTEM\CurrentControlSet\Control\Class\{4D36E968-E325-11CE-BFC1-08002BE10318}";
    let Ok(class) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(class_key) else {
        return Vec::new();
    };

    let mut list = Vec::new();

    for index in 0..10 {
        let sub = format!("{index:04}");
        let Ok(adapter) = class.open_subkey(&sub) else {
            continue;
        };

        let bytes = [
            "HardwareInformation.qwMemorySize",
            "HardwareInformation.MemorySize",
        ]
        .iter()
        .find_map(|name| {
            let raw = adapter.get_raw_value(name).ok()?;
            reg_value_bytes(raw.vtype, &raw.bytes)
        })
        .unwrap_or(0);

        if let Ok(desc) = adapter.get_value::<String, _>("DriverDesc") {
            if !desc.trim().is_empty() {
                list.push(AdapterRecord {
                    name: desc,
                    vram_bytes: bytes,
                });
            }
        }
    }

    list
}

#[cfg(target_os = "windows")]
fn adapter_info_windows() -> Option<(u64, Option<String>)> {
    let adapters = list_adapters_windows();
    // Prefer a discrete adapter first, then the one with the most VRAM.
    let best = adapters
        .iter()
        .filter(|a| is_discrete_gpu(&a.name))
        .max_by_key(|a| a.vram_bytes)
        .or_else(|| adapters.iter().max_by_key(|a| a.vram_bytes))?;
    if best.vram_bytes == 0 && best.name.is_empty() {
        return None;
    }
    Some((best.vram_bytes, Some(best.name.clone())))
}

#[cfg(target_os = "windows")]
fn reg_value_bytes(vtype: RegType, bytes: &[u8]) -> Option<u64> {
    match vtype {
        RegType::REG_QWORD => Some(u64::from_le_bytes(bytes.try_into().ok()?)),
        RegType::REG_DWORD => Some(u32::from_le_bytes(bytes.try_into().ok()?) as u64),
        RegType::REG_BINARY => match bytes.len() {
            4 => Some(u32::from_le_bytes(bytes.try_into().ok()?) as u64),
            _ => Some(u64::from_le_bytes(bytes.try_into().ok()?)),
        },
        _ => None,
    }
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn nvidia_driver_version() -> Option<String> {
    let output = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=driver_version", "--format=csv,noheader"])
        .output()
        .ok()?;
    let text = std::str::from_utf8(&output.stdout).ok()?;
    let line = text.lines().next()?.trim();
    if line.is_empty() {
        None
    } else {
        Some(line.to_string())
    }
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn vram_from_nvidia_smi() -> Option<f32> {
    let output = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;
    let first = std::str::from_utf8(&output.stdout)
        .ok()?
        .lines()
        .next()?
        .trim()
        .to_string();
    let mibs: f32 = first.lines().next().and_then(|line| line.trim().parse().ok())?;
    (mibs > 0.0).then_some(mibs / 1024.0)
}
