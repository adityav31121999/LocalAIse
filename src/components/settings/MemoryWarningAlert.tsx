import { useRuntimeConfig } from "./runtime-config";
import type { KvCacheType } from "../../types";

export function MemoryWarningAlert() {
  const { estimate, draft, patch, setKvType } = useRuntimeConfig();

  if (!estimate || !estimate.modelShapeKnown) return null;

  const onGpu = estimate.device.device === "user_gpu";
  const ramNeeded = estimate.ramNeededGb;
  const ramTotal = estimate.ramTotalGb;
  const ramAvailable = estimate.ramAvailableGb;
  const ramBudget = estimate.ramBudgetGb;

  const vramNeeded = estimate.vramNeededGb;
  const vramTotal = estimate.vramTotalGb;
  const vramBudget = estimate.vramBudgetGb;

  const ramExceedsTotal = !onGpu && ramNeeded > ramTotal;
  const ramExceedsAvailable = !onGpu && ramNeeded > ramBudget;
  const vramExceedsTotal = onGpu && estimate.vramKnown && vramNeeded > vramTotal;
  const vramExceedsBudget = onGpu && estimate.vramKnown && vramNeeded > vramBudget;

  const isExceeded = ramExceedsAvailable || ramExceedsTotal || vramExceedsTotal || vramExceedsBudget || estimate.fit === "exceeds";

  if (!isExceeded) return null;

  const handleAutoTune = () => {
    // Apply safe profile: lower context to 8192 or 4096, switch KV to q4_0, enable partialLoad
    const safeContext = Math.min(draft.contextTokens || 8192, 4096);
    patch({
      contextTokens: safeContext,
      kvType: "q4_0" as KvCacheType,
      flashAttn: true,
      partialLoad: true,
    });
    setKvType("q4_0");
  };

  const ramOverdraft = Math.max(0, ramNeeded - ramTotal);
  const vramOverdraft = Math.max(0, vramNeeded - vramTotal);

  return (
    <div
      role="alert"
      className="memory-alert"
      style={{
        margin: "0 0 var(--space-4) 0",
        padding: "var(--space-4)",
        background: "rgba(220, 38, 38, 0.08)",
        border: "1px solid rgba(220, 38, 38, 0.35)",
        borderRadius: "var(--radius-lg)",
        color: "var(--text)",
      }}
    >
      <div style={{ display: "flex", alignItems: "flex-start", gap: "12px" }}>
        <div
          style={{
            display: "inline-flex",
            alignItems: "center",
            justifyContent: "center",
            width: "28px",
            height: "28px",
            borderRadius: "50%",
            background: "rgba(220, 38, 38, 0.18)",
            color: "#ef4444",
            flexShrink: 0,
            marginTop: "1px",
            fontWeight: "bold",
            fontSize: "14px",
          }}
          aria-hidden="true"
        >
          !
        </div>

        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", flexWrap: "wrap", gap: "8px" }}>
            <h3 style={{ fontSize: "14px", fontWeight: "600", color: "#f87171", margin: 0 }}>
              {vramExceedsTotal || vramExceedsBudget
                ? ramExceedsTotal || ramExceedsAvailable
                  ? "Estimated Memory Exceeds Available Physical RAM & VRAM"
                  : "Estimated Memory Exceeds Available GPU VRAM"
                : "Estimated Memory Exceeds Available Physical RAM"}
            </h3>
            <span
              style={{
                fontSize: "11px",
                fontFamily: "var(--font-mono)",
                fontWeight: "600",
                padding: "2px 8px",
                borderRadius: "var(--radius-xs)",
                background: "rgba(220, 38, 38, 0.2)",
                color: "#fca5a5",
                letterSpacing: "0.02em",
              }}
            >
              CRITICAL OVERFLOW
            </span>
          </div>

          <p style={{ fontSize: "12.5px", color: "var(--text-secondary)", marginTop: "6px", marginBottom: "12px", lineHeight: 1.5 }}>
            The current inference configuration exceeds physical memory boundaries. Running llama-server with these
            parameters risks severe OS swapping, unresponsiveness, or process termination by the Out-Of-Memory (OOM) killer.
          </p>

          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fit, minmax(170px, 1fr))",
              gap: "8px",
              padding: "10px",
              background: "var(--surface)",
              borderRadius: "var(--radius-md)",
              border: "1px solid var(--border)",
              marginBottom: "12px",
              fontSize: "12px",
            }}
          >
            {onGpu ? (
              <>
                <div>
                  <span className="muted" style={{ display: "block", fontSize: "11px" }}>Estimated VRAM Needed</span>
                  <strong className="mono" style={{ color: "#ef4444", fontSize: "14px" }}>
                    {vramNeeded.toFixed(2)} GB
                  </strong>
                </div>
                <div>
                  <span className="muted" style={{ display: "block", fontSize: "11px" }}>Physical GPU VRAM</span>
                  <span className="mono" style={{ fontSize: "14px" }}>
                    {vramTotal.toFixed(1)} GB
                  </span>
                </div>
                <div>
                  <span className="muted" style={{ display: "block", fontSize: "11px" }}>VRAM Overdraft</span>
                  <span className="mono tabular-nums" style={{ color: "#f87171", fontWeight: "600", fontSize: "14px" }}>
                    +{vramOverdraft > 0 ? vramOverdraft.toFixed(2) : (vramNeeded - vramBudget).toFixed(2)} GB
                  </span>
                </div>
              </>
            ) : (
              <>
                <div>
                  <span className="muted" style={{ display: "block", fontSize: "11px" }}>Estimated RAM Needed</span>
                  <strong className="mono" style={{ color: "#ef4444", fontSize: "14px" }}>
                    {ramNeeded.toFixed(2)} GB
                  </strong>
                </div>
                <div>
                  <span className="muted" style={{ display: "block", fontSize: "11px" }}>Physical RAM Total</span>
                  <span className="mono" style={{ fontSize: "14px" }}>
                    {ramTotal.toFixed(1)} GB <span className="muted" style={{ fontSize: "11px" }}>({ramAvailable.toFixed(1)} GB free)</span>
                  </span>
                </div>
                <div>
                  <span className="muted" style={{ display: "block", fontSize: "11px" }}>RAM Overdraft</span>
                  <span className="mono tabular-nums" style={{ color: "#f87171", fontWeight: "600", fontSize: "14px" }}>
                    +{ramOverdraft > 0 ? ramOverdraft.toFixed(2) : (ramNeeded - ramBudget).toFixed(2)} GB
                  </span>
                </div>
              </>
            )}
          </div>

          <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", flexWrap: "wrap", gap: "8px" }}>
            <div style={{ fontSize: "11.5px", color: "var(--text-muted)" }}>
              Recommended: Reduce context window, use Q4 KV cache, or enable partial loading.
            </div>
            <button
              type="button"
              className="btn btn--primary btn--sm"
              onClick={handleAutoTune}
              style={{
                background: "#dc2626",
                color: "#ffffff",
                fontSize: "12px",
                fontWeight: "600",
                padding: "0 12px",
              }}
            >
              Auto-Tune to Safe Budget
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}