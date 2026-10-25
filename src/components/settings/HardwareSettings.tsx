/** The Hardware settings tab: where the model runs.
 *
 * The device, the build and the memory of the machine are one story: the GPU
 * is only used when the llama-server build in use can offload to it, and the
 * app asks that build itself rather than guessing from the driver files.
 */

import { useEffect, useState } from "react";
import {
  installGpuBuild,
  listGpuBuilds,
  pickServerBinary,
  type GpuBuildList,
  type GpuBuildStatus,
} from "../../api/models";
import { isTauri } from "../../api/index";
import { RuntimeActions } from "./RuntimeActions";
import { MemoryWarningAlert } from "./MemoryWarningAlert";
import { deviceLabel, useRuntimeConfig } from "./runtime-config";
import { DownloadProgress } from "../ui/DownloadProgress";
import { NumberField } from "../ui/NumberField";

const STATE_LABELS: Record<string, string> = {
  ready: "Running",
  starting: "Starting",
  stopped: "Stopped",
  error: "Failed",
};

/** What the install pipeline reports, in the order it reports it. */
const STAGE_LABELS: Record<string, string> = {
  resolving: "Looking up the release…",
  downloading: "Downloading…",
  extracting: "Unpacking…",
  verifying: "Asking the new build…",
  done: "Done.",
};

/** One backend build of llama.cpp: what is on disk and what to do next. */
function BuildRow({
  build,
  busy,
  installing,
  disabled,
  onInstall,
}: {
  build: GpuBuildStatus;
  /** True while any install runs, so two never race for the folder. */
  busy: boolean;
  /** True while this row's install runs. */
  installing: boolean;
  disabled: boolean;
  onInstall: (target: string) => void;
}) {
  return (
    <div className="caps__row" key={build.target}>
      <span className="caps__key">
        {build.label}
        {build.recommended ? <span className="muted"> · fits your graphics card</span> : ""}
        <br />
        <span className="muted">
          {build.installed
            ? build.source
              ? `installed from llama.cpp ${build.source.releaseTag}`
              : "installed"
            : build.downloadable
              ? "not installed"
              : "no official archive — build with scripts/fetch-llama-server.ts"}
        </span>
      </span>
      <span className="caps__value">
        {build.installed ? (
          "ready"
        ) : build.downloadable ? (
          <button
            type="button"
            className="btn btn--secondary"
            disabled={disabled || busy}
            onClick={() => onInstall(build.target)}
          >
            {installing ? "Installing…" : "Install"}
          </button>
        ) : (
          <span className="muted">from source</span>
        )}
      </span>
    </div>
  );
}

