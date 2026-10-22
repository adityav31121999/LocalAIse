import { useEffect, useState } from "react";
import { invoke, isTauri, delay } from "../../api";
import {
  installGpuBuild,
  listGpuBuilds,
  type GpuBuildList,
  type HardwareReport,
} from "../../api/models";

const STAGE_LABELS: Record<string, string> = {
  resolving: "Looking up release archive…",
  downloading: "Downloading GPU backend…",
  extracting: "Unpacking backend binaries…",
  verifying: "Verifying GPU capabilities…",
  done: "Backend installation complete.",
};

export function HardwareCheck({ onNext, onBack }: { onNext: () => void; onBack: () => void }) {
  const [report, setReport] = useState<HardwareReport | null>(null);
  const [buildList, setBuildList] = useState<GpuBuildList | null>(null);
  const [installing, setInstalling] = useState(false);
  const [installStage, setInstallStage] = useState<string | null>(null);
  const [downloadProgress, setDownloadProgress] = useState<{ done: number; total: number } | null>(null);
  const [installDone, setInstallDone] = useState(false);
  const [installError, setInstallError] = useState<string | null>(null);

  const fallback: HardwareReport = {
    totalRamGb: 16,
    availableRamGb: 16,
    cpuCores: 8,
    cpuPhysicalCores: 8,
    cpuAvx2: true,
    cpuAvx512: false,
    gpuName: "NVIDIA GeForce RTX (CUDA)",
    gpuBackend: "CUDA",
    gpuVramGb: 8.0,
    gpuAvailable: true,
    recommendedTier: "Medium (7–8B, Q4_K_M)",
  };

  useEffect(() => {
    async function check() {
      if (isTauri) {
        try {
          const [rep, builds] = await Promise.all([
            invoke<HardwareReport>("get_hardware_report"),
            listGpuBuilds().catch(() => null),
          ]);
          setReport(rep);
          setBuildList(builds);
        } catch {
          setReport(fallback);
        }
      } else {
        await delay(200);
        setReport(fallback);
        setBuildList(await listGpuBuilds());
      }
    }
    void check();

    let unlisten: (() => void) | null = null;
    let cancelled = false;
    if (isTauri) {
      import("@tauri-apps/api/event")
        .then(({ listen }) =>
          listen("localAIse/gpu-build", (event) => {
            const payload = event.payload as { stage?: string; done?: number; total?: number };
            if (payload?.stage) setInstallStage(payload.stage);
            if (payload?.done && payload?.total) {
              setDownloadProgress({ done: payload.done, total: payload.total });
            }
          }),
        )
        .then((stop) => {
          if (cancelled) stop();
          else unlisten = stop;
        });
    }

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const recommendedTarget = buildList?.recommended ?? null;
  const recommendedStatus = buildList?.builds.find((b) => b.target === recommendedTarget);
  const isAlreadyInstalled = recommendedStatus?.installed ?? false;

  const handleInstallBackend = async () => {
    if (!recommendedTarget) return;
    setInstalling(true);
    setInstallError(null);
    setInstallStage("resolving");
    setDownloadProgress(null);
    try {
      await installGpuBuild(recommendedTarget);
      setInstallDone(true);
      const updated = await listGpuBuilds().catch(() => null);
      if (updated) setBuildList(updated);
    } catch (err) {
      setInstallError(err instanceof Error ? err.message : String(err));
    } finally {
      setInstalling(false);
      setInstallStage(null);
    }
  };

  return (
    <div style={{ maxWidth: "560px", margin: "40px auto", padding: "24px" }}>
      <h2 style={{ fontSize: "22px", marginBottom: "8px" }}>Hardware & Backend Setup</h2>
      <p className="muted" style={{ fontSize: "14px", marginBottom: "20px" }}>
        Detecting CPU and GPU hardware to configure the optimal backend (CUDA, Vulkan, OpenCL, or CPU).
      </p>

      {report ? (
        <div style={{ background: "var(--surface)", border: "1px solid var(--border)", borderRadius: "8px", padding: "16px", marginBottom: "20px" }}>
          <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "12px" }}>
            <span>CPU Cores:</span>
            <strong>{report.cpuCores} cores (auto microarchitecture dispatch)</strong>
          </div>
          <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "12px" }}>
            <span>System Memory:</span>
            <strong>~{report.totalRamGb.toFixed(1)} GB RAM</strong>
          </div>
          <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "12px" }}>
            <span>Graphics Hardware:</span>
            <strong>{report.gpuName || "None detected"}</strong>
          </div>
          <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "12px" }}>
            <span>Detected GPU Backend:</span>
            <strong style={{ color: "var(--accent)" }}>{report.gpuBackend}</strong>
          </div>
          {report.gpuVramGb != null && (
            <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "12px" }}>
              <span>Detected VRAM:</span>
              <strong>~{report.gpuVramGb.toFixed(1)} GB VRAM</strong>
            </div>
          )}
          <div style={{ display: "flex", justifyContent: "space-between" }}>
            <span>Recommended Model Tier:</span>
            <strong style={{ color: "var(--accent)" }}>{report.recommendedTier}</strong>
          </div>
        </div>
      ) : (
        <p className="muted">Probing hardware capabilities…</p>
      )}

      {/* GPU Backend Installation Box */}
      {recommendedTarget && (
        <div
          style={{
            background: "var(--surface-hover, rgba(255, 255, 255, 0.04))",
            border: "1px solid var(--border)",
            borderRadius: "8px",
            padding: "16px",
            marginBottom: "24px",
          }}
        >
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
            <div>
              <strong>Vendor Backend: {recommendedTarget.toUpperCase()}</strong>
              <div className="muted" style={{ fontSize: "13px", marginTop: "4px" }}>
                {isAlreadyInstalled || installDone
                  ? `✓ ${recommendedTarget.toUpperCase()} backend is ready and active.`
                  : recommendedStatus?.downloadable
                    ? `Download the vendor-optimized ${recommendedTarget.toUpperCase()} runtime for GPU acceleration.`
                    : `${recommendedTarget.toUpperCase()} detected (custom build from source).`}
              </div>
            </div>

            {!isAlreadyInstalled && !installDone && recommendedStatus?.downloadable && (
              <button
                type="button"
                className="btn btn--primary"
                disabled={installing}
                onClick={handleInstallBackend}
                style={{ marginLeft: "12px", whiteSpace: "nowrap" }}
              >
                {installing ? "Installing…" : `Install ${recommendedTarget.toUpperCase()}`}
              </button>
            )}
          </div>

          {installStage && (
            <div style={{ marginTop: "12px", fontSize: "13px", color: "var(--accent)" }}>
              <span>{STAGE_LABELS[installStage] ?? installStage}</span>
              {downloadProgress && downloadProgress.total > 0 && (
                <span style={{ marginLeft: "8px" }}>
                  ({Math.round((downloadProgress.done / downloadProgress.total) * 100)}%)
                </span>
              )}
            </div>
          )}

          {installError && (
            <div style={{ marginTop: "12px", fontSize: "13px", color: "var(--danger, #e57373)" }}>
              Installation failed: {installError}. You can still continue using the CPU backend.
            </div>
          )}
        </div>
      )}

      <div style={{ display: "flex", justifyContent: "space-between" }}>
        <button className="btn btn--subtle" onClick={onBack} disabled={installing}>
          Back
        </button>
        <button className="btn btn--primary" onClick={onNext} disabled={installing}>
          Next: Choose Model →
        </button>
      </div>
    </div>
  );
}
