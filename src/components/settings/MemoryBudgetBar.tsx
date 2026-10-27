import { FIT_LABELS, deviceLabel } from "./runtime-config";
import type { ProfileEstimate } from "../../types";

interface MemoryBudgetBarProps {
  estimate: ProfileEstimate;
}

function gb(value: number): string {
  return value >= 10 ? `${value.toFixed(1)} GB` : `${value.toFixed(2)} GB`;
}

/**
 * The memory estimate as one stacked bar instead of a wall of numbers:
 * weights, KV cache and runtime overhead fill the device budget, and the
 * healthy remainder stays visible as "free". An over-budget configuration
 * spills into a rose segment so the overflow is impossible to miss.
 */
export function MemoryBudgetBar({ estimate }: MemoryBudgetBarProps) {
  const onGpu = estimate.device.device === "user_gpu" && estimate.vramKnown;
  const budget = onGpu ? estimate.vramBudgetGb : estimate.ramBudgetGb;
  const total = onGpu ? estimate.vramTotalGb : estimate.ramTotalGb;
  const needed = onGpu ? estimate.vramNeededGb : estimate.ramNeededGb;

  const weights = estimate.modelFileGb;
  const kv = estimate.kvCacheGb;
  const overhead = estimate.runtimeOverheadGb;

  // Everything is drawn against the device budget, which is what the server
  // is actually allowed to use. Over-budget work is drawn in its own segment.
  const denominator = Math.max(budget, needed, 0.01);
  const overflow = Math.max(0, needed - budget);
  const used = needed - overflow;
  const free = Math.max(0, budget - used);

  const pct = (value: number) => `${Math.max(0, (value / denominator) * 100)}%`;

  const verdict =
    estimate.fit === "exceeds"
      ? "exceeds"
      : estimate.fit === "tight"
        ? "tight"
        : "comfortable";

  return (
    <div className="memory-bar">
      <div
        className="memory-bar__track"
        role="img"
        aria-label={`Memory estimate: ${gb(needed)} needed of ${gb(budget)} available`}
      >
        <span className="memory-bar__seg memory-bar__seg--weights" style={{ width: pct(weights) }} />
        <span className="memory-bar__seg memory-bar__seg--kv" style={{ width: pct(kv) }} />
        <span className="memory-bar__seg memory-bar__seg--overhead" style={{ width: pct(overhead) }} />
        {overflow > 0 ? (
          <span className="memory-bar__seg memory-bar__seg--over" style={{ width: pct(overflow) }} />
        ) : (
          <span className="memory-bar__seg memory-bar__seg--free" style={{ width: pct(free) }} />
        )}
      </div>

      <div className="memory-bar__scale" aria-hidden="true">
        <span>0 GB</span>
        <span className="mono">
          budget {gb(budget)} / {onGpu ? `${gb(total)} VRAM` : `${gb(total)} RAM`}
        </span>
      </div>

      <div className="memory-bar__legend">
        <span className="memory-bar__key">
          <span className="memory-bar__swatch" style={{ background: "var(--accent-primary)" }} />
          Weights {gb(weights)}
        </span>
        <span className="memory-bar__key">
          <span className="memory-bar__swatch" style={{ background: "var(--accent-research)" }} />
          KV cache {gb(kv)}
        </span>
        <span className="memory-bar__key">
          <span className="memory-bar__swatch" style={{ background: "var(--accent-attention)" }} />
          Overhead {gb(overhead)}
        </span>
        {overflow > 0 ? (
          <span className="memory-bar__key">
            <span className="memory-bar__swatch" style={{ background: "var(--accent-danger)" }} />
            Over budget {gb(overflow)}
          </span>
        ) : (
          <span className="memory-bar__key">
            <span
              className="memory-bar__swatch"
              style={{ background: "color-mix(in srgb, var(--accent-success) 55%, transparent)" }}
            />
            Free {gb(free)}
          </span>
        )}
      </div>

      <span className={`memory-bar__verdict memory-bar__verdict--${verdict}`}>
        {FIT_LABELS[estimate.fit]} · {deviceLabel(estimate.device)} · {gb(needed)} needed
      </span>
    </div>
  );
}
