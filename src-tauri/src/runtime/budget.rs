//! Memory estimates for llama-server runs.
//!
//! The weights often come from an mmap'd GGUF file and the KV cache is
//! allocated in RAM, so long documents can exhaust memory before the model
//! does. These estimates let localAIse pick a context length and KV type that
//! actually fit, before llama-server ever starts.

use crate::runtime::profile::{InferenceProfile, KvCacheType, MoeOffload};

const GIB: u64 = 1024 * 1024 * 1024;

/// RAM kept free for the OS, the UI and the app itself.
pub const RAM_RESERVE_BYTES: u64 = GIB + GIB / 2;

/// VRAM kept free for the desktop compositor and driver overhead.
pub const VRAM_RESERVE_BYTES: u64 = 512 * 1024 * 1024;

/// Working memory llama.cpp needs on top of the weights and the cache:
/// the compute buffer, the graph and the tokenizer scratch space. This is an
/// approximation — llama.cpp's real figure depends on the batch size.
pub const RUNTIME_OVERHEAD_BYTES: u64 = 512 * 1024 * 1024;

/// How well a configuration fits into the memory a machine has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FitLevel {
    /// Plenty of room left.
    Comfortable,
    /// Fits, but the margin is thin.
    Tight,
    /// Does not fit.
    Exceeds,
}

impl FitLevel {
    /// Whether the configuration fits at all.
    pub fn fits(self) -> bool {
        !matches!(self, FitLevel::Exceeds)
    }
}

/// Classifies how much of a budget a configuration uses.
///
/// The thresholds are deliberately conservative: below 85% of the budget is
/// comfortable, up to the budget is tight, and anything above it exceeds
/// what the machine has.
pub fn fit_level(needed_bytes: u64, budget_bytes: u64) -> FitLevel {
    if budget_bytes == 0 {
        return FitLevel::Exceeds;
    }
    if needed_bytes > budget_bytes {
        return FitLevel::Exceeds;
    }
    if needed_bytes * 100 > budget_bytes * 85 {
        return FitLevel::Tight;
    }
    FitLevel::Comfortable
}

/// GGUF shape facts read from the model file.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ModelShape {
    pub file_bytes: u64,
    pub n_layers: u32,
    pub n_kv_heads: u32,
    pub head_dim: u32,
    /// Context length the file declares, 0 when absent.
    pub max_context: u32,
    pub moe: Option<MoeShape>,
}

