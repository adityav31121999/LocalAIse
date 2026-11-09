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

use serde::{Deserialize, Serialize};

use crate::runtime::profile::{ComputeDevice, InferenceProfile};

/// Where llama-server is listening for this session, plus the token that the
/// process requires on every request.
#[derive(Debug, Clone)]
pub struct Serving {
    pub port: u16,
    pub token: String,
}

/// Token usage of a single model call inside a turn. An agent turn is a
/// sequence of calls (one per tool-loop iteration); this records what each
/// iteration cost, so a saved chat shows where the tokens went.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallUsage {
    /// Position of the call in the turn, matching [`crate::projects::chats::AgentStep::call`].
    pub index: u32,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    /// Generated tokens per second for this one call.
    pub tokens_per_second: f64,
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
    /// The path of the model the running server was started with, when there
    /// is one. The UI uses it to mark the loaded model in the model list.
    pub model_path: Option<String>,
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
