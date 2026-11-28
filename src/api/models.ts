/** Model manager and runtime APIs.
 *
 * Wraps the Rust commands in `src-tauri/src/commands/models.rs`. The user
 * supplies their own GGUF files through registered folders; there is no
 * download pipeline.
 */

import { delay, invoke, isTauri } from "./index";
import type {
  ComputeDevice,
  FitLevel,
  InferenceProfile,
  KvCacheType,
  ProfileEstimate,
  ProfileScope,
  ProfileStorage,
  RuntimeStatus,
  ServerCapabilities,
  ValidationMessage,
  ValidationReport,
} from "../types";

export interface HardwareReport {
  totalRamGb: number;
  /** RAM that is free right now. */
  availableRamGb: number;
  cpuName?: string;
  cpuCores: number;
  cpuPhysicalCores?: number;
  cpuAvx2?: boolean;
  cpuAvx512?: boolean;
  gpuName?: string;
  gpuBackend?: string;
  /** Total VRAM of the detected adapter; null when it could not be read. */
  gpuVramGb?: number | null;
  /** True when the machine has a graphics card the app could use. */
  gpuAvailable: boolean;
  recommendedTier: string;
}

/** What a GGUF file is. */
export type ModelFileKind = "model" | "projector";

/** One GGUF file on disk. */
export interface ModelEntry {
  name: string;
  path: string;
  sizeBytes: number;
  /** True for files in the app's models folder (removable); false for files the user keeps in their own GGUF folders. */
  internal: boolean;
  /** The folder the file lives in. */
  dir: string;
  kind: ModelFileKind;
  /** Parameter count in billions, 0 when the name does not say. */
  paramsBillion: number;
  /** Quantization label such as "Q4_K_M"; empty when unknown. */
  quantization: string;
  /** GGUF architecture such as "llama"; empty when unknown. */
  architecture: string;
  /** True when the app stores a configuration of its own for this file. */
  hasProfile: boolean;
}

/**
 * A model and everything that belongs to it.
 *
 * A multimodal projector lives in the same folder as the weights it belongs
 * to, so the two are kept together.
 */
export interface ModelGroup {
  /** Stable identifier for the group — the folder it lives in. */
  id: string;
  name: string;
  dir: string;
  internal: boolean;
  /** The weight files in the folder. The first one is the primary model. */
  models: ModelEntry[];
  /** Projector files that belong to the weights. */
  projectors: ModelEntry[];
  totalBytes: number;
}

// ---------------------------------------------------------------------------
// Installed models and model folders
// ---------------------------------------------------------------------------

/** Marks the models the app stores a configuration of its own for. */
function markProfiles(groups: ModelGroup[]): ModelGroup[] {
  for (const group of groups) {
    for (const entry of [...group.models, ...group.projectors]) {
      entry.hasProfile = mockModelProfiles[entry.path] != null;
    }
  }
  return groups;
}

/** The models on this machine, grouped by the folder they live in. */
export async function listModelGroups(): Promise<ModelGroup[]> {
  if (isTauri) return invoke<ModelGroup[]>("list_available_models");
  await delay(140);
  return markProfiles(mockGroups);
}

/** The folders the user keeps GGUF files in, outside the app's models folder. */
export async function listModelFolders(): Promise<string[]> {
  if (isTauri) return invoke<string[]>("list_model_folders");
  await delay(60);
  return mockFolders;
}

/** Registers a folder path to scan for GGUF models. */
export async function addModelFolder(folder: string): Promise<void> {
  if (isTauri) return invoke("add_model_folder", { folder });
  await delay(90);
  mockFolders.push(folder);
  const path = `${folder}\\granite-4.1-8b-instruct-q4_k_m.gguf`;
  MOCK_SHAPES[path] = {
    fileBytes: 5_100_000_000,
    nLayers: 36,
    nKvHeads: 8,
    headDim: 128,
    maxContext: 32_768,
  };
  const group: ModelGroup = {
    id: folder,
    name: "granite 4.1 8b instruct q4 k m",
    dir: folder,
    internal: false,
    models: [
      {
        name: "granite 4.1 8b instruct q4 k m",
        path,
        sizeBytes: 5_100_000_000,
        internal: false,
        dir: folder,
        kind: "model",
        paramsBillion: 8,
        quantization: "Q4_K_M",
        architecture: "granite",
        hasProfile: false,
      },
    ],
    projectors: [],
    totalBytes: 5_100_000_000,
  };
  if (!mockGroups.some((entry) => entry.id === group.id)) mockGroups.push(group);
}

