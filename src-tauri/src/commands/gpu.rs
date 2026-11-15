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
    /// The backend the user opted for, null when the detection decides.
    pub selected: Option<String>,
    pub builds: Vec<gpu_build::GpuBuildStatus>,
    pub recommendation: GpuRecommendation,
}

/// Detects the machine's GPU and returns full recommendation details.
#[tauri::command]
pub async fn detect_gpu() -> GpuRecommendation {
    gpu::recommend_gpu()
}

/// The state of the GPU builds the app knows about.
#[tauri::command]
pub async fn list_gpu_builds(state: tauri::State<'_, AppState>) -> AppResult<GpuBuildList> {
    list_backends(state).await
}

/// Lists all known backends (aliased command per spec).
#[tauri::command]
pub async fn list_backends(state: tauri::State<'_, AppState>) -> AppResult<GpuBuildList> {
    let rec = gpu::recommend_gpu();
    let builds = gpu_build::status(&state.dirs, &rec.vendor);
    // For backward compat: single recommended (first in vendor's list)
    let recommended = gpu_build::recommend_for_vendor(&rec.vendor)
        .first()
        .map(|t| t.as_str().to_string());
    let selected = state
        .registry()
        .ok()
        .and_then(|registry| registry.settings.gpu_backend.clone());
    Ok(GpuBuildList {
        recommended,
        selected,
        gpu_name: rec.device_name.clone(),
        gpu_backend: rec.backend.clone(),
        gpu_available: rec.backend != "none",
        builds,
        recommendation: rec,
    })
}

/// Sets the GPU backend the user opted for. `None` hands the choice back to
/// the hardware detection. Only an installed backend can be selected.
#[tauri::command]
pub async fn set_gpu_backend(
    state: tauri::State<'_, AppState>,
    backend: Option<String>,
) -> AppResult<GpuBuildList> {
    let target = match backend.as_deref() {
        Some(name) => Some(GpuTarget::parse(name).ok_or_else(|| {
            AppError::msg("that is not a llama.cpp GPU backend. Expecting vulkan, cuda or opencl.")
        })?),
        None => None,
    };

    if let Some(target) = target {
        if !gpu_build::backend_dll_installed(&state.dirs, target) {
            return Err(AppError::msg(format!(
                "{} is not installed yet. Install it first, then select it.",
                target.label()
            )));
        }
    }

    {
        let state_file = state.dirs.state_file.clone();
        let mut registry = state.registry()?;
        registry.settings.gpu_backend = target.map(|target| target.as_str().to_string());
        registry.save(&state_file)?;
    }

    // The backend is fixed when the server starts, so the next turn has to
    // launch a fresh one.
    if let Ok(mut supervisor) = state.supervisor.lock() {
        supervisor.stop();
    }
    if let Ok(mut cache) = state.capabilities.lock() {
        cache.clear();
    }

    list_backends(state).await
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
        cache.clear();
    }

    Ok(binary.to_string_lossy().into_owned())
}

/// Removes an installed GPU backend.
#[tauri::command]
pub async fn remove_backend(
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

    // A removed backend cannot stay selected.
    {
        let state_file = state.dirs.state_file.clone();
        let mut registry = state.registry()?;
        if registry
            .settings
            .gpu_backend
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case(target.as_str()))
        {
            registry.settings.gpu_backend = None;
            registry.save(&state_file)?;
        }
    }

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
