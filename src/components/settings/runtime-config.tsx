/** The shared runtime configuration.
 *
 * The Inference and Hardware tabs edit the same profile, so the draft, the
 * live estimate and the validation findings live in one place. Everything the
 * user sees is measured by the Rust core: the estimate and the validation
 * come from the same code that starts the server, so the numbers on screen
 * cannot drift away from what will actually run.
 *
 * The flow is deliberate:
 *
 *   edit → measure → show what was found → apply → the server restarts
 *
 * Nothing is persisted while the user is still typing. Applying is the one
 * step that writes, and the core refuses it when the configuration has
 * errors.
 *
 * A configuration lives in one of three scopes. The global default is what
 * every model starts from; a model profile replaces it for that one model; a
 * project profile replaces both while that project is the one being worked
 * in. The scope decides where an applied profile is stored, and the runtime
 * always starts from the most specific one that exists.
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import type {
  ComputeDevice,
  InferenceProfile,
  KvCacheType,
  MoeOffload,
  ProfileEstimate,
  ProfileScope,
  ProfileStorage,
  RuntimeStatus,
  ServerCapabilities,
  ValidationMessage,
  ValidationReport,
} from "../../types";
import {
  DEFAULT_PROFILE,
  applyProfile,
  getHardwareReport,
  getSelectedModel,
  getServerBinary,
  profileEstimate,
  resetProfile,
  runtimeCapabilities,
  runtimeProfile,
  runtimeStatus,
  isModelReasoning,
} from "../../api/models";
import type { HardwareReport } from "../../api/models";
import { useProjectStore } from "../../state/projectStore";
import { PROFILE_BUDGET_DEFAULTS, withProfileDefaults } from "../../types";

/** How the three fit levels read on screen. */
export const FIT_LABELS: Record<ProfileEstimate["fit"], string> = {
  comfortable: "Fits comfortably",
  tight: "Tight memory budget",
  exceeds: "May exceed available memory",
};

/** How the three scopes read on screen. */
export const SCOPE_LABELS: Record<ProfileScope, string> = {
  global: "every model",
  model: "this model",
  project: "this project",
};

/** A readable name for a device setting. */
export function deviceLabel(device: ComputeDevice): string {
  switch (device.device) {
    case "auto":
      return "Automatic";
    case "cpu":
      return "CPU only";
    case "user_gpu":
      return device.gpu_layers >= 999
        ? "GPU · as many layers as fit"
        : `GPU · ${device.gpu_layers} layer${device.gpu_layers === 1 ? "" : "s"}`;
  }
}

/** The number of threads a profile ends up using. */
export function effectiveThreads(profile: InferenceProfile, cores: number | undefined): number {
  if (profile.threads > 0) return profile.threads;
  return Math.max(1, (cores ?? 2) - 1);
}

/**
 * The profile a scope starts from: its own when it has one, otherwise the
 * scope below it. The same rule the core applies when it stores a profile.
 */
export function scopeBase(storage: ProfileStorage, scope: ProfileScope): InferenceProfile {
  if (scope === "global") return storage.global;
  if (scope === "model") return storage.model ?? storage.global;
  return storage.project ?? storage.model ?? storage.global;
}

function sameDevice(a: ComputeDevice, b: ComputeDevice): boolean {
  if (a.device !== b.device) return false;
  if (a.device === "user_gpu" && b.device === "user_gpu") {
    return a.gpu_layers === b.gpu_layers;
  }
  return true;
}

function sameMoe(a: MoeOffload, b: MoeOffload): boolean {
  if (a.mode !== b.mode) return false;
  if (a.mode === "first_layers" && b.mode === "first_layers") {
    return a.n === b.n;
  }
  return true;
}

/** Whether two profiles ask for the same thing. */
export function sameProfile(a: InferenceProfile, b: InferenceProfile): boolean {
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
    sameDevice(a.device, b.device) &&
    sameMoe(a.moe, b.moe) &&
    sameBudgets(a, b)
  );
}