/** Stops scanning a folder — the files inside it are left untouched. */
export async function removeModelFolder(folder: string): Promise<void> {
  if (isTauri) return invoke("remove_model_folder", { folder });
  await delay(90);
  mockFolders.splice(
    mockFolders.findIndex((entry) => entry === folder),
    1,
  );
  mockGroups = mockGroups.filter((group) => group.dir !== folder);
}

/** Opens an OS folder picker; a picked folder is registered immediately. */
export async function pickModelFolder(): Promise<string | null> {
  if (isTauri) return invoke<string | null>("pick_model_folder");
  await delay(120);
  return "C:\\models\\custom";
}

/** Sets the model chat loads, or clears it with an empty path. */
export async function selectModel(modelPath: string): Promise<void> {
  if (isTauri) return invoke("select_model", { modelPath });
  await delay(90);
  mockSelectedPath = modelPath;
}

/** The path chat currently loads, so the UI can mark the active model. */
export async function getSelectedModel(): Promise<string | null> {
  if (isTauri) return invoke<string | null>("get_selected_model");
  await delay(60);
  return mockSelectedPath;
}

export async function deleteModel(modelPath: string): Promise<void> {
  if (isTauri) return invoke("delete_model", { modelPath });
  await delay(90);
  mockGroups = mockGroups
    .map((group) => ({
      ...group,
      models: group.models.filter((entry) => entry.path !== modelPath),
      projectors: group.projectors.filter((entry) => entry.path !== modelPath),
    }))
    .filter((group) => group.models.length > 0 || group.projectors.length > 0);
}

// ---------------------------------------------------------------------------
// Hardware, runtime status, profile
// ---------------------------------------------------------------------------

export async function getHardwareReport(): Promise<HardwareReport> {
  if (isTauri) return invoke<HardwareReport>("get_hardware_report");
  await delay(180);
  return {
    totalRamGb: 16,
    availableRamGb: 9.4,
    cpuCores: 12,
    cpuPhysicalCores: 8,
    cpuAvx2: true,
    cpuAvx512: false,
    gpuName: "None detected",
    gpuBackend: "cpu",
    gpuVramGb: null,
    gpuAvailable: false,
    recommendedTier: "Medium (7–8B, Q4_K_M)",
  };
}

export async function runtimeStatus(): Promise<RuntimeStatus> {
  if (isTauri) return invoke<RuntimeStatus>("runtime_status");
  await delay(120);
  return {
    model: MOCK_RUNNING.model,
    state: "ready",
    port: 51234,
    profile: MOCK_RUNNING.profile,
    device: MOCK_RUNNING.device,
    error: null,
  };
}

/**
 * The profiles the app has stored, and the one that is in effect.
 *
 * `threads: 0` still means "auto" — the UI shows the resolved count beside
 * the switch rather than baking it into the stored profile.
 */
export async function runtimeProfile(
  modelPath?: string | null,
  projectId?: string | null,
): Promise<ProfileStorage> {
  if (isTauri) {
    return invoke<ProfileStorage>("get_runtime_profile", { modelPath, projectId });
  }
  await delay(120);
  return mockStorage(modelPath, projectId);
}

/** What the llama-server build in use can do. */
export async function runtimeCapabilities(): Promise<ServerCapabilities> {
  if (isTauri) return invoke<ServerCapabilities>("get_runtime_capabilities");
  await delay(120);
  return { supportsGpu: false, devices: [], probed: true, flashAttn: true };
}

/**
 * Checks a draft against the model and the machine. Nothing is changed; the
 * findings come back so the UI can show them next to the controls.
 */
export async function validateProfile(
  profile: InferenceProfile,
  scope: ProfileScope = "global",
  modelPath?: string | null,
  projectId?: string | null,
): Promise<ValidationReport> {
  if (isTauri) {
    return invoke<ValidationReport>("validate_runtime_profile", {
      profile,
      scope,
      modelPath,
      projectId,
    });
  }
  await delay(140);
  return mockEstimate(profile, modelPath).report;
}

/**
 * Validates a draft and, when nothing is wrong with it, persists it.
 *
 * The profile is stored in the scope it belongs to — the app-wide default,
 * one model or one project. The running server is stopped when the profile
 * that is actually in effect changes, so the new flags take effect on the
 * next chat turn.
 */
