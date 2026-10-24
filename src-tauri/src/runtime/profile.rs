//! The inference profile: every llama-server flag the user can tune.
//!
//! A profile is validated before it reaches the supervisor, so the argument
//! builder never has to defend itself against impossible combinations.

use serde::{Deserialize, Serialize};

/// Which configuration a profile is stored for.
///
/// The scopes stack. The global default is the base every model starts from,
/// a model profile replaces it for that one model, and a project profile
/// replaces both while that project is the one being worked in. The more
/// specific the scope, the later it is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ProfileScope {
    /// The app-wide default, used when nothing more specific applies.
    #[default]
    Global,
    /// A profile stored for one model file.
    Model,
    /// A profile stored for one project.
    Project,
}

impl ProfileScope {
    /// True when the scope stores a profile of its own.
    ///
    /// The global default always exists, so it is never an override.
    pub fn is_override(self) -> bool {
        !matches!(self, ProfileScope::Global)
    }
}

/// Where model weights and the KV cache are allowed to live.
///
/// The default is `Auto`: the app picks the GPU when this machine has one
/// and the llama-server build in use can offload to it, and falls back to
/// the CPU otherwise. The GPU path needs a llama-server build with GPU
/// support — the bundled build is CPU only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "device", rename_all = "snake_case")]
pub enum ComputeDevice {
    /// Pick the best backend the machine and the build allow.
    #[default]
    Auto,
    /// Only the CPU: no GPU is used and no VRAM is requested.
    Cpu,
    /// The user's own GPU build of llama-server, offloading `gpu_layers`.
    UserGpu { gpu_layers: u32 },
}

impl ComputeDevice {
    pub fn is_gpu(self) -> bool {
        matches!(self, ComputeDevice::UserGpu { .. })
    }

    /// True when the run is pinned to the CPU.
    pub fn is_cpu(self) -> bool {
        matches!(self, ComputeDevice::Cpu)
    }

    /// True when the device still has to be resolved.
    pub fn is_auto(self) -> bool {
        matches!(self, ComputeDevice::Auto)
    }

    /// The `-ngl` value this device corresponds to. `0` keeps every layer on
    /// the CPU; `999` asks the server to offload as many layers as fit.
    ///
    /// An unresolved `Auto` reports `0`, so estimates stay on the
    /// conservative side until the device has been resolved.
    pub fn gpu_layers(self) -> u32 {
        match self {
            ComputeDevice::Auto | ComputeDevice::Cpu => 0,
            ComputeDevice::UserGpu { gpu_layers } => gpu_layers,
        }
    }

    /// Replaces `Auto` with the device the app will actually use.
    pub fn resolved(self, gpu_layers: Option<u32>) -> Self {
        match self {
            ComputeDevice::Auto => match gpu_layers {
                Some(layers) if layers > 0 => ComputeDevice::UserGpu { gpu_layers: layers },
                _ => ComputeDevice::Cpu,
            },
            other => other,
        }
    }
}

/// How K and V entries in the attention cache are stored.
///
/// Quantized cache types trade a small amount of answer quality for much less
/// memory growth at long contexts. They are the reason a low-RAM laptop can
/// keep a long context open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum KvCacheType {
    #[default]
    F16,
    Q8_0,
    Q5_1,
    Q4_0,
}

impl KvCacheType {
    pub fn as_arg(self) -> &'static str {
        match self {
            KvCacheType::F16 => "f16",
            KvCacheType::Q8_0 => "q8_0",
            KvCacheType::Q5_1 => "q5_1",
            KvCacheType::Q4_0 => "q4_0",
        }
    }

    /// Approximate bytes per cached element, including block-scale overhead:
    /// f16 stores 2 bytes per value, and the quantized types add scale
    /// (q8_0, q5_1) or scale-plus-minimum (q5_1) fields per 32-value block.
    pub fn bytes_per_element(self) -> f64 {
        match self {
            KvCacheType::F16 => 2.0,
            KvCacheType::Q8_0 => 34.0 / 32.0,
            KvCacheType::Q5_1 => 24.0 / 32.0,
            KvCacheType::Q4_0 => 18.0 / 32.0,
        }
    }

    /// Every KV type the UI offers, in the order the recommender prefers.
    /// llama-server accepts these for a quantized cache; finer types like
    /// q3 are not valid cache types and the server would reject them.
    pub const OPTIONS: [KvCacheType; 4] = [
        KvCacheType::F16,
        KvCacheType::Q8_0,
        KvCacheType::Q5_1,
        KvCacheType::Q4_0,
    ];
}

