//! Cross-setting validation for the inference profile.
//!
//! The profile the user edits in React is checked here against the model it
//! will run and the machine it will run on. Errors stop the configuration
//! from being applied; warnings are shown next to the control that caused
//! them, so nothing is rejected behind the user's back.

use serde::Serialize;

use crate::runtime::budget::{self, FitLevel, ModelShape};
use crate::runtime::profile::{ComputeDevice, InferenceProfile, KvCacheType, MoeOffload};

/// How serious a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The configuration cannot be applied.
    Error,
    /// The configuration works, but something about it is worth knowing.
    Warning,
    /// A neutral remark that explains a choice.
    Info,
}

/// One finding about the configuration.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationMessage {
    /// Stable identifier, so the UI can react to a specific finding.
    pub code: String,
    pub severity: Severity,
    /// Short headline, e.g. "May exceed the available memory".
    pub title: String,
    /// What it means and what to do about it.
    pub detail: String,
}

/// Everything the validation found.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    pub messages: Vec<ValidationMessage>,
    /// True when the running server has to be restarted for the change to
    /// take effect.
    pub requires_restart: bool,
}

impl ValidationReport {
    pub fn has_errors(&self) -> bool {
        self.messages
            .iter()
            .any(|message| message.severity == Severity::Error)
    }

    /// The messages of one severity.
    pub fn with_severity(&self, severity: Severity) -> Vec<&ValidationMessage> {
        self.messages
            .iter()
            .filter(|message| message.severity == severity)
            .collect()
    }
}

/// What the validation knows about the machine and the model.
pub struct ValidationContext<'a> {
    /// The profile the user wants to run.
    pub profile: &'a InferenceProfile,
    /// The same profile with its device and thread count resolved — what the
    /// server would actually run.
    ///
    /// The running server's profile is compared with this one, so a profile
    /// that only differs in how it spells "automatic" does not look like a
    /// change that needs a restart.
    pub resolved: Option<&'a InferenceProfile>,
    /// The profile the running server was started with.
    pub running: Option<&'a InferenceProfile>,
    /// The shape of the model the profile runs against.
    pub shape: Option<&'a ModelShape>,
    /// A problem with reading the model file, when there is one.
    pub model_error: Option<&'a str>,
    /// Total RAM of this machine.
    pub total_ram_bytes: u64,
    /// VRAM of the detected adapter, when there is one.
    pub vram_bytes: Option<u64>,
    /// The GPU backend the runtime would use.
    pub gpu_backend: Option<&'a str>,
    /// Whether the llama-server build in use can offload to a GPU.
    pub server_supports_gpu: bool,
    /// Whether the build exposes the flash-attention flag. `None` when the
    /// build could not be asked.
    pub server_supports_flash_attn: Option<bool>,
    /// Whether the user pointed the app at their own llama-server build.
    pub has_user_server: bool,
    /// The device the profile resolves to on this machine. The budget checks
    /// use it, because a GPU run is measured against VRAM and a CPU run
    /// against system RAM.
    pub resolved_device: Option<ComputeDevice>,
}

impl<'a> ValidationContext<'a> {
    /// A context with nothing known about the machine yet.
    pub fn new(profile: &'a InferenceProfile) -> Self {
        Self {
            profile,
            resolved: None,
            running: None,
            shape: None,
            model_error: None,
            total_ram_bytes: 0,
            vram_bytes: None,
            gpu_backend: None,
            server_supports_gpu: false,
            server_supports_flash_attn: None,
            has_user_server: false,
            resolved_device: None,
        }
    }
}

/// Checks a profile against the model and the machine.
pub fn validate(context: &ValidationContext<'_>) -> ValidationReport {
    let mut messages: Vec<ValidationMessage> = Vec::new();

    // Structural problems: these hold on every machine.
    if let Err(problem) = context.profile.validate() {
        messages.push(error(
            "invalid-profile",
            "This configuration cannot run",
            problem,
        ));
    }

    if let Some(problem) = context.model_error {
        messages.push(error(
            "model-unreadable",
            "The model file cannot be read",
            problem.to_string(),
        ));
    }

    check_device(context, &mut messages);
    check_context(context, &mut messages);
    check_cache(context, &mut messages);
    check_thinking(context, &mut messages);
    check_experts(context, &mut messages);

    ValidationReport {
        messages,
        requires_restart: match (context.running, context.resolved) {
            // The running server is compared with the profile as it would be
            // started, so an automatic device or an automatic thread count
            // does not look like a change.
            (Some(running), Some(resolved)) => running != resolved,
            (Some(_), None) => true,
            _ => false,
        },
    }
}