export async function applyProfile(
  profile: InferenceProfile,
  scope: ProfileScope = "global",
  modelPath?: string | null,
  projectId?: string | null,
): Promise<ValidationReport> {
  if (isTauri) {
    return invoke<ValidationReport>("apply_runtime_profile", {
      profile,
      scope,
      modelPath,
      projectId,
    });
  }
  await delay(160);
  const report = mockEstimate(profile, modelPath).report;
  if (report.messages.some((message) => message.severity === "error")) {
    throw new Error(
      report.messages
        .filter((message) => message.severity === "error")
        .map((message) => `${message.title}: ${message.detail}`)
        .join(" "),
    );
  }
  mockStore(scope, modelPath, projectId, profile);
  return report;
}

/**
 * Removes the profile stored in a scope, so the scope below it applies
 * again. The app-wide default falls back to the app's own default.
 */
export async function resetProfile(
  scope: ProfileScope = "global",
  modelPath?: string | null,
  projectId?: string | null,
): Promise<ProfileStorage> {
  if (isTauri) {
    return invoke<ProfileStorage>("reset_runtime_profile", { scope, modelPath, projectId });
  }
  await delay(140);
  mockClear(scope, modelPath, projectId);
  return mockStorage(modelPath, projectId);
}

/**
 * Memory estimate for a profile against the selected model. The profile is
 * the on-screen draft, so the estimate follows the controls live; omit it to
 * estimate the stored profile.
 */
export async function profileEstimate(
  profile?: InferenceProfile | null,
  scope: ProfileScope = "global",
  modelPath?: string | null,
  projectId?: string | null,
): Promise<ProfileEstimate> {
  if (isTauri) {
    return invoke<ProfileEstimate>("get_profile_estimate", {
      profile,
      scope,
      modelPath,
      projectId,
    });
  }
  await delay(200);
  return mockEstimate(profile ?? mockSavedProfile, modelPath);
}

/** The explicitly chosen llama-server build, if the user picked one. */
export async function getServerBinary(): Promise<string | null> {
  if (isTauri) return invoke<string | null>("get_server_binary");
  await delay(60);
  return null;
}

/** Opens a dialog to choose a llama-server build; null when cancelled. */
export async function pickServerBinary(): Promise<string | null> {
  if (isTauri) return invoke<string | null>("pick_server_binary");
  await delay(120);
  return null;
}

/** What one llama.cpp GPU backend build looks like on this machine. */
export interface GpuBuildStatus {
  /** "vulkan", "cuda" or "opencl". */
  target: string;
  label: string;
  installed: boolean;
  /** What an in-app install recorded, when it is one. */
  source: {
    releaseTag: string;
    nightlyTag: string;
    archives: string[];
    digests: string[];
    installedAt: string;
  } | null;
  /** True when the app can download this backend itself. */
  downloadable: boolean;
  directory: string;
  /** True when this backend matches the detected graphics card. */
  recommended: boolean;
}

/** The machine's GPU story as far as the build installer is concerned. */
export interface GpuBuildList {
  gpuName: string;
  /** "CUDA", "Vulkan" or "CPU only" — the detection's own word. */
  gpuBackend: string;
  gpuAvailable: boolean;
  /** The backend that matches the detected card, e.g. "cuda". */
  recommended: string | null;
  builds: GpuBuildStatus[];
}

/** The GPU backend builds the app knows about, and what to install next. */
export async function listGpuBuilds(): Promise<GpuBuildList> {
  if (isTauri) return invoke<GpuBuildList>("list_gpu_builds");
  await delay(80);
  return {
    gpuName: "NVIDIA GeForce RTX 4070 (mock)",
    gpuBackend: "CUDA",
    gpuAvailable: true,
    recommended: "cuda",
    builds: [
      { target: "cuda", label: "CUDA", installed: false, source: null, downloadable: true, directory: "", recommended: true },
      { target: "vulkan", label: "Vulkan", installed: false, source: null, downloadable: true, directory: "", recommended: false },
      { target: "opencl", label: "OpenCL", installed: false, source: null, downloadable: false, directory: "", recommended: false },
    ],
  };
}

/**
 * Downloads and installs an official prebuilt llama.cpp build of `target`.
 *
 * Reports progress through the `localAIse/gpu-build` event and returns the
 * path of the installed launcher when it is done.
 */
