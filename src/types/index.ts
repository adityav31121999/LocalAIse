/** Shared UI types. Mirrors the shapes returned by the Rust commands. */

/** Color mode — the independent light/dark setting. */
export type ThemeMode = "light" | "dark" | "system";

/** Theme — the visual skin. Orthogonal to the color mode. */
export type ThemeName = "atelier" | "paper" | "cyber-lab" | "amethyst" | "arctic" | "evergreen";

/** Interface density — how much breathing room the shared component system gets. */
export type Density = "compact" | "comfortable" | "spacious";

export type ProjectTemplate = "budget" | "content-writing" | "research" | "general";

/** How the assistant answers in this project's chat: plain chat or agent. */
export type ChatMode = "chat" | "agent";

export type SourceStatus = "indexed" | "indexing" | "pending" | "error";

export interface Project {
  id: string;
  name: string;
  path: string;
  template: ProjectTemplate;
  sourceCount: number;
  internetEnabled: boolean;
  /** Custom chat system prompt; null means the built-in default is in use. */
  systemPrompt: string | null;
  /** The assistant this project's chat uses: plain chat or the tool-using agent. */
  chatMode: ChatMode;
  /** Whether the project keeps its own diary/ folder (one page per day). */
  diaryEnabled: boolean;
  /** Folders the user added to the project directory. */
  customFolders: string[];
  model: string;
  lastOpenedAt: string;
}

export interface SourceFile {
  id: string;
  name: string;
  status: SourceStatus;
  sizeBytes: number;
  modifiedAt: string;
}

/** One item in the project's directory view. `path` is relative to the project root. */
export interface DirectoryEntry {
  name: string;
  path: string;
  isDir: boolean;
  sizeBytes: number;
  modifiedAt: string;
}

export interface Citation {
  index: number;
  fileName: string;
  page?: number;
  passage?: string;
}

export type ChatRole = "user" | "assistant";

export interface ChatMessage {
  id: string;
  role: ChatRole;
  content: string;
  /** The model's reasoning trace, when the runtime exposed one. */
  reasoning?: string;
  createdAt: string;
  citations?: Citation[];
  pending?: boolean;
}

export interface ChatStreamResult {
  /** The session the turn was saved in; a new chat receives its id here. */
  sessionId: string;
  title: string;
  content: string;
  /** Reasoning the model produced before answering, if any. */
  reasoning?: string;
  citations: Citation[];
}

/** One saved chat session, as listed in the chats/ folder. */
export interface ChatSummary {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
  messageCount: number;
}

/** A stored message, as written to the session's JSON file. */
export interface StoredChatMessage {
  id: string;
  role: ChatRole;
  content: string;
  reasoning?: string;
  createdAt: string;
  citations?: Citation[];
}

/** A full saved conversation. */
export interface ChatSession {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
  messages: StoredChatMessage[];
}

export interface ActivityEvent {
  id: string;
  at: string;
  kind: "local" | "network";
  tool: string;
  summary: string;
}

export type RuntimeState = "starting" | "ready" | "stopped" | "error";

export interface RuntimeStatus {
  model: string;
  state: RuntimeState;
  port: number | null;
  /** The profile the running server was started with. */
  profile: InferenceProfile | null;
  /** The device the running server actually uses. */
  device: ComputeDevice | null;
  /** Why the last launch failed, when it did. */
  error: string | null;
}

export type WorkspaceTab = "chat" | "files" | "calendar" | "analysis" | "research" | "diary" | "activity";

/** A diary entry. `date` is "YYYY-MM-DD" for a dated entry, or null for a page the user added. */
export interface DiaryPage {
  id: string;
  date: string | null;
  title: string;
  body: string;
  createdAt: string;
  updatedAt: string;
  folderId?: string;
  folderName?: string;
  filePath?: string;
  isHidden?: boolean;
}