/// Mixture-of-experts specifics, also read from GGUF metadata.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MoeShape {
    pub n_experts: u32,
    pub n_experts_used: u32,
    /// Total bytes of all expert tensors across all layers.
    pub expert_bytes_total: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostCaps {
    pub total_ram_bytes: u64,
    pub logical_cores: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuCaps {
    pub vram_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemorySplit {
    pub ram_bytes: u64,
    pub vram_bytes: u64,
}

impl ModelShape {
    fn expert_bytes(&self) -> u64 {
        self.moe.map_or(0, |m| m.expert_bytes_total)
    }

    fn expert_bytes_per_layer(&self) -> u64 {
        if self.n_layers == 0 {
            return 0;
        }
        self.expert_bytes() / self.n_layers as u64
    }

    /// Weights that are not expert tensors: attention, norms, embeddings.
    fn dense_bytes(&self) -> u64 {
        self.file_bytes.saturating_sub(self.expert_bytes())
    }
}

/// K and V for every layer, for `ctx` tokens.
pub fn kv_cache_bytes(shape: &ModelShape, ctx: u32, kv: KvCacheType) -> u64 {
    let elements = ctx as f64
        * shape.n_layers as f64
        * shape.n_kv_heads as f64
        * shape.head_dim as f64
        * 2.0; // K and V
    (elements * kv.bytes_per_element()) as u64
}

/// The context the memory estimate should assume: the profile's pick, or
/// when the profile says "model default" (0), the model's declared context.
pub fn effective_context(profile: &InferenceProfile, shape: &ModelShape) -> u32 {
    if profile.context_tokens > 0 {
        profile.context_tokens
    } else {
        shape.max_context.max(8_192)
    }
}

/// A shape we read nothing useful about (corrupt file, unknown arch, missing
/// metadata). The recommender then gives up rather than guessing.
pub fn is_known(shape: &ModelShape) -> bool {
    shape.n_layers > 0 && shape.n_kv_heads > 0 && shape.head_dim > 0
}

/// Estimate where the weights and KV cache land for a given profile.
///
/// Assumes that when layers are offloaded to the GPU, all non-expert layers
/// and the KV cache go to the GPU. Partial dense offload is not modeled, so
/// the estimate is conservative for VRAM.
pub fn split(shape: &ModelShape, profile: &InferenceProfile) -> MemorySplit {
    let kv = kv_cache_bytes(
        shape,
        effective_context(profile, shape),
        profile.kv_type,
    );

    if profile.device.gpu_layers() == 0 {
        return MemorySplit {
            ram_bytes: shape.file_bytes + kv,
            vram_bytes: 0,
        };
    }

    let cpu_expert_layers = match profile.moe {
        MoeOffload::Off => 0,
        MoeOffload::AllExperts => shape.n_layers,
        MoeOffload::FirstLayers { n } => n.min(shape.n_layers),
    };

    let expert_on_cpu = shape.expert_bytes_per_layer() * cpu_expert_layers as u64;
    let expert_on_gpu = shape.expert_bytes().saturating_sub(expert_on_cpu);

    MemorySplit {
        ram_bytes: expert_on_cpu,
        vram_bytes: shape.dense_bytes() + kv + expert_on_gpu,
    }
}

/// How many layers can be offloaded to a GPU of this size.
///
/// The cache and the runtime overhead are reserved first, then the weights
/// are divided evenly across the layers. Returns `None` when the shape is
/// unknown, and `Some(0)` when not even one layer fits.
pub fn auto_gpu_layers(
    shape: &ModelShape,
    profile: &InferenceProfile,
    vram_bytes: u64,
) -> Option<u32> {
    if !is_known(shape) || shape.n_layers == 0 {
        return None;
    }

    let kv = kv_cache_bytes(shape, effective_context(profile, shape), profile.kv_type);
    let budget = vram_bytes.saturating_sub(VRAM_RESERVE_BYTES);
    let room = budget
        .saturating_sub(kv)
        .saturating_sub(RUNTIME_OVERHEAD_BYTES);

    let per_layer = shape.file_bytes / shape.n_layers as u64;
    if per_layer == 0 {
        return None;
    }

    Some((room / per_layer).min(shape.n_layers as u64) as u32)
}

/// CPU-only recommendation: largest context that fits in RAM, with the
/// highest-precision KV cache that still fits at that context. Returns `None`
/// when even the smallest context does not fit.
pub fn recommend_cpu(
    shape: &ModelShape,
    host: &HostCaps,
    wanted_ctx: u32,
) -> Option<InferenceProfile> {
    const MIN_CTX: u32 = 2048;

    if !is_known(shape) {
        return None;
    }

    let budget = host
        .total_ram_bytes
        .saturating_sub(RAM_RESERVE_BYTES);

    let mut ctx = wanted_ctx;
    while ctx >= MIN_CTX {
        for kv in KvCacheType::OPTIONS {
            let usage = shape.file_bytes + kv_cache_bytes(shape, ctx, kv);
            if usage <= budget {
                return Some(InferenceProfile {
                    context_tokens: ctx,
                    threads: host.logical_cores.saturating_sub(1).max(1),
                    device: crate::runtime::profile::ComputeDevice::Auto,
                    kv_type: kv,
                    flash_attn: kv != KvCacheType::F16,
                    batch_size: 512,
                    parallel_slots: 1,
                    use_mmap: true,
                    moe: MoeOffload::Off,
                    thinking_budget: 0,
                    partial_load: false,
                });
            }
        }
        ctx /= 2;
    }
    None
}

/// A configuration that fits this machine, preferring the GPU when the
/// runtime build can use one.
///
/// The GPU is only tried when the weights and the cache fit in the card's
/// memory; otherwise the CPU recommendation is the honest answer.
pub fn recommend(
    shape: &ModelShape,
    host: &HostCaps,
    gpu: Option<GpuCaps>,
    wanted_ctx: u32,
) -> Option<InferenceProfile> {
    if !is_known(shape) {
        return None;
    }
    if let Some(gpu) = gpu {
        if let Some(profile) = recommend_gpu(shape, host, gpu, wanted_ctx) {
            return Some(profile);
        }
    }
    recommend_cpu(shape, host, wanted_ctx)
}

/// The largest context that fits in VRAM, with the layers the card can hold.
fn recommend_gpu(
    shape: &ModelShape,
    host: &HostCaps,
    gpu: GpuCaps,
    wanted_ctx: u32,
) -> Option<InferenceProfile> {
    const MIN_CTX: u32 = 2048;

    let vram_budget = gpu.vram_bytes.saturating_sub(VRAM_RESERVE_BYTES);
    let ram_budget = host.total_ram_bytes.saturating_sub(RAM_RESERVE_BYTES);
    let per_layer = shape.file_bytes / shape.n_layers.max(1) as u64;
    if per_layer == 0 {
        return None;
    }

    let mut ctx = wanted_ctx;
    while ctx >= MIN_CTX {
        for kv in KvCacheType::OPTIONS {
            let base = InferenceProfile {
                context_tokens: ctx,
                threads: host.logical_cores.saturating_sub(1).max(1),
                device: crate::runtime::profile::ComputeDevice::Cpu,
                kv_type: kv,
                flash_attn: kv != KvCacheType::F16,
                batch_size: 512,
                parallel_slots: 1,
                use_mmap: true,
                moe: MoeOffload::Off,
                thinking_budget: 0,
                partial_load: false,
            };

            let kv_bytes = kv_cache_bytes(shape, ctx, kv);
            let room = vram_budget
                .saturating_sub(kv_bytes)
                .saturating_sub(RUNTIME_OVERHEAD_BYTES);
            let layers = (room / per_layer).min(shape.n_layers as u64);
            if layers == 0 {
                continue;
            }

            let profile = InferenceProfile {
                device: crate::runtime::profile::ComputeDevice::UserGpu {
                    gpu_layers: layers as u32,
                },
                ..base
            };

            let memory = split(shape, &profile);
            if memory.vram_bytes <= vram_budget && memory.ram_bytes <= ram_budget {
                return Some(profile);
            }
        }
        ctx /= 2;
    }
    None
}

/// Hybrid GPU recommendation for MoE models.
///
/// Offloads all layers to the GPU, then keeps as few expert layers on the GPU
/// as needed. Each expert layer kept in RAM reduces VRAM use and also reduces
/// generation speed, so the search starts from zero CPU expert layers and
/// increases only until the VRAM budget is met.
///
/// The KV cache type comes straight from the base profile, so conduit-side
/// tuning stays in one place: the caller can quantize the cache first and
/// re-run this search if they want a smaller card to work.
pub fn recommend_moe_hybrid(
    shape: &ModelShape,
    host: &HostCaps,
    gpu: &GpuCaps,
    base: &InferenceProfile,
) -> Option<InferenceProfile> {
    shape.moe?;
    if !is_known(shape) {
        return None;
    }

    let vram_budget = gpu.vram_bytes.saturating_sub(VRAM_RESERVE_BYTES);
    let ram_budget = host.total_ram_bytes.saturating_sub(RAM_RESERVE_BYTES);

    for n in 0..=shape.n_layers {
        let profile = InferenceProfile {
            threads: if base.threads == 0 {
                host.logical_cores.saturating_sub(1).max(1)
            } else {
                base.threads
            },
            device: crate::runtime::profile::ComputeDevice::UserGpu { gpu_layers: 999 },
            moe: if n == 0 {
                MoeOffload::Off
            } else {
                MoeOffload::FirstLayers { n }
            },
            ..base.clone()
        };
        let m = split(shape, &profile);
        if m.vram_bytes <= vram_budget && m.ram_bytes <= ram_budget {
            return Some(profile);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::profile::ComputeDevice;

    fn shape() -> ModelShape {
        ModelShape {
            file_bytes: 4 * GIB,
            n_layers: 32,
            n_kv_heads: 8,
            head_dim: 128,
            max_context: 0,
            moe: None,
        }
    }

    fn host(ram_gib: u64) -> HostCaps {
        HostCaps {
            total_ram_bytes: ram_gib * GIB,
            logical_cores: 8,
        }
    }

    #[test]
    fn cpu_profile_puts_everything_in_ram() {
        let profile = InferenceProfile::desktop_default();
        let m = split(&shape(), &profile);
        assert!(m.ram_bytes > 4 * GIB);
        assert_eq!(m.vram_bytes, 0);
    }

    #[test]
    fn moe_hybrid_is_none_for_dense_models() {
        let base = InferenceProfile::desktop_default();
        assert!(recommend_moe_hybrid(&shape(), &host(16), &GpuCaps { vram_bytes: 8 * GIB }, &base).is_none());
    }

    #[test]
    fn recommend_respects_tight_ram() {
        // 4 GB weights + reserve: on a 6 GB machine even the smallest context
        // must fail, because 4 GB of weights with a 1.5 GB reserve fits in a
        // 6 GB total... so use 5 GB, which cannot fit both weights and cache.
        let tiny = host(5);
        assert!(recommend_cpu(&shape(), &tiny, 8_192).is_none());
    }

    #[test]
    fn the_recommendation_leaves_the_device_automatic() {
        let fit = recommend_cpu(&shape(), &host(16), 32_768).expect("16 GB should fit 4 GB model");
        assert_eq!(fit.device, ComputeDevice::Auto);
    }

    // --- checks: fit classification ------------------------------------------

    #[test]
    fn fit_level_separates_comfortable_tight_and_exceeding() {
        let budget = 10 * GIB;
        assert_eq!(fit_level(5 * GIB, budget), FitLevel::Comfortable);
        assert_eq!(fit_level(9 * GIB, budget), FitLevel::Tight);
        assert_eq!(fit_level(11 * GIB, budget), FitLevel::Exceeds);
        assert_eq!(fit_level(10 * GIB, budget), FitLevel::Tight);
        assert!(!FitLevel::Exceeds.fits());
        assert!(FitLevel::Tight.fits());
    }

    // --- checks: automatic GPU offloading ------------------------------------

    #[test]
    fn automatic_offload_fits_layers_into_vram() {
        // 4 GB of weights over 32 layers is 128 MB per layer. A 12 GB card
        // with 1 GB reserved for the cache and the overhead leaves room for
        // roughly 80 layers, so every layer fits.
        let mut profile = InferenceProfile::desktop_default();
        profile.context_tokens = 8_192;
        let layers = auto_gpu_layers(&shape(), &profile, 12 * GIB).expect("shape is known");
        assert_eq!(layers, 32, "a big card takes every layer");
    }

    #[test]
    fn automatic_offload_leaves_layers_on_the_cpu_when_vram_is_small() {
        let mut profile = InferenceProfile::desktop_default();
        profile.context_tokens = 8_192;
        // 2 GB card: less than one layer fits after the reserves.
        let layers = auto_gpu_layers(&shape(), &profile, 2 * GIB).expect("shape is known");
        assert_eq!(layers, 0);
    }

    #[test]
    fn automatic_offload_never_claims_more_layers_than_the_model_has() {
        let mut profile = InferenceProfile::desktop_default();
        profile.context_tokens = 8_192;
        let layers = auto_gpu_layers(&shape(), &profile, 64 * GIB).expect("shape is known");
        assert_eq!(layers, 32);
    }

    #[test]
    fn automatic_offload_gives_up_on_an_unknown_shape() {
        let unknown = ModelShape::default();
        let profile = InferenceProfile::desktop_default();
        assert_eq!(auto_gpu_layers(&unknown, &profile, 16 * GIB), None);
    }

    #[test]
    fn recommend_fits_comfortable_machine() {
        let fit = recommend_cpu(&shape(), &host(16), 32_768).expect("16 GB should fit 4 GB model");
        assert!(fit.device.is_auto());
        assert!(fit.context_tokens >= 2_048);
        let m = split(&shape(), &fit);
        assert!(m.ram_bytes <= 16 * GIB - RAM_RESERVE_BYTES);
    }

    #[test]
    fn unknown_shape_never_recommends() {
        let unknown = ModelShape {
            file_bytes: 4 * GIB,
            n_layers: 0,
            n_kv_heads: 0,
            head_dim: 0,
            max_context: 0,
            moe: None,
        };
        assert!(recommend_cpu(&unknown, &host(32), 8_192).is_none());
    }

    // --- checks: VRAM/RAM split per device and model class -------------------

    #[test]
    fn gpu_full_offload_moves_dense_weights_and_kv_to_vram() {
        let mut profile = InferenceProfile::desktop_default();
        profile.device = ComputeDevice::UserGpu { gpu_layers: 999 };
        profile.context_tokens = 4_096;
        let m = split(&shape(), &profile);
        let kv = kv_cache_bytes(&shape(), 4_096, KvCacheType::F16);
        assert_eq!(m.vram_bytes, 4 * GIB + kv, "dense weights + KV in VRAM");
        assert_eq!(m.ram_bytes, 0, "nothing is left in RAM at full offload");
    }

    #[test]
    fn moe_placement_moves_expert_bytes_between_ram_and_vram() {
        // 4 GB dense + 6 GB of experts = 10 GB file.
        let mut moe = shape();
        moe.file_bytes = 10 * GIB;
        moe.moe = Some(MoeShape {
            n_experts: 8,
            n_experts_used: 2,
            expert_bytes_total: 6 * GIB,
        });
        let kv = kv_cache_bytes(&moe, 4_096, KvCacheType::F16);
        let base = InferenceProfile::desktop_default();
        let gpu = |moe: MoeOffload| InferenceProfile {
            device: ComputeDevice::UserGpu { gpu_layers: 999 },
            context_tokens: 4_096,
            moe,
            ..base.clone()
        };

        // Off: expert tensors follow the layers onto the GPU.
        let m = split(&moe, &gpu(MoeOffload::Off));
        assert_eq!(m.vram_bytes, 10 * GIB + kv);
        assert_eq!(m.ram_bytes, 0);

        // AllExperts: --cpu-moe parks every expert in RAM.
        let m = split(&moe, &gpu(MoeOffload::AllExperts));
        assert_eq!(m.ram_bytes, 6 * GIB);
        assert_eq!(m.vram_bytes, 4 * GIB + kv, "only dense weights + KV in VRAM");

        // FirstLayers: half of the 32 layers' experts on the CPU.
        let m = split(&moe, &gpu(MoeOffload::FirstLayers { n: 16 }));
        assert_eq!(m.ram_bytes, 3 * GIB);
        assert_eq!(m.vram_bytes, 4 * GIB + 3 * GIB + kv);

        // n beyond the layer count clamps instead of half-guessing.
        let m = split(&moe, &gpu(MoeOffload::FirstLayers { n: 99 }));
        assert_eq!(m.ram_bytes, 6 * GIB);
    }

    #[test]
    fn dense_model_ignores_moe_placement_switches() {
        let mut profile = InferenceProfile::desktop_default();
        profile.device = ComputeDevice::UserGpu { gpu_layers: 999 };
        profile.moe = MoeOffload::AllExperts;
        let m = split(&shape(), &profile);
        assert_eq!(m.ram_bytes, 0, "no expert tensors to park");
        assert_eq!(m.vram_bytes, 4 * GIB + kv_cache_bytes(&shape(), 8_192, KvCacheType::F16));
    }

    // --- checks: KV cache quantisation ---------------------------------------

    #[test]
    fn quantized_kv_sizes_match_the_block_formats() {
        let ctx = 4_096;
        // 4096 tokens * 32 layers * 8 kv-heads * 128 dim * 2 (K and V)
        let elements = ctx as u64 * 32 * 8 * 128 * 2;
        assert_eq!(kv_cache_bytes(&shape(), ctx, KvCacheType::F16), elements * 2);
        assert_eq!(
            kv_cache_bytes(&shape(), ctx, KvCacheType::Q8_0),
            elements * 34 / 32
        );
        assert_eq!(
            kv_cache_bytes(&shape(), ctx, KvCacheType::Q5_1),
            elements * 24 / 32
        );
        assert_eq!(
            kv_cache_bytes(&shape(), ctx, KvCacheType::Q4_0),
            elements * 18 / 32
        );
        // and the ladder is strictly decreasing.
        let f16 = kv_cache_bytes(&shape(), ctx, KvCacheType::F16);
        let q8 = kv_cache_bytes(&shape(), ctx, KvCacheType::Q8_0);
        let q5 = kv_cache_bytes(&shape(), ctx, KvCacheType::Q5_1);
        let q4 = kv_cache_bytes(&shape(), ctx, KvCacheType::Q4_0);
        assert!(f16 > q8 && q8 > q5 && q5 > q4);
        assert!(q4 * 100 < f16 * 35, "Q4 should keep at most ~35% of f16");
    }

    // --- checks: context handling --------------------------------------------

    #[test]
    fn context_zero_falls_back_to_the_declared_context() {
        let mut declared = shape();
        declared.max_context = 40_960;
        let mut profile = InferenceProfile::desktop_default();
        profile.context_tokens = 0;
        assert_eq!(effective_context(&profile, &declared), 40_960);
        // Without a declared length the practical default applies.
        assert_eq!(effective_context(&profile, &shape()), 8_192);
        // An explicit profile context always wins.
        let mut explicit = profile.clone();
        explicit.context_tokens = 1_024;
        assert_eq!(effective_context(&explicit, &declared), 1_024);
        // and the KV size for a 0-context profile reflects that fallback.
        let m = split(&declared, &profile);
        assert_eq!(
            m.ram_bytes,
            declared.file_bytes + kv_cache_bytes(&declared, 40_960, KvCacheType::F16)
        );
    }

    // --- checks: thinking budget ---------------------------------------------

    #[test]
    fn thinking_budget_is_counted_outside_the_kv_cache() {
        // The reasoning budget caps generation tokens; the KV cache is sized
        // by the context window alone, whatever the budget says.
        let mut budgeted = shape();
        budgeted.max_context = 8_192;
        let mut profile = InferenceProfile::desktop_default();
        profile.thinking_budget = 32_768;
        assert_eq!(
            kv_cache_bytes(&budgeted, effective_context(&profile, &budgeted), profile.kv_type),
            kv_cache_bytes(&budgeted, 8_192, KvCacheType::F16)
        );
    }

    #[test]
    fn moe_recommendation_wins_over_ram_only_when_it_fits() {
        // Same split as moe_hybrid_keeps_experts_that_do_not_fit but with a
        // card big enough that no expert has to be parked in RAM.
        let mut moe_shape = shape();
        moe_shape.file_bytes = 7 * GIB;
        moe_shape.moe = Some(MoeShape {
            n_experts: 8,
            n_experts_used: 2,
            expert_bytes_total: 6 * GIB,
        });
        let gpu = GpuCaps {
            vram_bytes: 16 * GIB,
        };
        let base = InferenceProfile::desktop_default();
        let profile = recommend_moe_hybrid(&moe_shape, &host(24), &gpu, &base)
            .expect("16 GB card fits a 7 GB model outright");
        assert_eq!(profile.moe, MoeOffload::Off, "no experts parked in RAM");
        let m = split(&moe_shape, &profile);
        assert_eq!(m.ram_bytes, 0);
    }

    #[test]
    fn moe_hybrid_keeps_experts_that_do_not_fit() {
        // Dense part is small (1 GB), experts are 6 GB, VRAM is 4 GB, so the
        // recommender must park part of the expert set in RAM.
        let mut moe_shape = shape();
        moe_shape.file_bytes = 7 * GIB;
        moe_shape.moe = Some(MoeShape {
            n_experts: 8,
            n_experts_used: 2,
            expert_bytes_total: 6 * GIB,
        });
        let gpu = GpuCaps {
            vram_bytes: 4 * GIB,
        };
        let base = InferenceProfile::desktop_default();
        let profile =
            recommend_moe_hybrid(&moe_shape, &host(24), &gpu, &base).expect("should find a split");
        let m = split(&moe_shape, &profile);
        assert!(m.vram_bytes <= gpu.vram_bytes - VRAM_RESERVE_BYTES);
        assert!(m.ram_bytes > 0, "some experts must sit in RAM");
        assert!(profile.device.gpu_layers() > 0);
    }
}
