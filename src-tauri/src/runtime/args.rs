//! Builds the llama-server command line from a validated profile.
//!
//! This is the only place llama-server flags are spelled out, so the UI, the
//! recommender and the supervisor all agree on what a profile means.
//!
//! Flag surface checked against llama.cpp v0.7.0. Two breaking changes from
//! the older `b*` tags are handled here: the API key env is now
//! `LLAMA_API_KEY` (was `LLAMA_ARG_API_KEY`) and mmap is disabled through
//! `--load-mode none` (the old `--no-mmap` no longer exists).

use std::path::Path;

use super::profile::{InferenceProfile, MoeOffload};

/// The assembled process description.
#[derive(Debug, Clone)]
pub struct ServerLaunch {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// Build the llama-server invocation for this profile.
///
/// The API key goes through the environment rather than argv, so it does not
/// appear in the process list. In CPU-only mode the server is also asked not
/// to enumerate any GPU device, and CUDA runtime discovery is disabled, so a
/// machine with CUDA installed still sees zero VRAM used.
pub fn build_launch(
    model: &Path,
    projector: Option<&Path>,
    port: u16,
    api_key: &str,
    profile: &InferenceProfile,
) -> Result<ServerLaunch, String> {
    profile.validate()?;

    // Resolve "auto" threads before the flags are emitted; llama-server
    // itself would reject `-t 0`.
    let threads = if profile.threads == 0 {
        default_threads()
    } else {
        profile.threads
    };

    let flash = if profile.flash_attn { "on" } else { "off" };
    let kv_arg = profile.kv_type.as_arg();

    let mut args: Vec<String> = vec![
        // `-m` loads the GGUF weights. llama-server automatically reads the
        // tokenizer (vocab + merges) embedded in the GGUF file itself, so no
        // separate `--tokenizer` flag is needed — we use the tokenizer that
        // ships with the model. Passing one here would only override it.
        "-m".into(),
        model.to_string_lossy().into_owned(),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string(),
        "-t".into(),
        threads.to_string(),
        "-ngl".into(),
        profile.device.gpu_layers().to_string(),
        "-b".into(),
        profile.batch_size.to_string(),
        "-np".into(),
        profile.parallel_slots.to_string(),
        "-ctk".into(),
        kv_arg.into(),
        "-ctv".into(),
        kv_arg.into(),
        "-fa".into(),
        flash.into(),
    ];

    // Partial loading hands the context decision to llama-server's fitter:
    // it lowers the context until the model and its KV cache fit device
    // memory, bounded by --fit-ctx. An explicit -c would disable that.
    if profile.partial_load {
        args.push("--fit".into());
        args.push("on".into());
        args.push("--fit-ctx".into());
        args.push("512".into());
    } else {
        args.push("-c".into());
        args.push(profile.context_tokens.to_string());
    }

    // A multimodal projector that lives next to the weights. llama.cpp takes
    // it as a separate flag, and it only makes sense together with the
    // model it belongs to.
    if let Some(mmproj) = projector {
        args.push("--mmproj".into());
        args.push(mmproj.to_string_lossy().into_owned());
    }

    // 0 means "auto": without the flag llama-server leaves thinking free.
    if profile.thinking_budget > 0 {
        args.push("--reasoning-budget".into());
        args.push(profile.thinking_budget.to_string());
    }

    match profile.moe {
        MoeOffload::Off => {}
        MoeOffload::AllExperts => args.push("--cpu-moe".into()),
        MoeOffload::FirstLayers { n } => {
            args.push("--n-cpu-moe".into());
            args.push(n.to_string());
        }
    }

    if !profile.use_mmap {
        // v0.7.0: `--no-mmap` was folded into the load-mode switch and
        // `none` is exactly the old behaviour: read the model without mmap.
        args.push("--load-mode".into());
        args.push("none".into());
    }

    // v0.7.0 renamed the key env for `--api-key` to LLAMA_API_KEY. The key
    // goes through the environment rather than argv, so it does not appear
    // in the process list.
    let mut env = vec![("LLAMA_API_KEY".into(), api_key.to_string())];

    // A run without offloaded layers must not touch a GPU, even when the
    // binary happens to be a GPU build. An unresolved `Auto` lands here too,
    // and it means the same thing: no layer leaves the CPU. Every backend
    // gets its own empty device list: CUDA through the launcher flag and the
    // classic env, Vulkan and OpenCL through their loader envs.
    if profile.device.gpu_layers() == 0 {
        args.push("--device".into());
        args.push("none".into());
        env.push(("CUDA_VISIBLE_DEVICES".into(), "".into()));
        env.push(("GGML_VK_VISIBLE_DEVICES".into(), "".into()));
        env.push(("GGML_OPENCL_VISIBLE_DEVICES".into(), "".into()));
    }

    Ok(ServerLaunch { args, env })
}

/// Leaves a core or two for the rest of the machine.
fn default_threads() -> u32 {
    let cores = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(2);
    cores.saturating_sub(1).max(1) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::profile::{ComputeDevice, InferenceProfile, KvCacheType, MoeOffload};

    #[test]
    fn cpu_launch_has_no_gpu_flags() {
        let mut profile = InferenceProfile::desktop_default();
        profile.device = ComputeDevice::Cpu;
        let launch = build_launch(Path::new("m.gguf"), None, 8080, "key", &profile).unwrap();
        let joined = launch.args.join(" ");
        let ngl = launch
            .args
            .iter()
            .position(|a| a == "-ngl")
            .and_then(|i| launch.args.get(i + 1).cloned());
        assert_eq!(ngl.as_deref(), Some("0"));
        assert!(joined.contains("--device none"));
        assert!(launch.env.contains(&("CUDA_VISIBLE_DEVICES".to_string(), "".to_string())));
        assert!(launch.env.contains(&("GGML_VK_VISIBLE_DEVICES".to_string(), "".to_string())));
        assert!(launch.env.contains(&("GGML_OPENCL_VISIBLE_DEVICES".to_string(), "".to_string())));
        assert!(launch.env.contains(&("LLAMA_API_KEY".to_string(), "key".to_string())));
        assert!(!launch.args.contains(&"--cpu-moe".to_string()));
        assert!(launch.args.contains(&"-ngl".to_string()));
    }

    #[test]
    fn an_unresolved_device_never_reaches_for_the_gpu() {
        // `Auto` is resolved before the launch, but a profile that still
        // carries it must not ask the server for a device.
        let profile = InferenceProfile::desktop_default();
        assert!(profile.device.is_auto());
        let launch = build_launch(Path::new("m.gguf"), None, 8080, "key", &profile).unwrap();
        assert!(launch.args.join(" ").contains("--device none"));
        assert!(launch.env.contains(&("CUDA_VISIBLE_DEVICES".to_string(), "".to_string())));
    }

    #[test]
    fn a_gpu_run_leaves_the_devices_in_place() {
        let mut profile = InferenceProfile::desktop_default();
        profile.device = ComputeDevice::UserGpu { gpu_layers: 8 };
        let launch = build_launch(Path::new("m.gguf"), None, 8080, "key", &profile).unwrap();
        assert!(!launch.args.contains(&"none".to_string()));
        assert!(!launch.env.contains(&("CUDA_VISIBLE_DEVICES".to_string(), "".to_string())));
        assert!(!launch.env.contains(&("GGML_VK_VISIBLE_DEVICES".to_string(), "".to_string())));
    }

    #[test]
    fn quantized_kv_sets_ctk_ctv_and_fa() {
        let mut profile = InferenceProfile::desktop_default();
        profile.kv_type = KvCacheType::Q4_0;
        profile.flash_attn = true;
        let launch = build_launch(Path::new("m.gguf"), None, 8080, "key", &profile).unwrap();
        assert!(launch.args.contains(&"q4_0".to_string()));
        assert_eq!(
            launch
                .args
                .iter()
                .filter(|arg| arg.as_str() == "q4_0")
                .count(),
            2
        );
        assert!(launch.args.contains(&"on".to_string()));
    }

    #[test]
    fn moe_first_layers_emits_n_cpu_moe() {
        let mut profile = InferenceProfile::desktop_default();
        profile.device = ComputeDevice::UserGpu { gpu_layers: 999 };
        profile.moe = MoeOffload::FirstLayers { n: 12 };
        let launch = build_launch(Path::new("m.gguf"), None, 8080, "key", &profile).unwrap();
        let ngl = launch
            .args
            .iter()
            .position(|a| a == "-ngl")
            .and_then(|i| launch.args.get(i + 1))
            .and_then(|v| v.parse::<u32>().ok());
        assert_eq!(ngl, Some(999));
        assert!(launch.args.contains(&"--n-cpu-moe".to_string()));
        assert!(launch.args.contains(&"12".to_string()));
    }

    #[test]
    fn no_mmap_uses_load_mode_none() {
        let mut profile = InferenceProfile::desktop_default();
        profile.use_mmap = false;
        let launch = build_launch(Path::new("m.gguf"), None, 8080, "key", &profile).unwrap();
        let mode = launch
            .args
            .iter()
            .position(|arg| arg == "--load-mode")
            .and_then(|i| launch.args.get(i + 1).cloned());
        assert_eq!(mode.as_deref(), Some("none"));
        assert!(!launch.args.contains(&"--no-mmap".to_string()));
    }

    #[test]
    fn api_key_never_appears_in_argv() {
        let profile = InferenceProfile::desktop_default();
        let launch = build_launch(Path::new("m.gguf"), None, 8080, "secret", &profile).unwrap();
        assert!(launch.args.iter().all(|arg| !arg.contains("secret")));
    }

    #[test]
    fn invalid_profile_is_rejected() {
        let mut profile = InferenceProfile::desktop_default();
        profile.kv_type = KvCacheType::Q8_0;
        profile.flash_attn = false;
        assert!(build_launch(Path::new("m.gguf"), None, 8080, "key", &profile).is_err());
    }

    #[test]
    fn auto_threads_resolve_to_a_real_count() {
        let profile = InferenceProfile::desktop_default();
        let launch = build_launch(Path::new("m.gguf"), None, 8080, "key", &profile).unwrap();
        let t = launch
            .args
            .iter()
            .position(|arg| arg == "-t")
            .and_then(|i| launch.args.get(i + 1))
            .and_then(|value| value.parse::<u32>().ok());
        assert!(t.unwrap_or(0) >= 1, "auto threads must resolve to at least 1");
    }

    #[test]
    fn thinking_budget_is_explicit_only_when_capped() {
        let mut profile = InferenceProfile::desktop_default();
        profile.thinking_budget = 2_048;
        let launch = build_launch(Path::new("m.gguf"), None, 8080, "key", &profile).unwrap();
        let budget = launch
            .args
            .iter()
            .position(|arg| arg == "--reasoning-budget")
            .and_then(|i| launch.args.get(i + 1).cloned());
        assert_eq!(budget.as_deref(), Some("2048"));

        // Auto: no flag, llama-server keeps its unrestricted default.
        let mut auto_profile = InferenceProfile::desktop_default();
        auto_profile.thinking_budget = 0;
        let auto = build_launch(Path::new("m.gguf"), None, 8080, "key", &auto_profile).unwrap();
        assert!(!auto.args.contains(&"--reasoning-budget".to_string()));
    }

    #[test]
    fn a_projector_next_to_the_weights_is_passed_along() {
        let profile = InferenceProfile::desktop_default();
        let launch = build_launch(
            Path::new("m.gguf"),
            Some(Path::new("mmproj.gguf")),
            8080,
            "key",
            &profile,
        )
        .unwrap();
        assert!(launch.args.contains(&"--mmproj".to_string()));
        assert!(launch.args.contains(&"mmproj.gguf".to_string()));

        // Without a projector the flag is left out entirely.
        let plain = build_launch(Path::new("m.gguf"), None, 8080, "key", &profile).unwrap();
        assert!(!plain.args.contains(&"--mmproj".to_string()));
    }

    #[test]
    fn zero_context_is_passed_through_for_the_model_default() {
        let mut profile = InferenceProfile::desktop_default();
        profile.context_tokens = 0;
        let launch = build_launch(Path::new("m.gguf"), None, 8080, "key", &profile).unwrap();
        let ctx = launch
            .args
            .iter()
            .position(|arg| arg == "-c")
            .and_then(|i| launch.args.get(i + 1).cloned());
        assert_eq!(ctx.as_deref(), Some("0"), "llama-server reads the model default for -c 0");
    }

    #[test]
    fn partial_load_omits_the_explicit_context() {
        let mut profile = InferenceProfile::desktop_default();
        profile.partial_load = true;
        let launch = build_launch(Path::new("m.gguf"), None, 8080, "key", &profile).unwrap();
        // fitter owns the context, so must not be pinned by -c
        assert!(!launch.args.contains(&"-c".to_string()));
        assert!(launch.args.contains(&"--fit".to_string()));
        assert!(launch.args.contains(&"--fit-ctx".to_string()));

        // Default keeps the context explicit.
        let pinned = build_launch(Path::new("m.gguf"), None, 8080, "key", &InferenceProfile::desktop_default())
            .unwrap();
        assert!(pinned.args.contains(&"-c".to_string()));
        assert!(!pinned.args.contains(&"--fit".to_string()));
    }
}