export async function installGpuBuild(target: string): Promise<string> {
  if (isTauri) return invoke<string>("install_gpu_build", { target });
  await delay(1200);
  return "";
}

// ---------------------------------------------------------------------------
// Mock fixtures (browser dev mode only)
//
// The fixtures mirror what the Rust core answers: the same KV cache
// arithmetic, the same budgets and the same findings. That way the UI can be
// worked on in the browser without the numbers telling a different story
// than the app will tell later.
// ---------------------------------------------------------------------------
export const DEFAULT_PROFILE: InferenceProfile = {
  contextTokens: 8_192,
  threads: 0,
  device: { device: "auto" },
  kvType: "f16",
  flashAttn: false,
  batchSize: 512,
  parallelSlots: 1,
  useMmap: true,
  moe: { mode: "off" },
  thinkingBudget: 0,
  partialLoad: false,
};

const GIB = 1024 ** 3;

/** RAM kept free for the OS, the UI and the app itself. */
const RAM_RESERVE = 1.5 * GIB;
/** Working memory llama.cpp needs on top of the weights and the cache. */
const RUNTIME_OVERHEAD = 0.5 * GIB;

const MOCK_RAM_TOTAL = 16 * GIB;
const MOCK_RAM_AVAILABLE = 9.4 * GIB;
const MOCK_VRAM_TOTAL = 8.0 * GIB;
const VRAM_RESERVE = 0.8 * GIB;
const MOCK_CORES = 12;

/** Bytes per cached value, including the block-scale overhead. */
const KV_BYTES: Record<KvCacheType, number> = {
  f16: 2,
  q8_0: 34 / 32,
  q5_1: 24 / 32,
  q4_0: 18 / 32,
};

/** The shape of a model file, as the Rust core reads it out of the GGUF. */
interface MockShape {
  fileBytes: number;
  nLayers: number;
  nKvHeads: number;
  headDim: number;
  maxContext: number;
}

const LLAMA_8B_PATH =
  "C:\\models\\llm\\llama-3.1-8b-instruct\\Llama-3.1-8B-Instruct-Q4_K_M.gguf";
const QWEN_VL_PATH = "C:\\models\\llm\\qwen2.5-vl-7b-instruct\\Qwen2.5-VL-7B-Instruct-Q4_K_M.gguf";

const MOCK_SHAPES: Record<string, MockShape> = {
  [LLAMA_8B_PATH]: {
    fileBytes: 4_726_000_000,
    nLayers: 32,
    nKvHeads: 8,
    headDim: 128,
    maxContext: 131_072,
  },
  [QWEN_VL_PATH]: {
    fileBytes: 5_100_000_000,
    nLayers: 28,
    nKvHeads: 4,
    headDim: 128,
    maxContext: 32_768,
  },
};

let mockGroups: ModelGroup[] = [
  {
    id: "C:\\models\\llm\\llama-3.1-8b-instruct",    name: "llama 3.1 8b instruct",
    dir: "C:\\models\\llm\\llama-3.1-8b-instruct",
    internal: true,
    models: [
      {
        name: "Llama-3.1-8B-Instruct-Q4_K_M",
        path: LLAMA_8B_PATH,
        sizeBytes: 4_726_000_000,
        internal: true,
        dir: "C:\\models\\llm\\llama-3.1-8b-instruct",
        kind: "model",
        paramsBillion: 8,
        quantization: "Q4_K_M",
        architecture: "llama",
        hasProfile: false,
      },
    ],
    projectors: [],
    totalBytes: 4_726_000_000,
  },
  {
    id: "C:\\models\\llm\\qwen2.5-vl-7b-instruct",
    name: "qwen2.5 vl 7b instruct",
    dir: "C:\\models\\llm\\qwen2.5-vl-7b-instruct",
    internal: true,
    models: [
      {
        name: "Qwen2.5-VL-7B-Instruct-Q4_K_M",
        path: QWEN_VL_PATH,
        sizeBytes: 5_100_000_000,
        internal: true,
        dir: "C:\\models\\llm\\qwen2.5-vl-7b-instruct",
        kind: "model",
        paramsBillion: 7,
        quantization: "Q4_K_M",
        architecture: "qwen2vl",
        hasProfile: false,
      },
    ],
    projectors: [
      {
        name: "mmproj-Qwen2.5-VL-7B-Instruct-f16",
        path:
          "C:\\models\\llm\\qwen2.5-vl-7b-instruct\\mmproj-Qwen2.5-VL-7B-Instruct-f16.gguf",
        sizeBytes: 1_200_000_000,
        internal: true,
        dir: "C:\\models\\llm\\qwen2.5-vl-7b-instruct",
        kind: "projector",
        paramsBillion: 0,
        quantization: "F16",
        architecture: "clip",
        hasProfile: false,
      },
    ],
    totalBytes: 6_300_000_000,
  },
];