export interface PersonalFolderRecord {
  id: string;
  name: string;
  path: string;
  isDir: boolean;
  isHidden: boolean;
  isDefault: boolean;
  type: "calendar_pages" | "user_folder";
  description: string;
  createdAt: string;
  updatedAt: string;
}


export interface CalendarEvent {
  id: string;
  title: string;
  description?: string;
  startAt: string;
  endAt?: string;
  location?: string;
}

export interface PersonalSearchResult {
  kind: "diary" | "event";
  id: string;
  title: string;
  date?: string;
  snippet: string;
}

export type AppView = "home" | "diary" | "settings" | "theme-settings" | "model-config" | "deep-research";

/** Inference profile — mirrors the Rust InferenceProfile struct. */
export type KvCacheType = "f16" | "q8_0" | "q5_1" | "q4_0";

/**
 * Compute device placement.
 *
 * `auto` is the default: the app picks the GPU when this machine has one and
 * the llama-server build in use can offload to it, and the CPU otherwise.
 * `cpu` never allocates VRAM.
 */
export type ComputeDevice =
  | { device: "auto" }
  | { device: "cpu" }
  | { device: "user_gpu"; gpu_layers: number };

/** MoE expert placement. Only meaningful when layers run on the GPU. */
export type MoeOffload =
  | { mode: "off" }
  | { mode: "all_experts" }
  | { mode: "first_layers"; n: number };

export interface InferenceProfile {
  contextTokens: number;
  /** 0 means auto; the UI shows the resolved value. */
  threads: number;
  device: ComputeDevice;
  kvType: KvCacheType;
  flashAttn: boolean;
  batchSize: number;
  parallelSlots: number;
  useMmap: boolean;
  moe: MoeOffload;
  /** Cap on thinking tokens. 0 = auto: llama-server decides (unrestricted). */
  thinkingBudget: number;
  /** Let llama-server shrink the context to fit memory (--fit on). */
  partialLoad: boolean;

  // Token budgets per role. 0 = unlimited. Optional so an older core that
  // does not know the fields cannot break the settings page: missing values
  // read the defaults from `withProfileDefaults`.
  /** Cap on prompt tokens the model may be given. */
  maxInputTokens?: number;
  /** Cap on response tokens the model may produce. */
  maxOutputTokens?: number;
  /** Same input cap for the tool-using agent. */
  agentMaxInputTokens?: number;
  /** Same output cap for the agent. */
  agentMaxOutputTokens?: number;
  /** Thinking budget for the agent. 0 = auto. */
  agentThinkingBudget?: number;
  /** Input cap for subagent/map-reduce workers. */
  subagentMaxInputTokens?: number;
  /** Output cap for subagent/map-reduce workers. */
  subagentMaxOutputTokens?: number;
  /** Thinking budget for subagent workers. 0 = auto. */
  subagentThinkingBudget?: number;
}

/** Every token budget the settings page can edit, and its default. */
export const PROFILE_BUDGET_DEFAULTS: Record<
  | "maxInputTokens"
  | "maxOutputTokens"
  | "agentMaxInputTokens"
  | "agentMaxOutputTokens"
  | "agentThinkingBudget"
  | "subagentMaxInputTokens"
  | "subagentMaxOutputTokens"
  | "subagentThinkingBudget",
  number
> = {
  maxInputTokens: 0,
  maxOutputTokens: 0,
  agentMaxInputTokens: 0,
  agentMaxOutputTokens: 0,
  agentThinkingBudget: 0,
  subagentMaxInputTokens: 0,
  subagentMaxOutputTokens: 0,
  subagentThinkingBudget: 0,
};

/** Fills the optional token-budget fields a stored profile may not carry. */
export function withProfileDefaults<T extends Partial<InferenceProfile>>(
  profile: T,
): InferenceProfile & T {
  return { ...PROFILE_BUDGET_DEFAULTS, ...profile } as InferenceProfile & T;
}

