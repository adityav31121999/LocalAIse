//! Starting the local runtime for the commands that talk to a model.
//!
//! Chat and research both need the same thing: the model the app loads, the
//! llama-server build that will serve it, the profile with its device
//! resolved, and — when images are attached — the multimodal projector.
//! The projector is not loaded at server startup; it is passed in by the
//! caller only when a turn needs it. The work lives here so the two
//! commands cannot drift apart.

use std::path::{Path, PathBuf};

use crate::config::app_config::Settings;
use crate::error::{AppError, AppResult};
use crate::models::registry as models;
use crate::models::shape::read_shape;
use crate::runtime::backend;
use crate::runtime::profile::InferenceProfile;
use crate::runtime::supervisor::LaunchRequest;
use crate::runtime::Serving;
use crate::AppState;

/// Everything the supervisor needs to start the server.
pub struct RuntimeSetup {
    /// The GGUF the runtime loads.
    pub model: PathBuf,
    /// The llama-server build that will be started.
    pub server: PathBuf,
    /// The saved profile with its device already resolved — `Auto` never
    /// reaches the argument builder.
    pub profile: InferenceProfile,
}

/// Resolves what the runtime will run for the saved profile.
///
/// The device is resolved here rather than in the supervisor: the app has to
/// know whether the run lands on the GPU or the CPU before the server starts,
/// and the settings page has to show the same answer.
///
/// The profile is the one in effect for this model in this project — a
/// project's own settings win over the model's, and both win over the
/// app-wide default.
///
/// The multimodal projector is deliberately not resolved here: it is loaded
/// only when a chat turn actually attaches an image.
pub fn resolve(
    state: &AppState,
    settings: &Settings,
    project_id: Option<&str>,
) -> AppResult<RuntimeSetup> {
    let model = models::selected(&state.dirs, settings).ok_or(AppError::NoModelSelected)?;
    let profile = settings.profile_for(Some(&model), project_id);

    // The model shape decides how many layers fit on the GPU, so it is only
    // read when the device is not already pinned to the CPU.
    let shape = if profile.device.is_cpu() {
        None
    } else {
        Some(read_shape(&model).shape)
    };

    // One lock covers both questions — what the build can do and what will
    // run — so the probe and the plan cannot disagree about the binary.
    let plan = {
        let mut cache = state
            .capabilities
            .lock()
            .map_err(|_| AppError::msg("the local runtime is busy; try again"))?;
        backend::plan(&state.dirs, settings, profile, shape.as_ref(), &mut cache)
    }
    .map_err(AppError::msg)?;

    Ok(RuntimeSetup {
        model,
        server: plan.server,
        profile: plan.profile,
    })
}

/// Resolves the runtime and starts the server, reusing a healthy one.
///
/// `projector` is passed through to the supervisor: when it is `Some`, the
/// server is started (or restarted) with `--mmproj`; when `None`, the server
/// runs without a multimodal projector. The caller decides based on whether
/// the current turn attaches an image.
pub fn ensure(
    state: &AppState,
    settings: &Settings,
    project_id: Option<&str>,
    projector: Option<&Path>,
) -> AppResult<Serving> {
    let setup = resolve(state, settings, project_id)?;

    let mut supervisor = state.supervisor()?;
    match supervisor.ensure(&LaunchRequest {
        model: setup.model.as_path(),
        projector,
        server: setup.server.as_path(),
        profile: &setup.profile,
    }) {
        Ok(serving) => Ok(serving),
        Err(error) => {
            supervisor.mark_failed(error.to_string());
            Err(error)
        }
    }
}