let mockFolders: string[] = [];

let mockSelectedPath: string | null = LLAMA_8B_PATH;

/** The profile as it is stored. */
let mockSavedProfile: InferenceProfile = { ...DEFAULT_PROFILE };

/** Profiles stored for a single model, keyed by the model's path. */
const mockModelProfiles: Record<string, InferenceProfile> = {};

/** Profiles stored for a single project, keyed by the project id. */
const mockProjectProfiles: Record<string, InferenceProfile> = {};

/** The profiles the app holds for a model and a project. */
function mockStorage(
  modelPath?: string | null,
  projectId?: string | null,
): ProfileStorage {
  const model = modelPath ?? mockSelectedPath ?? null;
  const modelProfile = model ? (mockModelProfiles[model] ?? null) : null;
  const projectProfile = projectId ? (mockProjectProfiles[projectId] ?? null) : null;
  const effective = projectProfile ?? modelProfile ?? mockSavedProfile;
  return {
    effective,
    effectiveScope: projectProfile ? "project" : modelProfile ? "model" : "global",
    global: mockSavedProfile,
    model: modelProfile,
    project: projectProfile,
  };
}

/** Stores a profile in a scope. */
function mockStore(
  scope: ProfileScope,
  modelPath: string | null | undefined,
  projectId: string | null | undefined,
  profile: InferenceProfile,
): void {
  if (scope === "global") {
    mockSavedProfile = profile;
    return;
  }
  if (scope === "model") {
    const model = modelPath ?? mockSelectedPath;
    if (model) mockModelProfiles[model] = profile;
    return;
  }
  if (projectId) mockProjectProfiles[projectId] = profile;
}

/** Removes the profile stored in a scope. */
function mockClear(
  scope: ProfileScope,
  modelPath: string | null | undefined,
  projectId: string | null | undefined,
): void {
  if (scope === "global") {
    mockSavedProfile = { ...DEFAULT_PROFILE };
    return;
  }
  if (scope === "model") {
    const model = modelPath ?? mockSelectedPath;
    if (model) delete mockModelProfiles[model];
    return;
  }
  if (projectId) delete mockProjectProfiles[projectId];
}

/** What the running server was started with: the profile, resolved. */
const MOCK_RUNNING: {
  model: string;
  profile: InferenceProfile;
  device: ComputeDevice;
} = {
  model: "Llama 3.1 8B Instruct",
  profile: {
    ...DEFAULT_PROFILE,
    threads: MOCK_CORES - 1,
    device: { device: "cpu" },
  },
  device: { device: "cpu" },
};

/** K and V for every layer, for `ctx` tokens. */
function mockKvBytes(shape: MockShape, ctx: number, kv: KvCacheType): number {
  const elements = ctx * shape.nLayers * shape.nKvHeads * shape.headDim * 2;
  return Math.floor(elements * KV_BYTES[kv]);
}

/** Classifies how much of a budget a configuration uses. */
function mockFit(needed: number, budget: number): FitLevel {
  if (budget <= 0) return "exceeds";
  if (needed > budget) return "exceeds";
  if (needed * 100 > budget * 85) return "tight";
  return "comfortable";
}

/** Whether two resolved profiles ask the server for the same thing. */
function sameResolved(a: InferenceProfile, b: InferenceProfile): boolean {
  return (
    a.contextTokens === b.contextTokens &&
    a.threads === b.threads &&
    a.kvType === b.kvType &&
    a.flashAttn === b.flashAttn &&
    a.batchSize === b.batchSize &&
    a.parallelSlots === b.parallelSlots &&
    a.useMmap === b.useMmap &&
    a.thinkingBudget === b.thinkingBudget &&
    a.partialLoad === b.partialLoad &&
    a.device.device === b.device.device &&
    a.moe.mode === b.moe.mode
  );
}

