//! llama-server lifecycle: start it once per session for the chosen model,
//! reuse it while it is healthy, and shut it down when the app exits.
//!
//! Every flag comes from an [`InferenceProfile`], assembled by
//! [`args::build_launch`], so CPU-only runs never touch the GPU and GPU runs
//! only happen when the user opted in with their own build.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use crate::error::{AppError, AppResult};
use crate::runtime::{args, guard, port, profile::InferenceProfile, token, Serving};

pub struct Supervisor {
    running: Option<Running>,
    last_error: Option<String>,
    device_verified: Option<(PathBuf, String)>,
    fallback_notice: Option<String>,
}

struct Running {
    child: Child,
    serving: Serving,
    model: PathBuf,
    fingerprint: LaunchFingerprint,
}

/// Two launches with the same model and profile can share a server; anything
/// else requires a restart so the profile change actually takes effect.
#[derive(Debug, PartialEq, Eq)]
struct LaunchFingerprint {
    model: PathBuf,
    profile: InferenceProfile,
}

/// What the supervisor needs to start a server.
pub struct LaunchRequest<'a> {
    pub model: &'a Path,
    /// A multimodal projector that belongs to the model, when it has one.
    pub projector: Option<&'a Path>,
    pub server: &'a Path,
    /// The profile with its device already resolved — `Auto` never reaches
    /// the argument builder.
    pub profile: &'a InferenceProfile,
}

impl Supervisor {
    pub fn new() -> Self {
        Self {
            running: None,
            last_error: None,
            device_verified: None,
            fallback_notice: None,
        }
    }

    /// The port and token of a live server, if there is one.
    pub fn active(&self) -> Option<Serving> {
        self.running.as_ref().map(|run| run.serving.clone())
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error.clone()
    }

    pub fn fallback_notice(&self) -> Option<String> {
        self.fallback_notice.clone()
    }

    pub fn set_fallback_notice(&mut self, notice: Option<String>) {
        self.fallback_notice = notice;
    }

    pub fn running_model(&self) -> Option<PathBuf> {
        self.running.as_ref().map(|run| run.model.clone())
    }

    /// The profile the running server was started with, so the settings page
    /// can tell whether a change needs a restart.
    pub fn running_profile(&self) -> Option<&InferenceProfile> {
        self.running.as_ref().map(|run| &run.fingerprint.profile)
    }

    /// Starts a server for this model when there is not one already — or when
    /// the profile changed, in which case the old one is restarted.
    pub fn ensure(&mut self, request: &LaunchRequest) -> AppResult<Serving> {
        let fingerprint = LaunchFingerprint {
            model: request.model.to_path_buf(),
            profile: request.profile.clone(),
        };

        if let Some(run) = self.running.as_mut() {
            let exited = matches!(run.child.try_wait(), Ok(Some(_)));
            if !exited && run.fingerprint == fingerprint {
                return Ok(run.serving.clone());
            }
            self.stop();
        }

        let mut profile = request.profile.clone();
        if profile.threads == 0 {
            profile.threads = default_threads() as u32;
        }
        let profile = profile;

        // A CPU-only run must never touch a GPU device, even if the binary
        // happens to be a GPU build.
        let device = profile.device;
        self.verify_device(request.server, &device)?;

        let local_port = port::pick_free_port()?;
        let token = token::generate();

        let launch = args::build_launch(
            request.model,
            request.projector,
            local_port,
            &token,
            &profile,
        )
        .map_err(AppError::msg)?;

        let mut command = Command::new(request.server);
        command
            .args(&launch.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for (key, value) in &launch.env {
            command.env(key, value);
        }

        // On Linux, set LD_LIBRARY_PATH to the directory of the server binary so
        // backend shared libraries (.so) can be located dynamically.
        #[cfg(target_os = "linux")]
        {
            if let Some(parent) = request.server.parent() {
                command.env("LD_LIBRARY_PATH", parent);
            }
        }

        let child = command.spawn().map_err(|error| {
            AppError::msg(format!(
                "could not start {}: {error}",
                request.server.display()
            ))
        })?;

        let serving = Serving {
            port: local_port,
            token,
        };

        self.running = Some(Running {
            child,
            serving: serving.clone(),
            model: request.model.to_path_buf(),
            fingerprint: LaunchFingerprint {
                model: request.model.to_path_buf(),
                profile: profile.clone(),
            },
        });
        self.last_error = None;

        Ok(serving)
    }

    /// Runs `--list-devices` once per binary and caches the verdict, so a
    /// mislabeled GPU build is caught on the first launch and never again
    /// accidentally trusted even after a restart.
    fn verify_device(&mut self, server: &Path, device: &crate::runtime::profile::ComputeDevice)
        -> AppResult<()>
    {
        // Probe once per binary; the verdict is cached so a mislabeled GPU
        // build is caught on the first launch and never silently trusted
        // again after that.
        {
            let cache_key = format!("{device:?}");
            if self
                .device_verified
                .as_ref()
                .is_some_and(|(seen, seen_device)| seen == server && *seen_device == cache_key)
            {
                return Ok(());
            }
        }

        if let Err(message) = guard::verify_device_mode(server, device) {
            self.mark_failed(message.clone());
            return Err(AppError::msg(message));
        }

        self.device_verified = Some((server.to_path_buf(), format!("{device:?}")));
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(mut run) = self.running.take() {
            let _ = run.child.kill();
            let _ = run.child.wait();
        }
    }

    /// Stops the server after a failure and remembers why, so the status bar
    /// can show something better than "stopped".
    pub fn mark_failed(&mut self, message: String) {
        self.stop();
        self.last_error = Some(message);
    }
}

impl Default for Supervisor {
    fn default() -> Self {
        Self::new()
    }
}

/// Leaves a core or two for the rest of the machine.
fn default_threads() -> usize {
    let cores = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(2);
    cores.saturating_sub(1).max(1)
}