/// Where MoE expert tensors are placed.
///
/// In a mixture-of-experts model most parameters sit in expert feed-forward
/// tensors, and every token activates only a few of them. Placing the experts
/// in system RAM while attention and shared layers go to the GPU lets a small
/// card run a much larger model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum MoeOffload {
    /// Expert tensors follow the normal GPU offload.
    #[default]
    Off,
    /// `--cpu-moe`: all expert tensors stay in system RAM.
    AllExperts,
    /// `--n-cpu-moe N`: experts of the first N layers stay in system RAM.
    FirstLayers { n: u32 },
}

/// The profile as it is read off the wire.
///
/// Container-level default is load-bearing, not an optimisation: a state file
/// written before a setting existed falls back to `desktop_default()` for the
/// fields it does not have, instead of failing to load and taking every other
/// preference down with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ProfileWire {
    context_tokens: u32,
    threads: u32,
    /// Nested, the way every other device on the wire is spelled. See
    /// [`InferenceProfile`]'s `Deserialize` for why it is read by hand.
    device: ComputeDevice,
    kv_type: KvCacheType,
    flash_attn: bool,
    batch_size: u32,
    parallel_slots: u32,
    use_mmap: bool,
    moe: MoeOffload,
    thinking_budget: u32,
    partial_load: bool,
}

/// Rewrites the one shape that changed.
///
/// `device` used to be flattened into the profile, so the tag and `gpu_layers`
/// sat beside the profile's own fields. The nested spelling is left alone.
fn nest_flat_device(value: serde_json::Value) -> serde_json::Value {
    let Some(object) = value.as_object() else {
        return value;
    };
    // Already nested — `device` is an object, not the bare tag string.
    let Some(tag) = object.get("device").and_then(|entry| entry.as_str()) else {
        return value;
    };
    let mut nested = serde_json::Map::new();
    nested.insert("device".into(), serde_json::Value::String(tag.to_string()));
    if let Some(layers) = object.get("gpu_layers").cloned() {
        nested.insert("gpu_layers".into(), layers);
    }
    let mut object = object.clone();
    object.remove("gpu_layers");
    object.insert("device".into(), serde_json::Value::Object(nested));
    serde_json::Value::Object(object)
}

impl Default for ProfileWire {
    fn default() -> Self {
        InferenceProfile::desktop_default().into()
    }
}

impl From<ProfileWire> for InferenceProfile {
    fn from(wire: ProfileWire) -> Self {
        InferenceProfile {
            context_tokens: wire.context_tokens,
            threads: wire.threads,
            device: wire.device,
            kv_type: wire.kv_type,
            flash_attn: wire.flash_attn,
            batch_size: wire.batch_size,
            parallel_slots: wire.parallel_slots,
            use_mmap: wire.use_mmap,
            moe: wire.moe,
            thinking_budget: wire.thinking_budget,
            partial_load: wire.partial_load,
        }
    }
}

impl From<InferenceProfile> for ProfileWire {
    fn from(profile: InferenceProfile) -> Self {
        ProfileWire {
            context_tokens: profile.context_tokens,
            threads: profile.threads,
            device: profile.device,
            kv_type: profile.kv_type,
            flash_attn: profile.flash_attn,
            batch_size: profile.batch_size,
            parallel_slots: profile.parallel_slots,
            use_mmap: profile.use_mmap,
            moe: profile.moe,
            thinking_budget: profile.thinking_budget,
            partial_load: profile.partial_load,
        }
    }
}

impl<'de> Deserialize<'de> for InferenceProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = nest_flat_device(serde_json::Value::deserialize(deserializer)?);
        ProfileWire::deserialize(value)
            .map(Into::into)
            .map_err(serde::de::Error::custom)
    }
}

/// One llama-server launch, fully specified.
///
/// `device` is a nested object, the same spelling the estimate and the runtime
/// status use, so the settings page reads one shape everywhere. An older state
/// file that flattened it is migrated on the way in — see this type's
/// `Deserialize`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InferenceProfile {
    pub context_tokens: u32,
    /// `0` means "pick automatically", which the supervisor and argument
    /// builder resolve from the core count.
    pub threads: u32,
    pub device: ComputeDevice,
    pub kv_type: KvCacheType,
    /// Required whenever the KV cache is quantized.
    pub flash_attn: bool,
    pub batch_size: u32,
    pub parallel_slots: u32,
    pub use_mmap: bool,
    pub moe: MoeOffload,
    /// Cap on reasoning tokens. `0` means "auto": the flag is not passed and
    /// llama-server leaves thinking unrestricted (its default of -1).
    pub thinking_budget: u32,
    /// Let llama-server adapt unset parameters (`--fit on`) instead of
    /// passing an explicit context, so a bloated KV cache lowers the context
    /// rather than failing the load.
    pub partial_load: bool,
}