/**
 * Which configuration a profile is stored for.
 *
 * The scopes stack: the global default is the base every model starts from,
 * a model profile replaces it for that one model, and a project profile
 * replaces both while that project is the one being worked in.
 */
export type ProfileScope = "global" | "model" | "project";

/** The profiles the app has stored, and the one that is in effect. */
export interface ProfileStorage {
  /** The profile that is in effect for the requested model and project. */
  effective: InferenceProfile;
  /** Which scope the effective profile came from. */
  effectiveScope: ProfileScope;
  /** The app-wide default every model starts from. */
  global: InferenceProfile;
  /** The profile stored for this model, when it has one. */
  model: InferenceProfile | null;
  /** The profile stored for this project, when it has one. */
  project: InferenceProfile | null;
}

/** How well a configuration fits into the memory a machine has. */
export type FitLevel = "comfortable" | "tight" | "exceeds";

/** How serious a validation finding is. */
export type Severity = "error" | "warning" | "info";

/** One finding about the configuration the user is editing. */
export interface ValidationMessage {
  /** Stable identifier, so the UI can react to a specific finding. */
  code: string;
  severity: Severity;
  /** Short headline, e.g. "May exceed available memory". */
  title: string;
  /** What it means and what to do about it. */
  detail: string;
}

/** Everything the validation found. */
export interface ValidationReport {
  messages: ValidationMessage[];
  /** True when the running server has to be restarted for the change. */
  requiresRestart: boolean;
}

export interface ProfileEstimate {
  /** The draft the estimate was computed for. */
  profile: InferenceProfile;
  /** The draft with its device resolved — `auto` never survives here. */
  resolvedProfile: InferenceProfile;
  /** The device the profile will actually run on. */
  device: ComputeDevice;
  modelShapeKnown: boolean;
  modelFileGb: number;
  /** The context the estimate assumes: the profile's pick, or the model's. */
  contextTokens: number;
  /** Context length the model declares, 0 when unknown. */
  maxContext: number;
  kvCacheGb: number;
  /**
   * What the same window costs under every cache type, keyed by the type's
   * wire name. Empty when the model shape is unknown, so the control that
   * picks the type can show each option's price next to it.
   */
  kvCacheGbByType: Partial<Record<KvCacheType, number>>;
  /** Working memory llama.cpp needs on top of the weights and the cache. */
  runtimeOverheadGb: number;
  ramNeededGb: number;
  ramTotalGb: number;
  /** RAM that is free right now. */
  ramAvailableGb: number;
  /** RAM left for inference once the reserve for the OS is set aside. */
  ramBudgetGb: number;
  /** Estimated VRAM when layers are offloaded to a user GPU build. */
  vramNeededGb: number;
  /** Total VRAM of the detected adapter, 0 when unknown. */
  vramTotalGb: number;
  /** VRAM left for inference once the compositor reserve is set aside. */
  vramBudgetGb: number;
  /** True when the adapter reported its memory size. */
  vramKnown: boolean;
  /** How well the configuration fits into the memory this machine has. */
  fit: FitLevel;
  /** Whether the model declares mixture-of-experts tensors. */
  isMoe: boolean;
  recommended: InferenceProfile | null;
  /** The validation findings for this draft. */
  report: ValidationReport;
}

/** A graphics card the llama-server build can see. */
export interface GpuDevice {
  name: string;
  /** "CUDA", "Vulkan" or "Metal". */
  backend: string;
  /** VRAM the adapter reports, when it says so. */
  vramGb: number | null;
}

/** What the llama-server build in use can do. */
export interface ServerCapabilities {
  /** True when the build lists at least one GPU device. */
  supportsGpu: boolean;
  devices: GpuDevice[];
  /** False when the probe could not run at all. */
  probed: boolean;
  /**
   * Whether the build exposes the flash-attention flag. `null` when the build
   * could not be asked, so the app does not pretend the feature is there.
   */
  flashAttn: boolean | null;
}