fn check_device(context: &ValidationContext<'_>, messages: &mut Vec<ValidationMessage>) {
    match context.profile.device {
        ComputeDevice::Auto => {
            if context.server_supports_gpu && context.vram_bytes.is_some() {
                let backend = context.gpu_backend.unwrap_or("GPU");
                messages.push(info(
                    "auto-gpu",
                    "Automatic device: GPU",
                    format!(
                        "Layers are offloaded to the {backend}. The app keeps as many of them in \
                         VRAM as fit, and leaves the rest on the CPU."
                    ),
                ));
            } else if !context.server_supports_gpu {
                messages.push(info(
                    "auto-cpu-build",
                    "Automatic device: CPU",
                    "The llama-server build in use has no GPU support. Point the app at a GPU \
                     build of llama.cpp to use the graphics card.",
                ));
            } else {
                messages.push(info(
                    "auto-cpu",
                    "Automatic device: CPU",
                    "No usable graphics card was found, so the model runs on the CPU.",
                ));
            }
        }
        ComputeDevice::Cpu => {
            messages.push(info(
                "cpu-only",
                "CPU only",
                "No VRAM is used: the weights and the cache stay in system RAM.",
            ));
        }
        ComputeDevice::UserGpu { gpu_layers } => {
            if !context.has_user_server && !context.server_supports_gpu {
                messages.push(error(
                    "gpu-no-build",
                    "GPU mode needs a llama-server build with GPU support",
                    "The build in use reports no GPU device. Install a Vulkan or CUDA build in \
                     the hardware settings, point the app at your own build, or set the device \
                     back to CPU.",
                ));
            }
            if context.vram_bytes.is_none() {
                messages.push(warning(
                    "gpu-no-hardware",
                    "No graphics card was detected",
                    "This machine does not report a usable GPU, so the run falls back to the CPU.",
                ));
            }
            if gpu_layers == 0 {
                messages.push(warning(
                    "gpu-no-layers",
                    "No layers are offloaded",
                    "With 0 layers on the GPU this is a CPU run. Offload at least one layer to \
                     use the card.",
                ));
            }
        }
    }
}

