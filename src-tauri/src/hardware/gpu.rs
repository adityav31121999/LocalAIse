//! GPU detection: probe for CUDA, Vulkan, and ROCm availability.
//!
//! This does not load any GPU runtime DLL — it only checks which device
//! files or registry keys are present, so it cannot crash on systems
//! without a GPU. VRAM size comes from the display-adapter registry class
//! (no driver runtime needed), with nvidia-smi as fallback.

#[derive(Debug, Clone, Default)]
pub struct GpuInfo {
    pub name: String,
    pub vram_gb: Option<f32>,
    pub backend: GpuBackend,
}

#[cfg(target_os = "windows")]
use winreg::enums::{HKEY_LOCAL_MACHINE, RegType};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
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

/// Heuristically detect the best GPU backend available on this system.
///
/// Returns `GpuInfo` with a backend type, a name and, when the adapter
/// exposes it, the real VRAM size.
pub fn detect() -> GpuInfo {
    // On Windows: check for nvcuda.dll (NVIDIA CUDA), then Vulkan loaders, then OpenCL.dll
    #[cfg(target_os = "windows")]
    {
        let mut info = GpuInfo {
            name: String::new(),
            vram_gb: None,
            backend: GpuBackend::None,
        };

        let (name, backend) = if std::path::Path::new(r"C:\Windows\System32\nvcuda.dll").exists() {
            ("NVIDIA GPU (CUDA)", GpuBackend::Cuda)
        } else if std::path::Path::new(r"C:\Windows\System32\amdvlk64.dll").exists() {
            // AMD/Intel Vulkan: amdvlk64.dll or igvk64.dll
            ("AMD GPU (Vulkan)", GpuBackend::Vulkan)
        } else if std::path::Path::new(r"C:\Windows\System32\igvk64.dll").exists() {
            ("Intel GPU (Vulkan)", GpuBackend::Vulkan)
        } else if std::path::Path::new(r"C:\Windows\System32\vulkan-1.dll").exists() {
            // The Vulkan loader is present on almost every machine with any
            // graphics card, driver runtime included.
            ("GPU (Vulkan)", GpuBackend::Vulkan)
        } else if std::path::Path::new(r"C:\Windows\System32\OpenCL.dll").exists() {
            ("GPU (OpenCL)", GpuBackend::Opencl)
        } else {
            return info;
        };

        info.backend = backend.clone();
        info.name = name.to_string();

        // The display-adapter class key carries the real VRAM size and the
        // human-readable adapter name; nvidia-smi is a CUDA-only fallback.
        match adapter_info_windows() {
            Some((bytes, driver_desc)) => {
                info.vram_gb = Some(bytes as f32 / (1024.0 * 1024.0 * 1024.0));
                if let Some(desc) = driver_desc.filter(|desc| !desc.trim().is_empty()) {
                    info.name = desc;
                }
            }
            None if backend == GpuBackend::Cuda => {
                info.vram_gb = vram_from_nvidia_smi();
            }
            None => {}
        }
        return info;
    }

    // On Linux: check for /dev/dri/renderD128 (DRI/Mesa), /dev/nvidia0 (NVIDIA), or OpenCL ICD
    #[cfg(target_os = "linux")]
    {
        if std::path::Path::new("/dev/nvidia0").exists() {
            return GpuInfo {
                name: "NVIDIA GPU (CUDA)".to_string(),
                vram_gb: vram_from_nvidia_smi(),
                backend: GpuBackend::Cuda,
            };
        }
        if std::path::Path::new("/dev/dri/renderD128").exists() {
            return GpuInfo {
                name: "GPU (Vulkan/Mesa)".to_string(),
                vram_gb: None,
                backend: GpuBackend::Vulkan,
            };
        }
        if std::path::Path::new("/etc/OpenCL/vendors").exists()
            || std::path::Path::new("/usr/lib/x86_64-linux-gnu/libOpenCL.so.1").exists()
        {
            return GpuInfo {
                name: "GPU (OpenCL)".to_string(),
                vram_gb: None,
                backend: GpuBackend::Opencl,
            };
        }
        GpuInfo::default()
    }

    // macOS: Metal is always available
    #[cfg(target_os = "macos")]
    {
        return GpuInfo {
            name: "Apple GPU (Metal)".to_string(),
            vram_gb: None,
            backend: GpuBackend::Metal,
        };
    }
}

#[cfg(target_os = "windows")]
fn adapter_info_windows() -> Option<(u64, Option<String>)> {
    use winreg::RegKey;

    let class_key = r"SYSTEM\CurrentControlSet\Control\Class\{4D36E968-E325-11CE-BFC1-08002BE10318}";
    let class = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(class_key).ok()?;

    let mut best_bytes: u64 = 0;
    let mut best_name: Option<String> = None;

    // 0000..0009 covers most multi-adapter desktops and laptops.
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
        });
        if let Some(bytes) = bytes.filter(|bytes| *bytes > best_bytes) {
            best_bytes = bytes;
            // DriverDesc accompanies the real adapter on the same key.
            best_name = adapter.get_value("DriverDesc").ok();
        }
    }

    if best_bytes == 0 {
        return None;
    }

    Some((best_bytes, best_name))
}

/// `HardwareInformation.(qw)MemorySize` arrives as QWORD, DWORD or a raw
/// byte blob depending on driver; handle all three.
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

/// Drivers usually ship nvidia-smi next to the runtime; it reports the real
/// total, unlike the registry which can miss integrated or exotic adapters.
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
