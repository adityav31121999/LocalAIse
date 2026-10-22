//! Tauri entry points for the GPU build installer.
//!
//! The settings page lists the backend builds it knows about, sees which one
//! matches the machine's graphics card, and asks for the install. The work
//! itself lives in `runtime::gpu_build`; this module only wires state, the
//! run stop and the capability cache to it.

use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::runtime::gpu_build::{self, GpuTarget};
use crate::AppState;

/// What the settings page shows for the GPU builds of llama.cpp.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuBuildList {
    /// The adapter the machine reports, so the UI can say what was matched.
    pub gpu_name: String,
    /// "CUDA", "Vulkan" or "CPU only" — the detection's own word.
    pub gpu_backend: String,
    /// True when the machine reports a graphics card at all.
    pub gpu_available: bool,
    /// The backend that matches the detected card, for the badge.
    pub recommended: Option<String>,
    /// CUDA, Vulkan and OpenCL, in that order.
    pub builds: Vec<gpu_build::GpuBuildStatus>,
}

/// The state of the GPU builds the app knows about.
#[tauri::command]
pub fn list_gpu_builds(state: tauri::State<'_, AppState>) -> GpuBuildList {
    let gpu = crate::hardware::gpu::detect();
    let builds = gpu_build::status(&state.dirs, &gpu.backend);
    GpuBuildList {
        recommended: gpu_build::recommend(&gpu.backend)
            .map(|target| target.as_str().to_string()),
        gpu_name: gpu.name,
        gpu_backend: gpu.backend.as_str().to_string(),
        gpu_available: gpu.backend != crate::hardware::gpu::GpuBackend::None,
        builds,
    }
}

/// Installs the prebuilt llama.cpp build of `target`.
///
/// The launcher of the running server is stopped first: the install has to
/// replace files the old server may hold open. After the install the build
/// resolver prefers the new build automatically, so the next chat turn runs
/// on the GPU without further configuration.
#[tauri::command]
pub async fn install_gpu_build(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    target: String,
) -> AppResult<String> {
    let target = GpuTarget::parse(&target).ok_or_else(|| {
        AppError::msg("that is not a llama.cpp GPU backend. Expecting vulkan, cuda or opencl.")
    })?;

    let dirs = state.dirs.clone();

    // The install replaces the build on disk, so any running server has to
    // go first.
    if let Ok(mut supervisor) = state.supervisor.lock() {
        supervisor.stop();
    }

    let binary = gpu_build::install(&app, &dirs, target).await?;

    // The resolved build may have changed identity entirely.
    if let Ok(mut cache) = state.capabilities.lock() {
        cache.invalidate(&binary);
        // Every installed build was probed only through the old resolution;
        // a cosmetic extra probe is cheaper than a stale answer.
        cache.clear();
    }

    Ok(binary.to_string_lossy().into_owned())
}