fn check_context(context: &ValidationContext<'_>, messages: &mut Vec<ValidationMessage>) {
    let profile = context.profile;
    let Some(shape) = context.shape else {
        return;
    };
    if !budget::is_known(shape) {
        return;
    }

    let declared = shape.max_context;
    if profile.context_tokens == 0 {
        messages.push(info(
            "context-model-default",
            "The model's own context length is used",
            match declared {
                0 => "The file does not declare a context length, so llama-server picks one."
                    .to_string(),
                tokens => format!("The file declares {tokens} tokens."),
            },
        ));
    } else if declared > 0 && profile.context_tokens > declared {
        messages.push(warning(
            "context-above-model",
            "Larger than the model declares",
            format!(
                "The file declares {declared} tokens. A longer window does not give the model \
                 more room to look at — the runtime cuts it back to what the model supports."
            ),
        ));
    }

    // The budget is measured against the device the run will actually use:
    // a GPU run lives in VRAM, a CPU run in system RAM.
    let device = context.resolved_device.unwrap_or(profile.device);
    let mut effective = profile.clone();
    effective.device = device;
    let split = budget::split(shape, &effective);

    if device.is_gpu() {
        let Some(vram) = context.vram_bytes else {
            messages.push(info(
                "vram-unknown",
                "The size of the graphics memory is unknown",
                "The adapter did not report how much VRAM it has, so the app cannot say whether \
                 this configuration fits. Lower the context window to stay on the safe side.",
            ));
            return;
        };
        let budget = vram.saturating_sub(budget::VRAM_RESERVE_BYTES);
        let needed = split.vram_bytes + budget::RUNTIME_OVERHEAD_BYTES;
        match budget::fit_level(needed, budget) {
            FitLevel::Exceeds => messages.push(warning(
                "memory-exceeds",
                "May exceed available memory",
                format!(
                    "The weights, the cache and the runtime need about {:.1} GB of VRAM, but only \
                     {:.1} GB of the {:.1} GB of the graphics card are free for inference. Lower \
                     the context window, pick a smaller KV cache type (Q5 or Q4), keep the expert \
                     tensors in system RAM, or turn on partial loading so llama-server shrinks the \
                     context to fit.",
                    gigabytes(needed),
                    gigabytes(budget),
                    gigabytes(vram),
                ),
            )),
            FitLevel::Tight => messages.push(warning(
                "memory-tight",
                "Tight memory budget",
                format!(
                    "The weights, the cache and the runtime need about {:.1} GB of the {:.1} GB \
                     of VRAM that are free for inference. It fits, but there is little room for \
                     anything else.",
                    gigabytes(needed),
                    gigabytes(budget),
                ),
            )),
            FitLevel::Comfortable => {}
        }
        return;
    }

    let available = context
        .total_ram_bytes
        .saturating_sub(budget::RAM_RESERVE_BYTES);
    let needed = split.ram_bytes + budget::RUNTIME_OVERHEAD_BYTES;

    match budget::fit_level(needed, available) {
        FitLevel::Exceeds => messages.push(warning(
            "memory-exceeds",
            "May exceed available memory",
            format!(
                "The weights, the cache and the runtime need about {:.1} GB, but only {:.1} GB \
                 of the {:.1} GB of RAM are free for inference. Lower the context window, pick a \
                 smaller KV cache type (Q5 or Q4), or turn on partial loading so llama-server \
                 shrinks the context to fit.",
                gigabytes(needed),
                gigabytes(available),
                gigabytes(context.total_ram_bytes),
            ),
        )),
        FitLevel::Tight => messages.push(warning(
            "memory-tight",
            "Tight memory budget",
            format!(
                "The weights, the cache and the runtime need about {:.1} GB of the {:.1} GB \
                 that are free for inference. It fits, but there is little room for anything else.",
                gigabytes(needed),
                gigabytes(available),
            ),
        )),
        FitLevel::Comfortable => {}
    }
}

fn check_cache(context: &ValidationContext<'_>, messages: &mut Vec<ValidationMessage>) {
    let profile = context.profile;
    if profile.kv_type == KvCacheType::F16 && profile.flash_attn {
        messages.push(info(
            "flash-unused",
            "Flash attention without a quantized cache",
            "Flash attention pays off with a quantized cache. With F16 the cache stays full \
             precision, so the setting changes nothing but the memory layout.",
        ));
    }

    // Flash attention is not a given: the build has to know the flag, and the
    // quantized cache only works with it.
    match context.server_supports_flash_attn {
        Some(false) => messages.push(error(
            "flash-unsupported",
            "This build cannot use flash attention",
            "The llama-server build in use does not offer the flash-attention flag. Turn it off, \
             or point the app at a build that supports it.",
        )),
        Some(true) => {}
        None => {
            if profile.flash_attn {
                messages.push(info(
                    "flash-unverified",
                    "Flash attention could not be confirmed",
                    "The build did not answer when it was asked whether it supports flash \
                     attention. The setting is kept, but the app cannot promise it will take effect.",
                ));
            }
        }
    }
}

fn check_thinking(context: &ValidationContext<'_>, messages: &mut Vec<ValidationMessage>) {
    let profile = context.profile;
    if profile.thinking_budget == 0 {
        return;
    }

    let window = if profile.context_tokens > 0 {
        profile.context_tokens
    } else {
        context.shape.map(|shape| shape.max_context).unwrap_or(0)
    };

    if window > 0 && profile.thinking_budget >= window {
        messages.push(warning(
            "thinking-above-context",
            "The thinking budget is larger than the context",
            format!(
                "The budget of {} tokens does not fit in a window of {window} tokens, so the \
                 model runs out of room before it can spend it.",
                profile.thinking_budget
            ),
        ));
    }
}