/** The largest context that fits in RAM, with the best cache that fits. */
function mockRecommend(
  shape: MockShape | null,
  wanted: number,
  ramBudget: number,
): InferenceProfile | null {
  if (!shape) return null;
  let ctx = Math.max(wanted, 8_192);
  while (ctx >= 2_048) {
    for (const kv of ["f16", "q8_0", "q5_1", "q4_0"] as KvCacheType[]) {
      const usage = shape.fileBytes + mockKvBytes(shape, ctx, kv);
      if (usage <= ramBudget) {
        return {
          contextTokens: ctx,
          threads: MOCK_CORES - 1,
          device: { device: "auto" },
          kvType: kv,
          flashAttn: kv !== "f16",
          batchSize: 512,
          parallelSlots: 1,
          useMmap: true,
          moe: { mode: "off" },
          thinkingBudget: 0,
          partialLoad: false,
        };
      }
    }
    ctx = Math.floor(ctx / 2);
  }
  return null;
}

/** The findings the core would report for this draft. */
function mockFindings(
  draft: InferenceProfile,
  shape: MockShape | null,
  device: ComputeDevice,
  fit: FitLevel,
  needed: number,
  budget: number,
): ValidationMessage[] {
  const messages: ValidationMessage[] = [];

  if (draft.device.device === "auto") {
    messages.push({
      code: "auto-cpu-build",
      severity: "info",
      title: "Automatic device: CPU",
      detail:
        "The llama-server build in use has no GPU support. Point the app at a GPU build of " +
        "llama.cpp to use the graphics card.",
    });
  } else if (draft.device.device === "cpu") {
    messages.push({
      code: "cpu-only",
      severity: "info",
      title: "CPU only",
      detail: "No VRAM is used: the weights and the cache stay in system RAM.",
    });
  } else {
    messages.push({
      code: "gpu-no-build",
      severity: "error",
      title: "GPU mode needs a llama-server build",
      detail:
        "Pick a llama-server build with GPU support in the hardware settings. The bundled build " +
        "is CPU only.",
    });
  }

  const maxContext = shape?.maxContext ?? 0;
  if (draft.contextTokens === 0) {
    messages.push({
      code: "context-model-default",
      severity: "info",
      title: "The model's own context length is used",
      detail: maxContext
        ? `The file declares ${maxContext.toLocaleString()} tokens.`
        : "The file does not declare a context length, so llama-server picks one.",
    });
  } else if (maxContext > 0 && draft.contextTokens > maxContext) {
    messages.push({
      code: "context-above-model",
      severity: "warning",
      title: "Larger than the model declares",
      detail:
        `The file declares ${maxContext.toLocaleString()} tokens. A longer window does not give ` +
        "the model more room to look at — the runtime cuts it back to what the model supports.",
    });
  }

  const onGpu = device.device === "user_gpu";
  if (fit === "exceeds") {
    if (onGpu) {
      messages.push({
        code: "vram-exceeds",
        severity: "error",
        title: "Estimated VRAM exceeds physical GPU memory",
        detail:
          `The weights and KV cache need about ${(needed / GIB).toFixed(1)} GB VRAM, ` +
          `exceeding the ${(MOCK_VRAM_TOTAL / GIB).toFixed(1)} GB physical VRAM available on the graphics adapter. ` +
          "Lower the context window, switch to a quantized KV cache (Q4 or Q8), or reduce offloaded GPU layers.",
      });
    } else {
      messages.push({
        code: "memory-exceeds",
        severity: "warning",
        title: "Estimated memory exceeds available physical RAM",
        detail:
          `The weights, KV cache, and runtime overhead need about ${(needed / GIB).toFixed(1)} GB RAM, ` +
          `which exceeds the ${(MOCK_RAM_AVAILABLE / GIB).toFixed(1)} GB currently available and approaches/exceeds ${(MOCK_RAM_TOTAL / GIB).toFixed(1)} GB physical RAM. ` +
          "Lower the context window, pick a smaller KV cache type (Q5 or Q4), or turn on partial loading so llama-server shrinks the context to fit.",
      });
    }
  } else if (fit === "tight") {
    messages.push({
      code: "memory-tight",
      severity: "warning",
      title: "Tight memory budget",
      detail:
        `The weights, cache, and runtime need about ${(needed / GIB).toFixed(1)} GB of ` +
        `the ${(budget / GIB).toFixed(1)} GB available for inference. It fits, but with very little safety margin for multitasking.`,
    });
  }

  if (draft.kvType === "f16" && draft.flashAttn) {
    messages.push({
      code: "flash-unused",
      severity: "info",
      title: "Flash attention without a quantized cache",
      detail:
        "Flash attention pays off with a quantized cache. With F16 the cache stays full " +
        "precision, so the setting changes nothing but the memory layout.",
    });
  }

  const window = draft.contextTokens > 0 ? draft.contextTokens : maxContext;
  if (draft.thinkingBudget > 0 && window > 0 && draft.thinkingBudget >= window) {
    messages.push({
      code: "thinking-above-context",
      severity: "warning",
      title: "The thinking budget is larger than the context",
      detail:
        `The budget of ${draft.thinkingBudget.toLocaleString()} tokens does not fit in a window ` +
        `of ${window.toLocaleString()} tokens, so the model runs out of room before it can spend it.`,
    });
  }

  return messages;
}

