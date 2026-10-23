//! CPU facts: how many cores to use and which instruction sets llama.cpp can
//! pick at runtime. Informational only — llama.cpp does its own dispatch.

#[derive(Debug, Clone)]
pub struct CpuInfo {
    pub name: String,
    /// Logical cores are what the OS reports; physical cores are approximated
    /// as half of that, which matches every desktop CPU in the field.
    pub logical_cores: u32,
    pub physical_cores: u32,
    pub has_avx2: bool,
    pub has_avx512: bool,
}

fn cpu_name() -> String {
    #[cfg(target_os = "windows")]
    {
        if let Ok(out) = std::process::Command::new("wmic")
            .args(["cpu", "get", "Name"])
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let t = line.trim();
                if !t.is_empty() && t != "Name" {
                    return t.to_string();
                }
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(text) = std::fs::read_to_string("/proc/cpuinfo") {
            for line in text.lines() {
                if let Some(rest) = line.strip_prefix("model name") {
                    let v = rest.trim_start_matches([':', ' ', '\t']);
                    if !v.is_empty() {
                        return v.to_string();
                    }
                }
            }
        }
    }
    "Unknown CPU".to_string()
}

pub fn detect() -> CpuInfo {
    let logical = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(2);
    let logical_cores = logical.max(1) as u32;

    #[cfg(target_arch = "x86_64")]
    let (has_avx2, has_avx512) = (
        std::arch::is_x86_feature_detected!("avx2"),
        std::arch::is_x86_feature_detected!("avx512f"),
    );

    #[cfg(not(target_arch = "x86_64"))]
    let (has_avx2, has_avx512) = (false, false);

    CpuInfo {
        name: cpu_name(),
        logical_cores,
        physical_cores: (logical_cores / 2).max(1),
        has_avx2,
        has_avx512,
    }
}