fn check_experts(context: &ValidationContext<'_>, messages: &mut Vec<ValidationMessage>) {
    let profile = context.profile;
    if profile.moe == MoeOffload::Off {
        return;
    }

    let on_gpu = profile.device.is_gpu()
        || (profile.device.is_auto()
            && context.server_supports_gpu
            && context.vram_bytes.is_some());

    if !on_gpu {
        messages.push(warning(
            "moe-cpu",
            "Expert tensors stay in system RAM",
            "Offloading the experts only does something when the layers run on the GPU. On the \
             CPU every tensor already lives in system RAM.",
        ));
    }
}

fn gigabytes(bytes: u64) -> f32 {
    bytes as f32 / 1_073_741_824.0
}

fn error(code: &str, title: &str, detail: impl Into<String>) -> ValidationMessage {
    ValidationMessage {
        code: code.to_string(),
        severity: Severity::Error,
        title: title.to_string(),
        detail: detail.into(),
    }
}

fn warning(code: &str, title: &str, detail: impl Into<String>) -> ValidationMessage {
    ValidationMessage {
        code: code.to_string(),
        severity: Severity::Warning,
        title: title.to_string(),
        detail: detail.into(),
    }
}

fn info(code: &str, title: &str, detail: impl Into<String>) -> ValidationMessage {
    ValidationMessage {
        code: code.to_string(),
        severity: Severity::Info,
        title: title.to_string(),
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::budget::ModelShape;

    fn shape() -> ModelShape {
        ModelShape {
            file_bytes: 4 * 1_073_741_824,
            n_layers: 32,
            n_kv_heads: 8,
            head_dim: 128,
            max_context: 32_768,
            moe: None,
        }
    }

    fn context<'a>(profile: &'a InferenceProfile, shape: &'a ModelShape) -> ValidationContext<'a> {
        ValidationContext {
            profile,
            resolved: Some(profile),
            running: None,
            shape: Some(shape),
            model_error: None,
            total_ram_bytes: 16 * 1_073_741_824,
            vram_bytes: None,
            gpu_backend: None,
            server_supports_gpu: false,
            server_supports_flash_attn: Some(true),
            has_user_server: false,
            resolved_device: None,
        }
    }

    fn codes(report: &ValidationReport) -> Vec<&str> {
        report
            .messages
            .iter()
            .map(|message| message.code.as_str())
            .collect()
    }

    #[test]
    fn a_quantized_cache_without_flash_attention_is_an_error() {
        let mut profile = InferenceProfile::desktop_default();
        profile.kv_type = KvCacheType::Q8_0;
        profile.flash_attn = false;
        let shape = shape();
        let report = validate(&context(&profile, &shape));
        assert!(report.has_errors());
        assert!(codes(&report).contains(&"invalid-profile"));
    }

    #[test]
    fn a_context_above_the_model_limit_warns() {
        let mut profile = InferenceProfile::desktop_default();
        profile.context_tokens = 131_072;
        let shape = shape();
        let report = validate(&context(&profile, &shape));
        assert!(codes(&report).contains(&"context-above-model"));
        assert!(!report.has_errors());
    }

    #[test]
    fn a_context_the_memory_cannot_hold_warns() {
        let mut profile = InferenceProfile::desktop_default();
        profile.context_tokens = 32_768;
        let shape = shape();
        let mut ctx = context(&profile, &shape);
        ctx.total_ram_bytes = 3 * 1_073_741_824;
        let report = validate(&ctx);
        assert!(codes(&report).contains(&"memory-exceeds"));
    }

    #[test]
    fn automatic_device_resolves_to_the_cpu_without_a_gpu_build() {
        let profile = InferenceProfile::desktop_default();
        let shape = shape();
        let report = validate(&context(&profile, &shape));
        assert!(codes(&report).contains(&"auto-cpu-build"));
    }

    #[test]
    fn automatic_device_resolves_to_the_gpu_with_one() {
        let profile = InferenceProfile::desktop_default();
        let shape = shape();
        let mut ctx = context(&profile, &shape);
        ctx.server_supports_gpu = true;
        ctx.vram_bytes = Some(8 * 1_073_741_824);
        ctx.gpu_backend = Some("CUDA");
        let report = validate(&ctx);
        assert!(codes(&report).contains(&"auto-gpu"));
    }

    #[test]
    fn gpu_mode_without_a_build_is_an_error() {
        let mut profile = InferenceProfile::desktop_default();
        profile.device = ComputeDevice::UserGpu { gpu_layers: 32 };
        let shape = shape();
        let report = validate(&context(&profile, &shape));
        assert!(report.has_errors());
        assert!(codes(&report).contains(&"gpu-no-build"));
    }

    #[test]
    fn a_restart_is_only_needed_when_the_profile_changed() {
        let mut profile = InferenceProfile::desktop_default();
        let shape = shape();
        let mut ctx = context(&profile, &shape);
        ctx.running = Some(&profile);
        assert!(!validate(&ctx).requires_restart);

        let running = InferenceProfile::desktop_default();
        profile.context_tokens = 16_384;
        let mut ctx = context(&profile, &shape);
        ctx.running = Some(&running);
        assert!(validate(&ctx).requires_restart);
    }

    #[test]
    fn an_unresolved_draft_does_not_look_like_a_change() {
        // The running server was started with the device and the thread
        // count resolved. A draft that still says "automatic" means the same
        // thing, so it must not ask for a restart.
        let mut running = InferenceProfile::desktop_default();
        running.device = ComputeDevice::Cpu;
        running.threads = 11;

        let draft = InferenceProfile::desktop_default();
        let shape = shape();
        let resolved = draft.resolved_for(ComputeDevice::Cpu, 12);
        assert_eq!(resolved.threads, 11);

        let mut ctx = context(&draft, &shape);
        ctx.resolved = Some(&resolved);
        ctx.running = Some(&running);
        assert!(!validate(&ctx).requires_restart);
    }

    #[test]
    fn expert_offload_on_the_cpu_is_only_a_warning() {
        let mut profile = InferenceProfile::desktop_default();
        profile.device = ComputeDevice::Cpu;
        profile.moe = MoeOffload::AllExperts;
        let shape = shape();
        let report = validate(&context(&profile, &shape));
        assert!(codes(&report).contains(&"moe-cpu"));
        assert!(!report.has_errors());
    }

    #[test]
    fn a_gpu_run_is_measured_against_vram_not_system_ram() {
        let mut profile = InferenceProfile::desktop_default();
        profile.device = ComputeDevice::UserGpu { gpu_layers: 999 };
        profile.context_tokens = 32_768;
        let shape = shape();
        let mut ctx = context(&profile, &shape);
        // The machine has far too little RAM, but plenty of VRAM: a GPU run
        // must not be told it does not fit.
        ctx.total_ram_bytes = 2 * 1_073_741_824;
        ctx.vram_bytes = Some(24 * 1_073_741_824);
        ctx.resolved_device = Some(ComputeDevice::UserGpu { gpu_layers: 999 });
        let report = validate(&ctx);
        assert!(!codes(&report).contains(&"memory-exceeds"));
    }

    #[test]
    fn a_gpu_run_that_does_not_fit_in_vram_warns() {
        let mut profile = InferenceProfile::desktop_default();
        profile.device = ComputeDevice::UserGpu { gpu_layers: 999 };
        let shape = shape();
        let mut ctx = context(&profile, &shape);
        ctx.vram_bytes = Some(2 * 1_073_741_824);
        ctx.resolved_device = Some(ComputeDevice::UserGpu { gpu_layers: 999 });
        let report = validate(&ctx);
        assert!(codes(&report).contains(&"memory-exceeds"));
    }

    #[test]
    fn a_build_without_flash_attention_is_an_error() {
        let mut profile = InferenceProfile::desktop_default();
        profile.kv_type = KvCacheType::Q8_0;
        profile.flash_attn = true;
        let shape = shape();
        let mut ctx = context(&profile, &shape);
        ctx.server_supports_flash_attn = Some(false);
        let report = validate(&ctx);
        assert!(report.has_errors());
        assert!(codes(&report).contains(&"flash-unsupported"));
    }
}