/** The estimate for a draft, worked out the way the core does it. */
function mockEstimate(draft: InferenceProfile, modelPath?: string | null): ProfileEstimate {
  const shape = MOCK_SHAPES[modelPath ?? mockSelectedPath ?? ""] ?? null;
  const fileBytes = shape?.fileBytes ?? 0;
  const maxContext = shape?.maxContext ?? 0;

  // The bundled build has no GPU, so an automatic device resolves to the CPU.
  const device: ComputeDevice =
    draft.device.device === "auto" ? { device: "cpu" } : draft.device;
  const resolved: InferenceProfile = {
    ...draft,
    device,
    threads: draft.threads > 0 ? draft.threads : MOCK_CORES - 1,
  };

  const contextTokens =
    draft.contextTokens > 0 ? draft.contextTokens : Math.max(maxContext, 8_192);
  const kvBytes = shape ? mockKvBytes(shape, contextTokens, draft.kvType) : 0;
  // The same window priced under every cache type, mirroring the core.
  const kvCacheGbByType: Partial<Record<KvCacheType, number>> = shape
    ? Object.fromEntries(
        (["f16", "q8_0", "q5_1", "q4_0"] as KvCacheType[]).map((kv) => [
          kv,
          mockKvBytes(shape, contextTokens, kv) / GIB,
        ]),
      )
    : {};
  const onGpu = device.device === "user_gpu";
  const ramBytes = onGpu ? 0 : fileBytes + kvBytes;
  const vramBytes = onGpu ? fileBytes + kvBytes : 0;

  const ramBudget = MOCK_RAM_TOTAL - RAM_RESERVE;
  const vramBudget = MOCK_VRAM_TOTAL - VRAM_RESERVE;

  const fit: FitLevel = onGpu
    ? mockFit(vramBytes, vramBudget)
    : mockFit(ramBytes + RUNTIME_OVERHEAD, ramBudget);
  const needed = onGpu ? vramBytes : ramBytes + RUNTIME_OVERHEAD;
  const effectiveBudget = onGpu ? vramBudget : ramBudget;

  const messages = mockFindings(draft, shape, device, fit, needed, effectiveBudget);
  const running = MOCK_RUNNING.profile;
  const requiresRestart = !sameResolved(resolved, running);

  return {
    profile: draft,
    resolvedProfile: resolved,
    device,
    modelShapeKnown: shape !== null,
    modelFileGb: fileBytes / GIB,
    contextTokens,
    maxContext,
    kvCacheGb: kvBytes / GIB,
    kvCacheGbByType,
    runtimeOverheadGb: RUNTIME_OVERHEAD / GIB,
    ramNeededGb: (onGpu ? RUNTIME_OVERHEAD : ramBytes + RUNTIME_OVERHEAD) / GIB,
    ramTotalGb: MOCK_RAM_TOTAL / GIB,
    ramAvailableGb: MOCK_RAM_AVAILABLE / GIB,
    ramBudgetGb: ramBudget / GIB,
    vramNeededGb: vramBytes / GIB,
    vramTotalGb: MOCK_VRAM_TOTAL / GIB,
    vramBudgetGb: vramBudget / GIB,
    vramKnown: true,
    fit,
    isMoe: false,
    recommended: mockRecommend(shape, draft.contextTokens, ramBudget),
    report: { messages, requiresRestart },
  };
}