/** Token-budget fields participate in the dirty check only once present. */
function sameBudgets(a: InferenceProfile, b: InferenceProfile): boolean {
  const keys = Object.keys(PROFILE_BUDGET_DEFAULTS) as (keyof typeof PROFILE_BUDGET_DEFAULTS)[];
  return keys.every((key) => (a[key] ?? 0) === (b[key] ?? 0));
}

export interface RuntimeConfigValue {
  /** The profile being edited. */
  draft: InferenceProfile;
  /** The profile as it is stored, for the scope being edited. */
  saved: InferenceProfile;
  /** The scope the draft belongs to. */
  scope: ProfileScope;
  /** The profiles the app has stored. */
  storage: ProfileStorage | null;
  /** The scope whose profile is in effect right now. */
  effectiveScope: ProfileScope;
  /** The model the configuration is about. */
  modelPath: string | null;
  /** The project the configuration is about. */
  projectId: string | null;
  /** The name of that project, for the labels. */
  projectName: string | null;
  /** The memory estimate for the draft, null while it is being measured. */
  estimate: ProfileEstimate | null;
  /** What the core found when it measured the draft. */
  report: ValidationReport;
  /** What the llama-server build in use can do. */
  capabilities: ServerCapabilities | null;
  /** The machine this runs on. */
  hardware: HardwareReport | null;
  /** The runtime as it actually is right now. */
  status: RuntimeStatus | null;
  /** The llama-server build the runtime resolves to: the user's explicit
   * pick when there is one, otherwise an installed GPU build or the bundled
   * CPU build. */
  serverBinary: string | null;
  /** True when the draft differs from the stored profile. */
  dirty: boolean;
  /** True while the profile is being applied. */
  applying: boolean;
  /** The message of the last failed apply, if it failed. */
  error: string | null;
  /** The findings, most serious first. */
  messages: ValidationMessage[];
  /** True when at least one finding blocks the configuration. */
  blocked: boolean;
  patch(changes: Partial<InferenceProfile>): void;
  setDevice(device: ComputeDevice): void;
  setKvType(kvType: KvCacheType): void;
  setMoe(mode: MoeOffload["mode"]): void;
  /** Switches to another scope and loads the profile it holds. */
  setScope(scope: ProfileScope): void;
  /** Validates and stores the draft, then restarts the runtime. */
  apply(): Promise<void>;
  /** Throws the draft away and goes back to the stored profile. */
  reset(): void;
  /** Removes the stored profile of the scope, so the one below applies again. */
  clearScope(): Promise<void>;
  /** Takes the profile the core recommends for this model and machine. */
  useRecommended(): void;
  /** Reads the machine, the build and the runtime again. */
  refresh(): Promise<void>;
  /** Reads the build and the runtime again, leaving the draft alone. */
  refreshBuild(): Promise<void>;
}

const EMPTY_REPORT: ValidationReport = { messages: [], requiresRestart: false };

const RuntimeConfigContext = createContext<RuntimeConfigValue | null>(null);

