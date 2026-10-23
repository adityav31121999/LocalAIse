pub mod args;
pub mod backend;
pub mod budget;
pub mod client;
pub mod gpu_build;
pub mod guard;
pub mod port;
pub mod profile;
pub mod supervisor;
pub mod token;

use serde::Serialize;

use crate::runtime::profile::{ComputeDevice, InferenceProfile};

/// Where llama-server is listening for this session, plus the token that the
/// process requires on every request.
#[derive(Debug, Clone)]
pub struct Serving {
    pub port: u16,
    pub token: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeState {
    Starting,
    Ready,
    Stopped,
    Error,
}

/// The same shape the status bar renders.
///
/// The profile and the device describe the server that is *running*, not the
/// one that is configured — the settings page compares the two to tell the
/// user whether a restart is needed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub model: String,
    pub state: RuntimeState,
    pub port: Option<u16>,
    /// The profile the running server was started with.
    pub profile: Option<InferenceProfile>,
    /// The device the running server actually uses.
    pub device: Option<ComputeDevice>,
    /// Why the last launch failed, when it did.
    pub error: Option<String>,
    /// Fallback notice if GPU health check failed and reverted to CPU.
    pub fallback_notice: Option<String>,
}
