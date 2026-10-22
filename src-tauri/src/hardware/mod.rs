pub mod cpu;
pub mod gpu;
pub mod memory;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareReport {
    pub total_ram_gb: f32,
    /// RAM that is free right now.
    pub available_ram_gb: f32,
    pub cpu_cores: usize,
    pub cpu_physical_cores: u32,
    /// Informational, shown in the model manager. llama.cpp dispatches on its
    /// own at runtime, so nothing decides based on this.
    pub cpu_avx2: bool,
    pub cpu_avx512: bool,
    pub gpu_name: String,
    pub gpu_backend: String,
    /// Total VRAM of the detected adapter, when it could be read. GPU mode
    /// memory budgeting uses this; None hides the VRAM bar.
    pub gpu_vram_gb: Option<f32>,
    /// True when the machine has a graphics card the app could use.
    pub gpu_available: bool,
    /// "small" (3-4B) | "medium" (7-8B) | "large" (13B+)
    pub recommended_tier: String,
}

/// Detect basic host hardware without loading any GPU runtime.
pub fn probe_hardware() -> HardwareReport {
    let cpu = cpu::detect();
    let mem = memory::read();
    let gpu = gpu::detect();

    let total_ram_gb = if mem.total_gb > 0.0 { mem.total_gb } else { 16.0 };

    let recommended_tier = if total_ram_gb < 10.0 {
        "Small (3–4B, Q4_K_M)".to_string()
    } else if total_ram_gb < 20.0 {
        "Medium (7–8B, Q4_K_M)".to_string()
    } else {
        "Large (13B+, Q4_K_M)".to_string()
    };

    HardwareReport {
        total_ram_gb,
        available_ram_gb: if mem.available_gb > 0.0 {
            mem.available_gb
        } else {
            total_ram_gb
        },
        cpu_cores: cpu.logical_cores as usize,
        cpu_physical_cores: cpu.physical_cores,
        cpu_avx2: cpu.has_avx2,
        cpu_avx512: cpu.has_avx512,
        gpu_name: gpu.name,
        gpu_backend: gpu.backend.as_str().to_string(),
        gpu_vram_gb: gpu.vram_gb,
        gpu_available: gpu.backend != gpu::GpuBackend::None,
        recommended_tier,
    }
}
