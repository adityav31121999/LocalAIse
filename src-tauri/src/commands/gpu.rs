//! Tauri entry points for GPU detection and backend management.

use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::hardware::gpu::{self, GpuRecommendation};
use crate::runtime::gpu_build::{self, GpuTarget};
use crate::AppState;

/// What the settings page shows for the GPU builds of llama.cpp.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuBuildList {
    pub gpu_name: String,
    pub gpu_backend: String,
    pub gpu_available: bool,
    pub recommended: Option<String>,
    pub builds: Vec<gpu_build::GpuBuildStatus>,
    pub recommendation: GpuRecommendation,
}

/// Detects the machine's GPU and returns full recommendation details.
#[tauri::command]
pub fn detect_gpu() -> GpuRecommendation {
    gpu::recommend_gpu()
}

/// The state of the GPU builds the app knows about.
#[tauri::command]
pub fn list_gpu_builds(state: tauri::State<'_, AppState>) -> GpuBuildList {
    list_backends(state)
}

/// Lists all known backends (aliased command per spec).
#[tauri::command]
pub fn list_backends(state: tauri::State<'_, AppState>) -> GpuBuildList {
    let rec = gpu::recommend_gpu();
    let gpu_info = gpu::detect();
    let builds = gpu_build::status(&state.dirs, &gpu_info.backend);
    GpuBuildList {
        recommended: gpu_build::recommend(&gpu_info.backend)
            .map(|target| target.as_str().to_string()),
        gpu_name: rec.device_name.clone(),
        gpu_backend: rec.backend.clone(),
        gpu_available: rec.backend != "none",
        builds,
        recommendation: rec,
    }
}

/// Installs a backend.
#[tauri::command]
pub async fn install_backend(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    backend: String,
) -> AppResult<String> {
    install_gpu_build(app, state, backend).await
}

/// Installs the prebuilt llama.cpp build of `target`.
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

    // The install replaces the build on disk, so any running server has to go first.
    if let Ok(mut supervisor) = state.supervisor.lock() {
        supervisor.stop();
    }

    let binary = gpu_build::install(&app, &dirs, target).await?;

    if let Ok(mut cache) = state.capabilities.lock() {
        cache.invalidate(&binary);
        cache.clear();
    }

    Ok(binary.to_string_lossy().into_owned())
}

/// Removes an installed GPU backend.
#[tauri::command]
pub fn remove_backend(
    state: tauri::State<'_, AppState>,
    backend: String,
) -> AppResult<()> {
    let target = GpuTarget::parse(&backend).ok_or_else(|| {
        AppError::msg("Unknown backend")
    })?;

    if let Ok(mut supervisor) = state.supervisor.lock() {
        supervisor.stop();
    }

    gpu_build::remove(&state.dirs, target)?;

    if let Ok(mut cache) = state.capabilities.lock() {
        cache.clear();
    }

    Ok(())
}

/// Cancels an in-progress backend download/install.
#[tauri::command]
pub fn cancel_install() -> AppResult<()> {
    gpu_build::cancel_current_install();
    Ok(())
}
