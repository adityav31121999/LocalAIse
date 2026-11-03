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
 * There is one live inference profile — the app-wide settings the runtime
 * starts with. Applying stores it as the live profile and restarts the
 * server so the next chat turn uses the new flags.
 */

import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import type {
  ComputeDevice,
  InferenceProfile,
  KvCacheType,
  MoeOffload,
  ProfileEstimate,
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
  runtimeCapabilities,
  runtimeProfile,
  runtimeStatus,
  isModelReasoning,
} from "../../api/models";
import type { HardwareReport } from "../../api/models";
import { PROFILE_BUDGET_DEFAULTS, withProfileDefaults } from "../../types";

/** How the three fit levels read on screen. */
export const FIT_LABELS: Record<ProfileEstimate["fit"], string> = {
  comfortable: "Fits comfortably",
  tight: "Tight memory budget",
  exceeds: "May exceed available memory",
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
  /** The profile as it is stored (the app-wide live profile). */
  saved: InferenceProfile;
  /** The model the configuration is about. */
  modelPath: string | null;
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
  /** Validates and stores the draft, then restarts the runtime. */
  apply(): Promise<void>;
  /** Throws the draft away and goes back to the stored profile. */
  reset(): void;
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
  const [draft, setDraft] = useState<InferenceProfile>(DEFAULT_PROFILE);
  const [saved, setSaved] = useState<InferenceProfile>(DEFAULT_PROFILE);
  const [modelPath, setModelPath] = useState<string | null>(null);
  const [estimate, setEstimate] = useState<ProfileEstimate | null>(null);
  const [capabilities, setCapabilities] = useState<ServerCapabilities | null>(null);
  const [hardware, setHardware] = useState<HardwareReport | null>(null);
  const [status, setStatus] = useState<RuntimeStatus | null>(null);
  const [serverBinary, setServerBinary] = useState<string | null>(null);
  const [applying, setApplying] = useState(false);
  const [error, setError] = useState<string | null>(null);

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

    // Auto-set thinking budget for reasoning models only.
    // Non-reasoning models keep the default (from DEFAULT_PROFILE or stored profile).
    if (model && model !== previousModel) {
      const reasoning = await isModelReasoning(model).catch(() => false);
      if (reasoning) {
        // Reasoning model: ensure thinking budget is enabled (use default if 0)
        setDraft((current) =>
          current.thinkingBudget === 0
            ? { ...current, thinkingBudget: PROFILE_BUDGET_DEFAULTS.maxOutputTokens ?? 2048 }
            : current
        );
      }
    }

    const stored = await runtimeProfile().catch(() => null);
    if (stored) {
      setDraft(stored);
      setSaved(stored);
    }
  }, []);

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
      profileEstimate(draft, modelPath)
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
  }, [draft, active, modelPath]);

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

  const apply = useCallback(async () => {
    setApplying(true);
    setError(null);
    try {
      const report = await applyProfile(draft, modelPath);
      // The core stored the live profile and stopped the running server when
      // the configuration changed, so the draft is now what is stored.
      setSaved(draft);
      setEstimate((current) => (current ? { ...current, report } : current));
      const state = await runtimeStatus().catch(() => null);
      if (state) setStatus(state);
    } catch (applyError) {
      setError(applyError instanceof Error ? applyError.message : String(applyError));
    } finally {
      setApplying(false);
    }
  }, [draft, modelPath]);

  const reset = useCallback(() => {
    setError(null);
    setDraft(saved);
  }, [saved]);

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
      modelPath,
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
      apply,
      reset,
      useRecommended,
      refresh,
      refreshBuild,
    }),
    [
      draft,
      saved,
      modelPath,
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
      apply,
      reset,
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