export function RuntimeConfigProvider({
  active = true,
  children,
}: {
  /** While false the provider stays mounted but does no measuring or polling. */
  active?: boolean;
  children: ReactNode;
}) {
  const projectId = useProjectStore((state) => state.activeProjectId);
  const projectName = useProjectStore((state) => {
    const project = state.projects.find((entry) => entry.id === state.activeProjectId);
    return project?.name ?? null;
  });

  const [draft, setDraft] = useState<InferenceProfile>(DEFAULT_PROFILE);
  const [saved, setSaved] = useState<InferenceProfile>(DEFAULT_PROFILE);
  const [scope, setScopeState] = useState<ProfileScope>("global");
  const [storage, setStorage] = useState<ProfileStorage | null>(null);
  const [modelPath, setModelPath] = useState<string | null>(null);
  const [estimate, setEstimate] = useState<ProfileEstimate | null>(null);
  const [capabilities, setCapabilities] = useState<ServerCapabilities | null>(null);
  const [hardware, setHardware] = useState<HardwareReport | null>(null);
  const [status, setStatus] = useState<RuntimeStatus | null>(null);
  const [serverBinary, setServerBinary] = useState<string | null>(null);
  const [applying, setApplying] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // The scope is read while the storage is being fetched, so it is kept in a
  // ref as well: a refresh must not drag the draft back to another scope.
  const scopeRef = useRef(scope);
  scopeRef.current = scope;

  const refresh = useCallback(async () => {
    const [model, report, binary, caps, state] = await Promise.all([
      getSelectedModel().catch(() => null),
      getHardwareReport().catch(() => null),
      getServerBinary(),
      runtimeCapabilities().catch(() => null),
      runtimeStatus().catch(() => null),
    ]);
    const previousModel = modelPath;
    setModelPath(model);
    if (report) setHardware(report);
    setServerBinary(binary);
    if (caps) setCapabilities(caps);
    if (state) setStatus(state);

    // Auto-set thinking budget based on model type
    if (model && model !== previousModel) {
      const reasoning = await isModelReasoning(model).catch(() => false);
      if (!reasoning) {
        // Non-reasoning model: disable thinking budget
        setDraft((current) => ({ ...current, thinkingBudget: 0 }));
      } else {
        // Reasoning model: ensure thinking budget is enabled (use default if 0)
        setDraft((current) =>
          current.thinkingBudget === 0
            ? { ...current, thinkingBudget: PROFILE_BUDGET_DEFAULTS.maxOutputTokens ?? 2048 }
            : current
        );
      }
    }

    const stored = await runtimeProfile(model, projectId).catch(() => null);
    if (stored) {
      setStorage(stored);
      const base = withProfileDefaults(scopeBase(stored, scopeRef.current));
      setDraft(base);
      setSaved(base);
    }
  }, [projectId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const refreshBuild = useCallback(async () => {
    const [binary, caps, state] = await Promise.all([
      getServerBinary(),
      runtimeCapabilities().catch(() => null),
      runtimeStatus().catch(() => null),
    ]);
    setServerBinary(binary);
    if (caps) setCapabilities(caps);
    if (state) setStatus(state);
  }, []);

  /** The runtime changes on its own — the server starts when a chat turn
   * arrives — so its state is polled instead of fetched once. */
  useEffect(() => {
    if (!active) return;
    const timer = window.setInterval(() => {
      runtimeStatus()
        .then(setStatus)
        .catch(() => undefined);
    }, 4_000);
    return () => window.clearInterval(timer);
  }, [active]);

  /** Every edit is measured by the core. The estimate carries the findings
   * with it, so one round trip covers both. */
  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    const timer = window.setTimeout(() => {
      profileEstimate(draft, scope, modelPath, projectId)
        .then((next) => {
          if (!cancelled) setEstimate(next);
        })
        .catch(() => {
          if (!cancelled) setEstimate(null);
        });
    }, 250);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [draft, active, scope, modelPath, projectId]);

  const patch = useCallback((changes: Partial<InferenceProfile>) => {
    setError(null);
    setDraft((current) => ({ ...current, ...changes }));
  }, []);

  const setDevice = useCallback((device: ComputeDevice) => {
    setError(null);
    setDraft((current) => ({
      ...current,
      device,
      // Expert placement only does something when layers run on the GPU.
      moe: device.device === "cpu" ? { mode: "off" } : current.moe,
    }));
  }, []);

  const setKvType = useCallback((kvType: KvCacheType) => {
    setError(null);
    setDraft((current) => ({
      ...current,
      kvType,
      // A quantized cache only works with flash attention, so picking one
      // turns it on.
      flashAttn: kvType === "f16" ? current.flashAttn : true,
    }));
  }, []);

  const setMoe = useCallback((mode: MoeOffload["mode"]) => {
    setError(null);
    setDraft((current) => {
      if (mode === "first_layers") {
        const n = current.moe.mode === "first_layers" ? current.moe.n : 4;
        return { ...current, moe: { mode: "first_layers", n } };
      }
      if (mode === "all_experts") {
        return { ...current, moe: { mode: "all_experts" } };
      }
      return { ...current, moe: { mode: "off" } };
    });
  }, []);

  /** Switching scope swaps the draft for the profile that scope holds, so
   * the two never bleed into each other. */
  const setScope = useCallback(
    (next: ProfileScope) => {
      setScopeState(next);
      setError(null);
      if (storage) {
        const base = withProfileDefaults(scopeBase(storage, next));
        setDraft(base);
        setSaved(base);
      }
    },
    [storage],
  );

  const apply = useCallback(async () => {
    setApplying(true);
    setError(null);
    try {
      const report = await applyProfile(draft, scope, modelPath, projectId);
      // The core stored the profile and stopped the running server when the
      // configuration in effect changed, so both are read again: this is the
      // state the app is really in.
      const stored = await runtimeProfile(modelPath, projectId).catch(() => null);
      if (stored) {
        setStorage(stored);
        setSaved(scopeBase(stored, scope));
      }
      setEstimate((current) => (current ? { ...current, report } : current));
      const state = await runtimeStatus().catch(() => null);
      if (state) setStatus(state);
    } catch (applyError) {
      setError(applyError instanceof Error ? applyError.message : String(applyError));
    } finally {
      setApplying(false);
    }
  }, [draft, scope, modelPath, projectId]);

  const reset = useCallback(() => {
    setError(null);
    setDraft(saved);
  }, [saved]);

  /** Removes the profile of the scope, so the scope below it applies again. */
  const clearScope = useCallback(async () => {
    setApplying(true);
    setError(null);
    try {
      const stored = await resetProfile(scope, modelPath, projectId);
      setStorage(stored);
      const base = withProfileDefaults(scopeBase(stored, scope));
      setDraft(base);
      setSaved(base);
      const state = await runtimeStatus().catch(() => null);
      if (state) setStatus(state);
    } catch (clearError) {
      setError(clearError instanceof Error ? clearError.message : String(clearError));
    } finally {
      setApplying(false);
    }
  }, [scope, modelPath, projectId]);

  const useRecommended = useCallback(() => {
    const recommended = estimate?.recommended;
    if (!recommended) return;
    setError(null);
    setDraft(withProfileDefaults(recommended));
  }, [estimate]);

  const report = estimate?.report ?? EMPTY_REPORT;
  const dirty = !sameProfile(draft, saved);

  const value = useMemo<RuntimeConfigValue>(
    () => ({
      draft,
      saved,
      scope,
      storage,
      effectiveScope: storage?.effectiveScope ?? "global",
      modelPath,
      projectId,
      projectName,
      estimate,
      report,
      capabilities,
      hardware,
      status,
      serverBinary,
      dirty,
      applying,
      error,
      messages: report.messages,
      blocked: report.messages.some((message) => message.severity === "error"),
      patch,
      setDevice,
      setKvType,
      setMoe,
      setScope,
      apply,
      reset,
      clearScope,
      useRecommended,
      refresh,
      refreshBuild,
    }),
    [
      draft,
      saved,
      scope,
      storage,
      modelPath,
      projectId,
      projectName,
      estimate,
      report,
      capabilities,
      hardware,
      status,
      serverBinary,
      dirty,
      applying,
      error,
      patch,
      setDevice,
      setKvType,
      setMoe,
      setScope,
      apply,
      reset,
      clearScope,
      useRecommended,
      refresh,
      refreshBuild,
    ],
  );

  return <RuntimeConfigContext.Provider value={value}>{children}</RuntimeConfigContext.Provider>;
}

export function useRuntimeConfig(): RuntimeConfigValue {
  const value = useContext(RuntimeConfigContext);
  if (!value) {
    throw new Error("useRuntimeConfig has to be used inside a RuntimeConfigProvider");
  }
  return value;
}
