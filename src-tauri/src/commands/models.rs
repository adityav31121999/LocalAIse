//! Tauri commands for model and runtime management: model discovery over the
//! user's folders, selection, hardware report, runtime status and the
//! inference profile.
//!
//! The profile follows a deliberate order. The user edits a draft in the UI,
//! the core validates it against the model that is loaded and the machine it
//! will run on, and only then is it persisted. Applying a profile stops the
//! running server, so the next chat starts it again with the validated flags.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tauri::State;

use crate::config::app_config::Settings;
use crate::config::validation::{ValidationContext, ValidationReport};
use crate::error::{AppError, AppResult};
use crate::models::registry as models;
use crate::models::naming::is_reasoning_model;
use crate::models::shape::{read_shape, read_meta};
use crate::runtime::backend::{self, ServerCapabilities};
use crate::runtime::budget::{self, FitLevel, ModelShape};
use crate::runtime::profile::{ComputeDevice, InferenceProfile, KvCacheType};
use crate::runtime::{RuntimeState, RuntimeStatus};
use crate::AppState;

/// Outcome of a profile estimate against a model on this machine.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileEstimate {
    /// The draft the estimate was computed for.
    pub profile: InferenceProfile,
    /// The draft with its device resolved — `auto` never survives here.
    pub resolved_profile: InferenceProfile,
    /// The device the profile will actually run on.
    pub device: ComputeDevice,
    /// Whether the shape could be read; when false the estimates are absent.
    pub model_shape_known: bool,
    pub model_file_gb: f32,
    /// The context the estimate assumes: the profile's pick, or the model's.
    pub context_tokens: u32,
    /// Context length the model file declares, 0 when unknown. The context
    /// slider uses this as its maximum.
    pub max_context: u32,
    /// Estimated KV cache bytes for the current profile.
    pub kv_cache_gb: f32,
    /// What the same window costs under every cache type, keyed by the type's
    /// wire name. Empty when the shape is unknown. The control that picks the
    /// type shows all of them, so the choice is made with the numbers in view
    /// instead of by guesswork.
    pub kv_cache_gb_by_type: BTreeMap<String, f32>,
    /// Working memory llama.cpp needs on top of the weights and the cache.
    pub runtime_overhead_gb: f32,
    /// Estimated RAM for the whole run when the device is CPU.
    pub ram_needed_gb: f32,
    pub ram_total_gb: f32,
    /// RAM that is free right now.
    pub ram_available_gb: f32,
    /// RAM left for inference once the reserve for the OS is set aside.
    pub ram_budget_gb: f32,
    /// Estimated VRAM for the whole run when the device is a user GPU build.
    pub vram_needed_gb: f32,
    /// Total VRAM of the detected adapter, 0 when unknown.
    pub vram_total_gb: f32,
    /// VRAM left for inference once the reserve for the compositor is set aside.
    pub vram_budget_gb: f32,
    /// True when the adapter reported its memory size.
    pub vram_known: bool,
    /// How well the configuration fits into the memory this machine has.
    pub fit: FitLevel,
    /// Whether the model file declares mixture-of-experts tensors, which the
    /// expert-placement control should offer to keep in system RAM.
    pub is_moe: bool,
    /// Recommended profile for this model and machine, or null when the
    /// shape is unknown or nothing fits.
    pub recommended: Option<InferenceProfile>,
    /// The validation findings for this draft.
    pub report: ValidationReport,
}

/// What the app knows about the machine and the model a draft runs on.
struct Facts {
    shape: Option<ModelShape>,
    model_error: Option<String>,
    total_ram_bytes: u64,
    available_ram_bytes: u64,
    vram_bytes: Option<u64>,
    gpu_backend: Option<String>,
    capabilities: ServerCapabilities,
    has_user_server: bool,
}

// ---------------------------------------------------------------------------
// Model discovery
// ---------------------------------------------------------------------------