impl InferenceProfile {
    /// The default profile: automatic device, a comfortable context and
    /// conservative batching, with threads picked automatically.
    pub fn desktop_default() -> Self {
        Self {
            context_tokens: 8_192,
            threads: 0,
            device: ComputeDevice::Auto,
            kv_type: KvCacheType::F16,
            flash_attn: false,
            batch_size: 512,
            parallel_slots: 1,
            use_mmap: true,
            moe: MoeOffload::Off,
            thinking_budget: 0,
            partial_load: false,
        }
    }

    /// The profile as the server will run it: the device resolved and an
    /// automatic thread count replaced by the number of threads it stands
    /// for.
    ///
    /// Two profiles that mean the same thing compare equal here, so an
    /// automatic device does not look like a change just because it is
    /// written down unresolved.
    pub fn resolved_for(&self, device: ComputeDevice, logical_cores: usize) -> Self {
        let mut profile = self.clone();
        profile.device = device;
        profile.threads = threads_effective(self, logical_cores);
        profile
    }

    /// Checks that hold no matter what machine the profile runs on.
    ///
    /// Whether a setting makes sense here — a GPU that is not there, a
    /// context the memory cannot hold — is decided by
    /// [`crate::config::validation`], which knows the hardware.
    pub fn validate(&self) -> Result<(), String> {
        // 0 is "model default": llama-server reads the context length from
        // the GGUF metadata itself.
        if self.context_tokens > 0 && self.context_tokens < 512 {
            return Err("context must be 0 (model default) or at least 512 tokens".into());
        }
        if self.kv_type != KvCacheType::F16 && !self.flash_attn {
            return Err("quantized KV cache requires flash attention".into());
        }
        if let MoeOffload::FirstLayers { n } = &self.moe {
            if *n == 0 {
                return Err("use moe \"off\" instead of firstLayers with n: 0".into());
            }
        }
        if self.batch_size == 0 || self.parallel_slots == 0 {
            return Err("batch size and parallel slots must be at least 1".into());
        }
        Ok(())
    }
}

/// The profile an app starts with when nothing was ever configured.
///
/// This is the same as [`InferenceProfile::desktop_default`], so a state file
/// written before a setting existed still loads into a sensible profile.
impl Default for InferenceProfile {
    fn default() -> Self {
        Self::desktop_default()
    }
}

