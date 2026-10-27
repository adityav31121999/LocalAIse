import { useRuntimeConfig } from "./runtime-config";
import type { KvCacheType } from "../../types";

/**
 * A short, actionable warning when the draft cannot fit in memory. The
 * detailed breakdown lives in the memory bar; this card only says what is
 * wrong and offers the one-click correction.
 */
export function MemoryWarningAlert() {
  const { estimate, draft, patch, setKvType } = useRuntimeConfig();

  if (!estimate || !estimate.modelShapeKnown) return null;

  const onGpu = estimate.device.device === "user_gpu";
  const ramExceeds = !onGpu && estimate.ramNeededGb > estimate.ramBudgetGb;
  const vramExceeds =
    onGpu && estimate.vramKnown && estimate.vramNeededGb > estimate.vramBudgetGb;

  if (!ramExceeds && !vramExceeds && estimate.fit !== "exceeds") return null;

  const overBy = Math.max(
    0,
    onGpu ? estimate.vramNeededGb - estimate.vramBudgetGb : estimate.ramNeededGb - estimate.ramBudgetGb,
  );

  const handleAutoTune = () => {
    const safeContext = Math.min(draft.contextTokens || 8192, 4096);
    patch({
      contextTokens: safeContext,
      kvType: "q4_0" as KvCacheType,
      flashAttn: true,
      partialLoad: true,
    });
    setKvType("q4_0");
  };

  return (
    <div role="alert" className="doc-recovery">
      <div className="doc-recovery__title">
        <span aria-hidden="true">!</span>
        This profile does not fit {onGpu ? "in VRAM" : "in RAM"}
      </div>
      <p className="doc-recovery__cause">
        The current settings ask for roughly{" "}
        <code>
          {overBy > 0
            ? `${overBy.toFixed(2)} GB more than the ${onGpu ? "graphics memory" : "memory"} budget`
            : "more memory than the device budget allows"}
        </code>
        . Starting the server like this risks heavy swapping or an out-of-memory kill.
      </p>
      <div className="doc-recovery__actions">
        <button type="button" className="btn btn--primary btn--sm" onClick={handleAutoTune}>
          Apply a safe profile
        </button>
        <span className="muted" style={{ fontSize: 11.5, alignSelf: "center" }}>
          Lower context to 4K, Q4 KV cache, partial loading.
        </span>
      </div>
    </div>
  );
}