/// The GGUF models on this machine, grouped by the folder they live in.
///
/// A model and the multimodal projector beside it belong together, so they are
/// returned as one group rather than as loose files. Models the app stores a
/// configuration of its own for are marked, so the models list can say so.
///
/// Heavy commands in this file are `async` so Tauri runs them on its worker
/// pool instead of the main thread: a synchronous command freezes the whole
/// window while it works, and these ones walk folders and spawn processes.
/// Async commands that borrow `State` must return a `Result`, so the ones
/// that cannot fail say `Ok` — to the webview that reads the same as before.
#[tauri::command]
pub async fn list_available_models(
    state: State<'_, AppState>,
) -> AppResult<Vec<models::ModelGroup>> {
    let settings = state
        .registry()
        .map(|registry| registry.settings.clone())
        .unwrap_or_default();
    let groups = models::groups(&state.dirs, &settings.model_folders);

    Ok(groups)
}

/// The GGUF folders the user keeps models in.
#[tauri::command]
pub fn list_model_folders(state: State<'_, AppState>) -> Vec<String> {
    state
        .registry()
        .map(|registry| {
            registry
                .settings
                .model_folders
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// Registers a folder to scan for GGUF files. Saved into persistent settings
/// so it survives restarts.
#[tauri::command]
pub fn add_model_folder(state: State<'_, AppState>, folder: String) -> AppResult<()> {
    register_folder(&state, &folder)
}

/// Stops using a folder as a model source. Files inside it are left alone.
#[tauri::command]
pub fn remove_model_folder(state: State<'_, AppState>, folder: String) -> AppResult<()> {
    let state_file = state.dirs.state_file.clone();
    let mut registry = state.registry()?;
    registry
        .settings
        .model_folders
        .retain(|path| path.to_string_lossy() != folder);
    registry.save(&state_file)
}

/// Opens a folder picker; a picked folder is registered right away.
#[tauri::command]
pub async fn pick_model_folder(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> AppResult<Option<String>> {
    let picked = {
        use tauri_plugin_dialog::DialogExt;
        app.dialog()
            .file()
            .set_title("Choose the folder your GGUF models live in")
            .blocking_pick_folder()
    };

    let Some(picked) = picked else {
        return Ok(None);
    };

    let path = picked
        .into_path()
        .map_err(|error| AppError::msg(format!("that location cannot be read: {error}")))?;

    let folder = path.to_string_lossy().into_owned();
    register_folder(&state, &folder)?;
    Ok(Some(folder))
}

/// Shared impl for the picker and the typed path: validate, canonicalize,
/// dedupe and persist a model folder.
fn register_folder(state: &State<'_, AppState>, folder: &str) -> AppResult<()> {
    let path = std::path::PathBuf::from(folder);
    if !path.is_dir() {
        return Err(AppError::msg(format!("Not a folder: {folder}")));
    }

    let folder = crate::util::paths::plain_canonicalize(&path).unwrap_or(path);
    let state_file = state.dirs.state_file.clone();
    let mut registry = state.registry()?;
    if !registry.settings.model_folders.contains(&folder) {
        registry.settings.model_folders.push(folder);
        registry.save(&state_file)?;
    }
    Ok(())
}

/// Set the selected model path in persistent settings.
#[tauri::command]
pub async fn select_model(state: State<'_, AppState>, model_path: String) -> AppResult<()> {
    let state_file = state.dirs.state_file.clone();
    let mut registry = state.registry()?;
    registry.settings.selected_model = if model_path.is_empty() {
        None
    } else {
        Some(std::path::PathBuf::from(&model_path))
    };
    // A model change needs a fresh server, so the next chat restarts it.
    if let Ok(mut supervisor) = state.supervisor.lock() {
        supervisor.stop();
    }
    registry.save(&state_file)
}

/// Remove a model file from the models folder.
#[tauri::command]
pub async fn delete_model(state: State<'_, AppState>, model_path: String) -> AppResult<()> {
    let models_dir = state.dirs.models.clone();
    let path = std::path::PathBuf::from(&model_path);
    if !path.starts_with(&models_dir) {
        return Err(AppError::msg(
            "Models can only be removed from the app's own models folder.",
        ));
    }
    // Drop the server first so a running model does not keep the file open.
    if let Ok(mut supervisor) = state.supervisor.lock() {
        supervisor.stop();
    }
    crate::models::registry::delete_file(&path)
}

/// The path chat currently loads, if any model is installed.
#[tauri::command]
pub fn get_selected_model(state: State<'_, AppState>) -> Option<String> {
    let Ok(registry) = state.registry() else {
        return None;
    };
    crate::models::registry::selected(&state.dirs, &registry.settings)
        .map(|path| path.to_string_lossy().into_owned())
}

// ---------------------------------------------------------------------------
// Hardware, runtime status, profile
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_hardware_report() -> crate::hardware::HardwareReport {
    crate::hardware::probe_hardware()
}

/// The runtime as it actually is: what is loaded, where it runs and whether
/// it failed.
///
/// `model_path` is the path of the model the server is running, so the model
/// list can mark the loaded model. When nothing is running it reports the
/// selected (or first installed) model's label without a path.
fn runtime_status_for(state: &AppState, settings: &Settings) -> RuntimeStatus {
    let supervisor = match state.supervisor.lock() {
        Ok(supervisor) => supervisor,
        Err(_) => {
            return RuntimeStatus {
                model: models::selected_label(&state.dirs, settings),
                model_path: None,
                state: RuntimeState::Error,
                port: None,
                profile: None,
                device: None,
                error: Some("the local runtime is busy".to_string()),
                fallback_notice: None,
            };
        }
    };

    let (state_, port) = if supervisor.active().is_some() {
        (RuntimeState::Ready, supervisor.active().map(|s| s.port))
    } else if supervisor.last_error().is_some() {
        (RuntimeState::Error, None)
    } else {
        (RuntimeState::Stopped, None)
    };

    let profile = supervisor.running_profile().cloned();
    let model_path = supervisor
        .running_model()
        .map(|path| path.to_string_lossy().into_owned());
    let model = model_path
        .as_deref()
        .map(|path| models::label(Path::new(path)))
        .unwrap_or_else(|| models::selected_label(&state.dirs, settings));

    RuntimeStatus {
        model,
        model_path,
        state: state_,
        port,
        device: profile.as_ref().map(|profile| profile.device),
        profile,
        error: supervisor.last_error(),
        fallback_notice: supervisor.fallback_notice(),
    }
}

/// The runtime as it actually is: what is loaded, where it runs and whether
/// it failed.
#[tauri::command]
pub fn runtime_status(state: State<'_, AppState>) -> RuntimeStatus {
    let settings = state
        .registry()
        .map(|registry| registry.settings.clone())
        .unwrap_or_default();
    runtime_status_for(&state, &settings)
}

/// Selects a model and starts the runtime server for it right away, so it is
/// ready for chat instead of waiting for the first turn. Returns the runtime
/// status after loading.
#[tauri::command]
pub async fn load_model(state: State<'_, AppState>, model_path: String) -> AppResult<RuntimeStatus> {
    let path = PathBuf::from(&model_path);
    if !path.is_file() {
        return Err(AppError::msg(format!("Model file not found: {model_path}")));
    }

    // Persist the selection so chat and the runtime resolve to this model.
    let state_file = state.dirs.state_file.clone();
    {
        let mut registry = state.registry()?;
        registry.settings.selected_model = Some(path.clone());
        registry.save(&state_file)?;
    }

    let settings = state
        .registry()
        .map(|registry| registry.settings.clone())
        .unwrap_or_default();
    let projector = models::projector_for(&path);

    // Start (or restart) the server for this model immediately.
    let _serving = crate::commands::runtime::ensure(&state, &settings, projector.as_deref())
        .inspect_err(|error| {
            if let Ok(mut supervisor) = state.supervisor() {
                supervisor.mark_failed(error.to_string());
            }
        })?;

    Ok(runtime_status_for(&state, &settings))
}

/// Stops the running runtime server and clears the selected model, so nothing
/// is loaded for chat until a model is loaded again. Returns the new status.
#[tauri::command]
pub async fn unload_model(state: State<'_, AppState>) -> AppResult<RuntimeStatus> {
    let settings = state
        .registry()
        .map(|registry| registry.settings.clone())
        .unwrap_or_default();

    if let Ok(mut supervisor) = state.supervisor() {
        supervisor.stop();
    }

    let state_file = state.dirs.state_file.clone();
    let mut registry = state.registry()?;
    registry.settings.selected_model = None;
    registry.save(&state_file)?;

    Ok(runtime_status_for(&state, &settings))
}

/// The profiles the app has stored, and the one that is in effect.
///
/// The live inference profile the runtime starts with.
///
/// `threads: 0` still means "auto" — the UI shows the resolved count beside
/// the switch rather than baking it into the stored profile.
#[tauri::command]
pub fn get_runtime_profile(state: State<'_, AppState>) -> InferenceProfile {
    let settings = state
        .registry()
        .map(|registry| registry.settings.clone())
        .unwrap_or_default();
    settings.performance
}

/// What the llama-server build in use can do.
///
/// The answer comes from the build itself, not from the driver files: a
/// build that cannot see a graphics card says so.
#[tauri::command]
pub async fn get_runtime_capabilities(
    state: State<'_, AppState>,
) -> AppResult<ServerCapabilities> {
    let settings = state
        .registry()
        .map(|registry| registry.settings.clone())
        .unwrap_or_default();
    Ok(capabilities_for(&state, &settings))
}

/// The capabilities of the build that would run for these settings.
fn capabilities_for(state: &AppState, settings: &Settings) -> ServerCapabilities {
    let mut cache = match state.capabilities.lock() {
        Ok(cache) => cache,
        Err(poisoned) => poisoned.into_inner(),
    };
    backend::capabilities_for(&state.dirs, settings, &mut cache)
}

/// Checks a draft profile against the model and the machine.
///
/// This never changes anything: the UI calls it while the user edits, and the
/// findings are shown next to the controls that caused them.
#[tauri::command]
pub async fn validate_runtime_profile(
    state: State<'_, AppState>,
    profile: InferenceProfile,
    model_path: Option<String>,
) -> AppResult<ValidationReport> {
    let settings = state.registry()?.settings.clone();
    let facts = gather(&state, &settings, model_path.as_deref());
    Ok(validate_draft(&state, &profile, &facts))
}

/// Validates a draft and, when nothing is wrong with it, persists it as the
/// live inference profile.
///
/// A profile with errors is refused: the running server is left alone and the
/// findings come back so the UI can show what to fix.
///
/// The server is only stopped when the live profile changes, so the new flags
/// take effect on the next chat turn.
#[tauri::command]
pub async fn apply_runtime_profile(
    state: State<'_, AppState>,
    profile: InferenceProfile,
    model_path: Option<String>,
) -> AppResult<ValidationReport> {
    let mut settings = state.registry()?.settings.clone();
    let facts = gather(&state, &settings, model_path.as_deref());

    let mut report = validate_draft(&state, &profile, &facts);
    if report.has_errors() {
        let detail = report
            .messages
            .iter()
            .filter(|message| message.severity == crate::config::validation::Severity::Error)
            .map(|message| format!("{}: {}", message.title, message.detail))
            .collect::<Vec<_>>()
            .join(" ");
        return Err(AppError::msg(format!(
            "This configuration cannot run yet. {detail}"
        )));
    }

    let before = settings.performance.clone();
    settings.performance = profile;

    let after = settings.performance.clone();
    let restarted = resolved(&before, &facts) != resolved(&after, &facts);
    report.requires_restart = restarted;

    let state_file = state.dirs.state_file.clone();
    {
        let mut registry = state.registry()?;
        registry.settings = settings;
        registry.save(&state_file)?;
    }

    // The flags only take effect on a fresh server, so a running one has to
    // go when the profile in effect changed. The next chat turn starts it
    // again with the new profile.
    if restarted {
        if let Ok(mut supervisor) = state.supervisor.lock() {
            supervisor.stop();
        }
    }

    Ok(report)
}

/// Validates a draft against everything the app knows.
fn validate_draft(
    state: &AppState,
    profile: &InferenceProfile,
    facts: &Facts,
) -> ValidationReport {
    let running = state
        .supervisor
        .lock()
        .ok()
        .and_then(|supervisor| supervisor.running_profile().cloned());

    let device = backend::resolve_device(profile, &facts.capabilities, facts.shape.as_ref());

    // The live profile is what runs; the device is settled and an automatic
    // thread count written out, so the running server is compared against
    // this resolved profile.
    let effective = resolved(profile, facts);

    let context = ValidationContext {
        profile,
        resolved: Some(&effective),
        running: running.as_ref(),
        shape: facts.shape.as_ref(),
        model_error: facts.model_error.as_deref(),
        total_ram_bytes: facts.total_ram_bytes,
        vram_bytes: facts.vram_bytes,
        gpu_backend: facts.gpu_backend.as_deref(),
        server_supports_gpu: facts.capabilities.supports_gpu,
        server_supports_flash_attn: facts.capabilities.flash_attn,
        has_user_server: facts.has_user_server,
        resolved_device: Some(device),
    };

    crate::config::validation::validate(&context)
}

/// A profile with its device and thread count settled, so two profiles that
/// mean the same thing compare equal.
fn resolved(profile: &InferenceProfile, facts: &Facts) -> InferenceProfile {
    let device = backend::resolve_device(profile, &facts.capabilities, facts.shape.as_ref());
    let cores = crate::hardware::cpu::detect().logical_cores as usize;
    profile.resolved_for(device, cores)
}

/// The model a configuration is about: the one asked for, else the selected
/// one, else the first one installed.
fn model_for(state: &AppState, settings: &Settings, requested: Option<&str>) -> Option<PathBuf> {
    requested
        .map(Path::new)
        .filter(|path| path.is_file())
        .map(Path::to_path_buf)
        .or_else(|| models::selected(&state.dirs, settings))
}

/// Reads everything the estimate and the validation need: the model on disk,
/// the memory of the machine and the capabilities of the build.
fn gather(state: &AppState, settings: &Settings, requested: Option<&str>) -> Facts {
    let resolved = model_for(state, settings, requested);

    let (shape, model_error) = match &resolved {
        Some(path) => {
            let result = read_shape(path);
            (Some(result.shape), result.warning)
        }
        None => (None, None),
    };

    let memory = crate::hardware::memory::read();
    let gpu = crate::hardware::gpu::detect();
    let capabilities = capabilities_for(state, settings);

    // The build's own device list is the better source for VRAM; the
    // hardware probe is the fallback when the build does not say.
    let vram_bytes = capabilities.best_vram_bytes().or_else(|| {
        gpu.vram_gb.map(|gigabytes| (gigabytes * 1_073_741_824.0) as u64)
    });

    Facts {
        shape,
        model_error,
        total_ram_bytes: memory.total_bytes,
        available_ram_bytes: memory.available_bytes,
        vram_bytes,
        gpu_backend: capabilities
            .backend_name()
            .map(str::to_string)
            .or_else(|| {
                let backend = gpu.backend.as_str();
                (backend != "CPU only").then(|| backend.to_string())
            }),
        capabilities,
        has_user_server: settings
            .llama_server_path
            .as_ref()
            .is_some_and(|path| path.is_file()),
    }
}

/// Estimate memory for a profile against a model on disk, and suggest a
/// profile that fits when the machine cannot honor the current one.
///
/// The profile is sent by the UI so the estimate follows the on-screen draft
/// live; without it, the live profile is used.
#[tauri::command]
pub async fn get_profile_estimate(
    state: State<'_, AppState>,
    profile: Option<InferenceProfile>,
    model_path: Option<String>,
) -> AppResult<ProfileEstimate> {
    let settings = state.registry()?.settings.clone();
    let draft = profile.unwrap_or_else(|| settings.performance.clone());
    let facts = gather(&state, &settings, model_path.as_deref());
    let report = validate_draft(&state, &draft, &facts);
    let mut estimate = estimate_for(&draft, &facts);
    estimate.report = report;
    Ok(estimate)
}

/// Builds the estimate for a draft.
fn estimate_for(draft: &InferenceProfile, facts: &Facts) -> ProfileEstimate {
    let shape = facts.shape.unwrap_or_default();
    let known = budget::is_known(&shape);

    let device = backend::resolve_device(draft, &facts.capabilities, facts.shape.as_ref());
    let mut resolved = draft.clone();
    resolved.device = device;

    let context_tokens = budget::effective_context(&resolved, &shape);
    let kv = if known {
        budget::kv_cache_bytes(&shape, context_tokens, resolved.kv_type)
    } else {
        0
    };
    // The same window priced under every cache type, so the picker can show
    // what each option costs — the ratios follow bytes_per_element, but the
    // absolute GB are what the user is choosing between.
    let kv_by_type = if known {
        KvCacheType::OPTIONS
            .iter()
            .map(|kv| {
                (
                    kv.as_arg().to_string(),
                    budget::kv_cache_bytes(&shape, context_tokens, *kv) as f32
                        / 1_073_741_824.0,
                )
            })
            .collect()
    } else {
        BTreeMap::new()
    };
    let split = budget::split(&shape, &resolved);

    let ram_budget = facts
        .total_ram_bytes
        .saturating_sub(budget::RAM_RESERVE_BYTES);
    let vram_total = facts.vram_bytes.unwrap_or(0);
    let vram_budget = vram_total.saturating_sub(budget::VRAM_RESERVE_BYTES);

    let on_gpu = device.is_gpu();
    let fit = if on_gpu {
        // The VRAM fit is the one that matters; the RAM fit only covers the
        // expert tensors parked in system RAM.
        let vram_fit = if vram_total > 0 {
            budget::fit_level(
                split.vram_bytes + budget::RUNTIME_OVERHEAD_BYTES,
                vram_budget,
            )
        } else {
            FitLevel::Comfortable
        };
        let ram_fit = budget::fit_level(split.ram_bytes, ram_budget);
        worse(vram_fit, ram_fit)
    } else {
        budget::fit_level(
            split.ram_bytes + budget::RUNTIME_OVERHEAD_BYTES,
            ram_budget,
        )
    };

    let host = budget::HostCaps {
        total_ram_bytes: facts.total_ram_bytes,
        logical_cores: crate::hardware::cpu::detect().logical_cores,
    };
    let gpu_caps = facts
        .vram_bytes
        .filter(|_| facts.capabilities.supports_gpu)
        .map(|vram_bytes| budget::GpuCaps { vram_bytes });
    let wanted = draft.context_tokens.max(8_192);
    let recommended = if known {
        // The recommendation keeps the device automatic: the app resolves it
        // against the build in use, so a machine without a GPU build does not
        // end up with a profile that asks for one.
        budget::recommend(&shape, &host, gpu_caps, wanted).map(|mut profile| {
            profile.device = ComputeDevice::Auto;
            profile
        })
    } else {
        None
    };

    ProfileEstimate {
        profile: draft.clone(),
        resolved_profile: resolved,
        device,
        model_shape_known: known,
        model_file_gb: shape.file_bytes as f32 / 1_073_741_824.0,
        context_tokens,
        max_context: shape.max_context,
        kv_cache_gb: kv as f32 / 1_073_741_824.0,
        kv_cache_gb_by_type: kv_by_type,
        runtime_overhead_gb: budget::RUNTIME_OVERHEAD_BYTES as f32 / 1_073_741_824.0,
        ram_needed_gb: split.ram_bytes as f32 / 1_073_741_824.0,
        ram_total_gb: facts.total_ram_bytes as f32 / 1_073_741_824.0,
        ram_available_gb: facts.available_ram_bytes as f32 / 1_073_741_824.0,
        ram_budget_gb: ram_budget as f32 / 1_073_741_824.0,
        vram_needed_gb: split.vram_bytes as f32 / 1_073_741_824.0,
        vram_total_gb: vram_total as f32 / 1_073_741_824.0,
        vram_budget_gb: vram_budget as f32 / 1_073_741_824.0,
        vram_known: vram_total > 0,
        fit,
        is_moe: shape.moe.is_some(),
        recommended,
        report: ValidationReport::default(),
    }
}

/// The worse of two fit levels.
fn worse(a: FitLevel, b: FitLevel) -> FitLevel {
    match (a, b) {
        (FitLevel::Exceeds, _) | (_, FitLevel::Exceeds) => FitLevel::Exceeds,
        (FitLevel::Tight, _) | (_, FitLevel::Tight) => FitLevel::Tight,
        _ => FitLevel::Comfortable,
    }
}

/// The llama-server build the runtime resolves to.
///
/// The user's explicit pick when there is one, otherwise the next build on
/// disk — an installed GPU build first, the bundled CPU build afterwards.
/// The settings page shows this as the build in use.
#[tauri::command]
pub async fn get_server_binary(state: State<'_, AppState>) -> AppResult<Option<String>> {
    let settings = state
        .registry()
        .ok()
        .map(|registry| registry.settings.clone())
        .unwrap_or_default();
    Ok(
        backend::resolve_server(&state.dirs, &settings)
            .map(|path| path.to_string_lossy().into_owned()),
    )
}

/// Check if the selected model (or a given model path) is a reasoning/thinking model.
/// Returns true for models that natively produce reasoning traces (Qwen3, DeepSeek-R1, QwQ, Nemotron, etc.).
#[tauri::command]
pub fn is_model_reasoning(state: State<'_, AppState>, model_path: Option<String>) -> AppResult<bool> {
    let settings = state.registry()?.settings.clone();
    let model = model_path
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| models::selected(&state.dirs, &settings));

    let Some(path) = model else {
        return Ok(false);
    };

    let meta = read_meta(&path);
    Ok(is_reasoning_model(&meta.name, &meta.architecture))
}

/// Lets the user pick their own llama-server build. Needed together with GPU
/// mode; leaving the slot empty keeps the bundled CPU-only build.
#[tauri::command]
pub async fn pick_server_binary(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> AppResult<Option<String>> {
    let picked = {
        use tauri_plugin_dialog::DialogExt;
        app.dialog()
            .file()
            .set_title("Choose the llama-server build to use")
            .blocking_pick_file()
    };

    let Some(picked) = picked else {
        return Ok(None);
    };

    let path = picked
        .into_path()
        .map_err(|error| AppError::msg(format!("that location cannot be read: {error}")))?;

    let acceptable_name = path.file_name().is_some_and(|name| {
        let lowered = name.to_string_lossy().to_lowercase();
        lowered == "llama-server.exe" || lowered == "llama-server"
    });
    if !acceptable_name {
        return Err(AppError::msg(
            "That is not a llama-server build. Pick the executable named llama-server.",
        ));
    }

    {
        let state_file = state.dirs.state_file.clone();
        let mut registry = state.registry()?;
        registry.settings.llama_server_path = Some(path.clone());
        registry.save(&state_file)?;
    }
    if let Ok(mut supervisor) = state.supervisor.lock() {
        supervisor.stop();
    }

    // The new build has to be asked what it can do.
    if let Ok(mut cache) = state.capabilities.lock() {
        cache.invalidate(&path);
    }

    Ok(Some(path.to_string_lossy().into_owned()))
}