/// Counterfactual higher threads are harmless, but `0` is a legitimate
/// serialised "auto" value — the supervisor resolves it, the UI never edits
/// it directly. This helper gives the UI the value it should display.
pub fn threads_effective(profile: &InferenceProfile, logical_cores: usize) -> u32 {
    if profile.threads == 0 {
        logical_cores.saturating_sub(1).clamp(1, usize::MAX) as u32
    } else {
        profile.threads
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_passes_validation() {
        assert!(InferenceProfile::desktop_default().validate().is_ok());
    }

    #[test]
    fn the_default_device_is_automatic() {
        assert_eq!(InferenceProfile::desktop_default().device, ComputeDevice::Auto);
    }

    #[test]
    fn automatic_resolves_to_the_cpu_without_a_gpu() {
        assert_eq!(ComputeDevice::Auto.resolved(None), ComputeDevice::Cpu);
        assert_eq!(ComputeDevice::Auto.resolved(Some(0)), ComputeDevice::Cpu);
    }

    #[test]
    fn automatic_resolves_to_the_gpu_when_layers_fit() {
        assert_eq!(
            ComputeDevice::Auto.resolved(Some(12)),
            ComputeDevice::UserGpu { gpu_layers: 12 }
        );
    }

    #[test]
    fn quantized_kv_requires_flash_attention() {
        let mut profile = InferenceProfile::desktop_default();
        profile.kv_type = KvCacheType::Q8_0;
        profile.flash_attn = false;
        assert_eq!(
            profile.validate().err(),
            Some("quantized KV cache requires flash attention".to_string())
        );
        profile.flash_attn = true;
        assert!(profile.validate().is_ok());
    }

    #[test]
    fn moe_placement_is_a_machine_question_not_a_structural_one() {
        // Whether expert offload makes sense depends on the resolved device,
        // so the structural check leaves it alone.
        let mut profile = InferenceProfile::desktop_default();
        profile.moe = MoeOffload::AllExperts;
        assert!(profile.validate().is_ok());
    }

    #[test]
    fn zero_expert_layers_is_rejected() {
        let mut profile = InferenceProfile::desktop_default();
        profile.moe = MoeOffload::FirstLayers { n: 0 };
        assert!(profile.validate().is_err());
    }

    #[test]
    fn tiny_context_is_rejected() {
        let mut profile = InferenceProfile::desktop_default();
        profile.context_tokens = 256;
        assert!(profile.validate().is_err());
    }

    #[test]
    fn zero_context_means_model_default() {
        let mut profile = InferenceProfile::desktop_default();
        profile.context_tokens = 0;
        assert!(profile.validate().is_ok());
    }

    /// The exact shape the Inference tab sends over `apply_runtime_profile`.
    /// Serde errors carry no field path, so the wire format has to be pinned
    /// down by a test instead of by reading the struct.
    #[test]
    fn the_settings_page_payload_deserializes() {
        // The shape the settings page sends, and the shape the core must emit
        // back: `device` nested, the same spelling the estimate and the
        // runtime status already used.
        let wire = r#"{
            "contextTokens": 41472,
            "threads": 11,
            "device": { "device": "user_gpu", "gpu_layers": 999 },
            "kvType": "f16",
            "flashAttn": false,
            "batchSize": 512,
            "parallelSlots": 1,
            "useMmap": true,
            "moe": { "mode": "off" },
            "thinkingBudget": 2048,
            "partialLoad": false
        }"#;
        let profile: InferenceProfile = serde_json::from_str(wire)
            .unwrap_or_else(|error| panic!("settings page payload rejected: {error}"));
        assert_eq!(profile.context_tokens, 41_472);
        assert_eq!(profile.thinking_budget, 2_048);
        assert_eq!(profile.device, ComputeDevice::UserGpu { gpu_layers: 999 });

        // Round trip: what the core writes must read back as the same profile.
        let emitted = serde_json::to_string(&profile).expect("profile serializes");
        assert!(
            emitted.contains(r#""device":{"device":"user_gpu","gpu_layers":999}"#),
            "device must stay nested on the way out, got: {emitted}"
        );
        let back: InferenceProfile =
            serde_json::from_str(&emitted).expect("a profile the core emits must read back");
        assert_eq!(back, profile);

        // The other two device shapes the same tab produces.
        for (device, expected) in [
            (r#"{ "device": "auto" }"#, ComputeDevice::Auto),
            (r#"{ "device": "cpu" }"#, ComputeDevice::Cpu),
        ] {
            let payload = format!(
                r#"{{"contextTokens":8192,"threads":0,"device":{device},"kvType":"f16",
                    "flashAttn":false,"batchSize":512,"parallelSlots":1,"useMmap":true,
                    "moe":{{"mode":"off"}},"thinkingBudget":0,"partialLoad":false}}"#
            );
            let parsed: InferenceProfile = serde_json::from_str(&payload)
                .unwrap_or_else(|error| panic!("device shape {device} rejected: {error}"));
            assert_eq!(parsed.device, expected);
        }
    }

    /// A state file written before `device` stopped being flattened into the
    /// profile must still load, and must keep the layers it stored.
    #[test]
    fn a_state_file_with_a_flattened_device_still_loads() {
        for (wire, expected) in [
            (r#""device":"cpu""#, ComputeDevice::Cpu),
            (r#""device":"auto""#, ComputeDevice::Auto),
            (
                r#""device":"user_gpu","gpu_layers":999"#,
                ComputeDevice::UserGpu { gpu_layers: 999 },
            ),
        ] {
            let payload = format!(
                r#"{{"contextTokens":32768,"threads":11,{wire},"kvType":"q4_0",
                    "flashAttn":true,"batchSize":512,"parallelSlots":1,"useMmap":true,
                    "moe":{{"mode":"off"}},"thinkingBudget":2048,"partialLoad":false}}"#
            );
            let profile: InferenceProfile = serde_json::from_str(&payload)
                .unwrap_or_else(|error| panic!("legacy shape {wire} rejected: {error}"));
            assert_eq!(profile.device, expected);
            assert_eq!(profile.context_tokens, 32_768);
        }
    }

    /// A profile written before a setting existed must still load, falling
    /// back to the default for the fields it does not carry.
    #[test]
    fn a_profile_missing_the_newer_settings_still_loads() {
        let wire = r#"{
            "contextTokens": 16384,
            "threads": 6,
            "device": { "device": "cpu" },
            "kvType": "q8_0",
            "flashAttn": true,
            "batchSize": 256,
            "parallelSlots": 1,
            "useMmap": true,
            "moe": { "mode": "off" }
        }"#;
        let profile: InferenceProfile = serde_json::from_str(wire)
            .unwrap_or_else(|error| panic!("older profile rejected: {error}"));
        assert_eq!(profile.thinking_budget, 0);
        assert!(!profile.partial_load);
        // The defaults the container falls back to are the desktop defaults,
        // not zeros — a zero batch size would fail validation on load.
        assert_eq!(
            InferenceProfile::from(ProfileWire::default()),
            InferenceProfile::desktop_default()
        );
    }

    /// The estimate prices every cache type under `as_arg`, so the two
    /// spellings must stay the same word or the map keys stop matching what
    /// the settings page sends back.
    #[test]
    fn kv_type_argument_names_match_the_wire_names() {
        for kv in KvCacheType::OPTIONS {
            let wire = serde_json::to_value(kv).expect("kv type serializes");
            assert_eq!(wire, serde_json::json!(kv.as_arg()));
        }
    }
}