export function HardwareSettings() {
  const {
    draft,
    estimate,
    capabilities,
    hardware,
    status,
    serverBinary,
    setDevice,
    patch,
    refreshBuild,
  } = useRuntimeConfig();
  const [picking, setPicking] = useState(false);
  const [builds, setBuilds] = useState<GpuBuildList | null>(null);
  const [installing, setInstalling] = useState<string | null>(null);
  const [installStage, setInstallStage] = useState<string | null>(null);
  /** How far the running download has got, in bytes, as the installer reports it. */
  const [progress, setProgress] = useState<{ target: string; done: number; total: number } | null>(
    null,
  );
  const [buildError, setBuildError] = useState<string | null>(null);

  const readBuilds = async () => {
    const list = await listGpuBuilds().catch(() => null);
    setBuilds(list);
  };

  useEffect(() => {
    void readBuilds();
    // The install pipeline reports its steps; the label follows along.
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    if (isTauri) {
      import("@tauri-apps/api/event")
        .then(({ listen }) =>
          listen("localAIse/gpu-build", (event) => {
            const payload = event.payload as {
              stage?: string;
              target?: string;
              done?: number;
              total?: number;
            };
            if (payload?.stage) setInstallStage(payload.stage);
            // A download reports bytes; other stages (unpacking, probing) do not.
            if (payload?.stage === "downloading" && payload.target != null) {
              setProgress({
                target: payload.target,
                done: payload.done ?? 0,
                total: payload.total ?? 0,
              });
            }
          }),
        )
        .then((stop) => {
          if (cancelled) stop();
          else unlisten = stop;
        })
        .catch(() => {});
    }
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const device = draft.device;
  const isAuto = device.device === "auto";
  const isCpu = device.device === "cpu";
  const gpuLayers = device.device === "user_gpu" ? device.gpu_layers : 0;

  // What automatic mode resolves to right now: the core answers this from
  // the build and the model.
  const resolved = estimate?.device ?? null;
  const resolvedLayers = resolved?.device === "user_gpu" ? resolved.gpu_layers : 0;

  const chooseBuild = async () => {
    setPicking(true);
    try {
      const picked = await pickServerBinary();
      if (picked) await refreshBuild();
      await readBuilds();
    } finally {
      setPicking(false);
    }
  };

  const install = async (target: string) => {
    setInstalling(target);
    setBuildError(null);
    setInstallStage("resolving");
    setProgress(null);
    try {
      await installGpuBuild(target);
      // The new build takes over as the resolved one; both readings move.
      await Promise.all([refreshBuild(), readBuilds()]);
    } catch (error) {
      setBuildError(error instanceof Error ? error.message : String(error));
    } finally {
      setInstalling(null);
      setInstallStage(null);
      setProgress(null);
    }
  };

  const flashSupport = capabilities?.flashAttn ?? null;

  return (
    <div className="perf">
      <MemoryWarningAlert />
      <section className="field" aria-labelledby="field-device">
        <h2 className="field__label" id="field-device">
          Compute device
        </h2>
        <p className="field__hint">
          Automatic uses the graphics card when this machine has one and the llama-server build in
          use can offload to it, and the CPU otherwise. CPU never allocates VRAM.
        </p>
        <div className="segmented" role="group" aria-labelledby="field-device">
          <button
            type="button"
            className="segmented__item"
            aria-pressed={isAuto}
            onClick={() => setDevice({ device: "auto" })}
          >
            Automatic
          </button>
          <button
            type="button"
            className="segmented__item"
            aria-pressed={isCpu}
            onClick={() => setDevice({ device: "cpu" })}
          >
            CPU only
          </button>
          <button
            type="button"
            className="segmented__item"
            aria-pressed={device.device === "user_gpu"}
            onClick={() => setDevice({ device: "user_gpu", gpu_layers: 999 })}
          >
            GPU
          </button>
        </div>

        <p className="field__hint">
          {resolved ? (
            <>
              This configuration runs on <strong>{deviceLabel(resolved)}</strong>
              {resolvedLayers > 0 && resolvedLayers < 999
                ? ` — ${resolvedLayers} of the model's layers are offloaded.`
                : "."}
            </>
          ) : (
            "The device is resolved once a model is selected."
          )}
        </p>

        {device.device === "user_gpu" && (
          <>
            <div className="perf__slider-row">
              <span className="field__hint">GPU layers (-ngl):</span>
              <input
                className="slider"
                type="range"
                min={0}
                max={999}
                step={1}
                value={gpuLayers}
                onChange={(event) =>
                  setDevice({ device: "user_gpu", gpu_layers: Number(event.target.value) })
                }
                aria-label="GPU layers"
              />
              <span className="perf__slider-value mono">
                {gpuLayers >= 999 ? "auto" : gpuLayers}
              </span>
            </div>
            <p className="field__hint">
              {gpuLayers >= 999
                ? "As many layers as fit in the graphics memory are offloaded."
                : `${gpuLayers} layer${gpuLayers === 1 ? "" : "s"} run on the graphics card; the rest stay on the CPU.`}
            </p>
          </>
        )}

        {device.device === "user_gpu" && capabilities?.probed && !capabilities.supportsGpu && (
          <p className="field__hint perf__error">
            The llama-server build in use reports no GPU device. Install a Vulkan or CUDA build
            below, or point the app at your own GPU build.
          </p>
        )}
      </section>

      <section className="field" aria-labelledby="field-build">
        <h2 className="field__label" id="field-build">
          llama-server build
        </h2>
        <p className="field__hint">
          The installer ships a CPU-only build of llama.cpp. For the graphics card, the app can
          install the official llama.cpp build that matches this machine — CUDA on NVIDIA, Vulkan
          everywhere else — verified against the checksums pinned in the app, or you can point
          it at your own build.
        </p>
        <div className="caps">
          <div className="caps__row">
            <span className="caps__key">Build in use</span>
            <span className="caps__value mono truncate">
              {serverBinary ?? "bundled CPU-only build"}
            </span>
          </div>
          <div className="caps__row">
            <span className="caps__key">GPU support</span>
            <span className="caps__value">
              {!capabilities?.probed
                ? "the build could not be asked"
                : capabilities.supportsGpu
                  ? `yes${capabilities.devices[0]?.backend ? ` · ${capabilities.devices[0].backend}` : ""}`
                  : "no — this build cannot offload"}
            </span>
          </div>
          <div className="caps__row">
            <span className="caps__key">Flash attention</span>
            <span className="caps__value">
              {flashSupport === true
                ? "supported"
                : flashSupport === false
                  ? "not offered by this build"
                  : "unknown — the build did not answer"}
            </span>
          </div>
          {capabilities?.devices.map((device) => (
            <div className="caps__row" key={`${device.backend}-${device.name}`}>
              <span className="caps__key">{device.backend}</span>
              <span className="caps__value truncate">
                {device.name}
                {device.vramGb ? ` · ${device.vramGb.toFixed(1)} GB VRAM` : ""}
              </span>
            </div>
          ))}
        </div>

        {builds && (
          <div className="caps caps--builds">
            <div className="caps__row">
              <span className="caps__key">
                GPU builds
                <br />
                <span className="muted">{builds.gpuName || "no graphics card detected"}</span>
              </span>
              <span className="caps__value muted">{builds.gpuBackend}</span>
            </div>
            {builds.builds.map((entry) => (
              <BuildRow
                build={entry}
                key={entry.target}
                busy={installing != null}
                installing={installing === entry.target}
                disabled={!builds.gpuAvailable && !entry.installed}
                onInstall={(target) => void install(target)}
              />
            ))}
          </div>
        )}
        {installing && installStage && (
          <p className="field__hint">
            {STAGE_LABELS[installStage] ?? installStage} {installing ? `(${installing})` : ""}
          </p>
        )}
        {installing && (
          <DownloadProgress
            done={progress?.done ?? 0}
            total={progress?.total ?? 0}
            pendingLabel={
              installStage && installStage !== "downloading"
                ? "The archive is unpacking — the download itself is finished."
                : "Waiting for the download to start…"
            }
          />
        )}
        {buildError && <p className="field__hint perf__error">{buildError}</p>}

        <div className="perf__slider-row">
          <button
            type="button"
            className="btn btn--secondary"
            disabled={picking}
            onClick={() => void chooseBuild()}
          >
            {picking ? "Opening…" : "Choose build…"}
          </button>
        </div>
      </section>

      <section className="field" aria-labelledby="field-machine">
        <h2 className="field__label" id="field-machine">
          This machine
        </h2>
        <div className="caps">
          <div className="caps__row">
            <span className="caps__key">Processor</span>
            <span className="caps__value">
              {hardware ? `${hardware.cpuCores} logical cores` : "not known yet"}
              {hardware?.cpuPhysicalCores
                ? ` · ${hardware.cpuPhysicalCores} physical`
                : ""}
            </span>
          </div>
          <div className="caps__row">
            <span className="caps__key">Memory</span>
            <span className="caps__value">
              {hardware
                ? `${hardware.totalRamGb.toFixed(1)} GB total · ${hardware.availableRamGb.toFixed(1)} GB free`
                : "not known yet"}
            </span>
          </div>
          <div className="caps__row">
            <span className="caps__key">Graphics card</span>
            <span className="caps__value">
              {hardware?.gpuAvailable
                ? `${hardware.gpuName} · ${hardware.gpuBackend}${
                    hardware.gpuVramGb ? ` · ${hardware.gpuVramGb.toFixed(1)} GB VRAM` : ""
                  }`
                : "none detected"}
            </span>
          </div>
          {estimate && (
            <div className="caps__row">
              <span className="caps__key">Budget for inference</span>
              <span className="caps__value">
                {estimate.ramBudgetGb.toFixed(1)} GB of RAM
                {estimate.vramKnown ? ` · ${estimate.vramBudgetGb.toFixed(1)} GB of VRAM` : ""}
              </span>
            </div>
          )}
        </div>
        <p className="field__hint">
          The graphics card is detected by the system, but the build's own device list decides what
          the runtime can actually use.
        </p>
      </section>

      <section className="field" aria-labelledby="field-status">
        <h2 className="field__label" id="field-status">
          Runtime status
        </h2>
        {status ? (
          <div className="runtime-status">
            <div className="runtime-status__head">
              <span className={`runtime-status__state runtime-status__state--${status.state}`}>
                {STATE_LABELS[status.state] ?? status.state}
              </span>
              <span className="muted truncate">{status.model}</span>
            </div>
            <div className="runtime-status__rows">
              <div className="caps__row">
                <span className="caps__key">Port</span>
                <span className="caps__value mono">{status.port ?? "—"}</span>
              </div>
              <div className="caps__row">
                <span className="caps__key">Running on</span>
                <span className="caps__value">
                  {status.device ? deviceLabel(status.device) : "—"}
                </span>
              </div>
              <div className="caps__row">
                <span className="caps__key">Running context</span>
                <span className="caps__value mono">
                  {status.profile
                    ? status.profile.contextTokens === 0
                      ? "model default"
                      : `${status.profile.contextTokens.toLocaleString()} tokens`
                    : "—"}
                </span>
              </div>
            </div>
            {status.error && <p className="perf__error">{status.error}</p>}
          </div>
        ) : (
          <p className="muted">The runtime state is not known yet.</p>
        )}
        <p className="field__hint">
          The server starts on the next chat turn. Changing the profile and applying stops the
          running one, so the next turn starts it again with the new flags.
        </p>
      </section>

      <section className="field" aria-labelledby="field-advanced">
        <h2 className="field__label" id="field-advanced">
          Batching
        </h2>
        <p className="field__hint">
          The batch size decides how many tokens are processed at once, and the number of slots how
          many conversations can be served in parallel. Both are rarely worth changing.
        </p>
        <div className="perf__slider-row">
          <span className="field__hint">Batch size:</span>
          <NumberField
            className="input perf__threads"
            min={1}
            step={64}
            value={draft.batchSize}
            onChange={(n) => patch({ batchSize: n })}
            label="Batch size"
          />
          <span className="field__hint">Parallel slots:</span>
          <NumberField
            className="input perf__threads"
            min={1}
            step={1}
            value={draft.parallelSlots}
            onChange={(n) => patch({ parallelSlots: n })}
            label="Parallel slots"
          />
        </div>
      </section>

      <RuntimeActions />
    </div>
  );
}
