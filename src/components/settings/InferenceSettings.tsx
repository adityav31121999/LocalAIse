/** The Inference settings tab: how the model runs.
 *
 * Every control here maps to one llama-server flag. The draft is measured by
 * the core while the user edits, so the numbers beside the controls are the
 * same ones the server will be started with — and never a guess made in the
 * browser.
 */

import { useState } from "react";
import type { KvCacheType } from "../../types";

import { MemoryWarningAlert } from "./MemoryWarningAlert";
import { MemoryBudgetBar } from "./MemoryBudgetBar";
import { NumberField } from "../ui/NumberField";
import { RuntimeActions } from "./RuntimeActions";
import {
  useRuntimeConfig,
} from "./runtime-config";

const KV_OPTIONS: { value: KvCacheType; label: string; hint: string }[] = [
  { value: "f16", label: "f16", hint: "full precision" },
  { value: "q8_0", label: "Q8", hint: "about half the memory" },
  { value: "q5_1", label: "Q5", hint: "three eighths of the memory" },
  { value: "q4_0", label: "Q4", hint: "a quarter of the memory" },
];

function formatTokens(tokens: number): string {
  return tokens.toLocaleString();
}

function gigabytes(value: number): string {
  return value >= 10 ? `${value.toFixed(0)} GB` : `${value.toFixed(2)} GB`;
}

/** The label the KV control shows for a cache type. */
function kvLabel(value: KvCacheType): string {
  return KV_OPTIONS.find((option) => option.value === value)?.label ?? value;
}

/** The file name of a model, without its extension. */

export function InferenceSettings() {
  const {
    draft,
    estimate,
    hardware,
    patch,
    setKvType,
    setMoe,
  } = useRuntimeConfig();
  const [expertLayers, setExpertLayers] = useState(4);

  const onGpu = estimate?.device.device === "user_gpu";
  const quantized = draft.kvType !== "f16";

  // What the same window costs under every cache type, so the control can
  // price each option. Absent until the model file has been parsed.
  const kvCosts = estimate?.modelShapeKnown ? (estimate.kvCacheGbByType ?? {}) : {};
  const kvCost = (value: KvCacheType): number | undefined => kvCosts[value];

  // The model declares a context length; the input may still hold an older,
  // larger value — the readout below shows what the model supports.
  const declared = estimate?.maxContext ?? 0;

  // The window the server will run with. The core works it out — it knows
  // what the model declares and what the memory allows — so the number here
  // is the number the server gets, not the number the input happens to hold.
  // Without an estimate only the draft is known.
  const runtimeContext = estimate
    ? estimate.contextTokens
    : draft.contextTokens === 0
      ? 0
      : draft.contextTokens;

  return (
    <div className="perf">
      <MemoryWarningAlert />
      <section className="field" aria-labelledby="field-context">
        <h2 className="field__label" id="field-context">
          Context window &amp; KV cache
        </h2>
        <p className="field__hint">Max text the model sees at once; 0 = model default.</p>
        <div className="perf__slider-row" style={{ gap: "var(--space-4)", flexWrap: "wrap" }}>
          <NumberField
            className="input"
            style={{ maxWidth: "140px" }}
            min={0}
            step={1024}
            value={draft.contextTokens}
            onChange={(n) => {
              patch({ contextTokens: n === 0 ? 0 : Math.max(512, n) });
            }}
            label="Context window in tokens"
          />
          <div className="chip-row chip-row--compact" role="group" aria-label="Context window presets">
            {[4_096, 8_192, 16_384, 32_768]
              .filter((preset) => declared === 0 || preset <= declared)
              .map((preset) => (
                <button
                  key={preset}
                  type="button"
                  className="chip"
                  aria-pressed={draft.contextTokens === preset}
                  onClick={() => patch({ contextTokens: preset })}
                >
                  {preset >= 1024 ? `${preset / 1024}K` : String(preset)}
                </button>
              ))}
            {declared > 0 ? (
              <button
                type="button"
                className="chip"
                aria-pressed={draft.contextTokens === declared}
                onClick={() => patch({ contextTokens: declared })}
                title={`The model's own limit: ${formatTokens(declared)} tokens`}
              >
                {formatTokens(declared)} · model max
              </button>
            ) : null}
            <button
              type="button"
              className="chip"
              aria-pressed={draft.contextTokens === 0}
              onClick={() => patch({ contextTokens: 0 })}
              title="Let the model use its own default context"
            >
              Auto
            </button>
          </div>
        </div>

        <h3 id="field-kv">KV cache type</h3>
        <p className="field__hint">Quantized cache trades slight quality for memory savings.</p>
        <div className="segmented" role="group" aria-labelledby="field-kv">
          {KV_OPTIONS.map((option) => {
            const cost = kvCost(option.value);
            return (
              <button
                key={option.value}
                type="button"
                className="segmented__item"
                aria-pressed={draft.kvType === option.value}
                onClick={() => setKvType(option.value)}
              >
                {option.label} · {cost != null ? `≈ ${gigabytes(cost)}` : option.hint}
              </button>
            );
          })}
        </div>
        {/* Memory calculator below the quantisation option — uses remaining space. */}
        <div className="memory-calculator">
          <div className="memory-calculator__rows">
            <div className="memory-calculator__row">
              <span className="memory-calculator__key">Runtime context</span>
              <span className="memory-calculator__value mono">
                {runtimeContext === 0
                  ? "Model default"
                  : `${formatTokens(runtimeContext)} tokens`}
              </span>
            </div>
            <div className="memory-calculator__row">
              <span className="memory-calculator__key">
                KV cache ({kvLabel(draft.kvType)}, estimated)
              </span>
              <span className="memory-calculator__value mono">
                {estimate && estimate.modelShapeKnown
                  ? `≈ ${gigabytes(estimate.kvCacheGb)}`
                  : "not known yet"}
              </span>
            </div>
            <div className="memory-calculator__row">
              <span className="memory-calculator__key">Total memory used</span>
              <span className="memory-calculator__value mono">
                {estimate && estimate.modelShapeKnown
                  ? onGpu
                    ? `≈ ${gigabytes(estimate.vramNeededGb)} VRAM + ${gigabytes(estimate.ramNeededGb)} RAM`
                    : `≈ ${gigabytes(estimate.ramNeededGb)} RAM`
                  : "not known yet"}
              </span>
            </div>
            <div className="memory-calculator__row">
              <span className="memory-calculator__key">Model declares</span>
              <span className="memory-calculator__value mono">
                {declared > 0 ? `${formatTokens(declared)} tokens` : "no limit in the file"}
              </span>
            </div>
          </div>
        </div>

        <label className="perf__check">
          <input
            type="checkbox"
            checked={quantized ? true : draft.flashAttn}
            disabled={quantized}
            onChange={(event) => patch({ flashAttn: event.target.checked })}
          />
          <span>
            Flash attention
            {quantized ? " (required with a quantized cache)" : ""}
          </span>
        </label>

        {/* Flash attention is a property of the build, not of the app: the
            core asks the server what it can do. */}
      </section>

      <section className="field" aria-labelledby="field-budgets">
        <h2 className="field__label" id="field-budgets">
          Token budgets
        </h2>
        <p className="field__hint">Per-role caps for prompt, response, and thinking tokens.</p>
        <div className="limits-table">
          <span className="limits-table__cell--row">All roles</span>
          <span className="limits-table__cell--head">Max prompt</span>
          <span className="limits-table__cell--head">Max response</span>
          <span className="limits-table__cell--head">Thinking</span>

          <span className="limits-table__cell--row" aria-hidden="true" />
          <NumberField
            className="input"
            min={0}
            step={256}
            value={draft.maxInputTokens ?? 0}
            onChange={(n) => patch({ maxInputTokens: n })}
            label="Max prompt tokens (also used for summarization)"
          />
          <NumberField
            className="input"
            min={0}
            step={256}
            value={draft.maxOutputTokens ?? 0}
            onChange={(n) => patch({ maxOutputTokens: n })}
            label="Max response tokens (also used for summarization)"
          />
          <NumberField
            className="input"
            min={0}
            step={128}
            value={draft.thinkingBudget}
            onChange={(n) => patch({ thinkingBudget: n })}
            label="Thinking budget"
          />
        </div>
      </section>

      <section className="field" aria-labelledby="field-threads">
        <h2 className="field__label" id="field-threads">
          CPU threads
        </h2>
        <p className="field__hint">Auto leaves one core free for the interface.</p>
        <div className="perf__slider-row">
          <label className="perf__check">
            <input
              type="checkbox"
              checked={draft.threads === 0}
              onChange={(event) =>
                patch({
                  threads: event.target.checked ? 0 : Math.max(1, (hardware?.cpuCores ?? 4) - 1),
                })
              }
            />
            <span>Auto</span>
          </label>
          {draft.threads > 0 && (
            <NumberField
              className="input perf__threads"
              min={1}
              max={hardware?.cpuCores ?? 64}
              value={draft.threads}
              onChange={(n) => patch({ threads: n })}
              label="Thread count"
            />
          )}
        </div>
      </section>

      <section className="field" aria-labelledby="field-loading">
        <h2 className="field__label" id="field-loading">
          Loading
        </h2>
        <p className="field__hint">Partial loading and memory-mapped weight files.</p>
        <label className="perf__check">
          <input
            type="checkbox"
            checked={draft.partialLoad}
            onChange={(event) => patch({ partialLoad: event.target.checked })}
          />
          <span>
            Partial loading
          </span>
        </label>
        <label className="perf__check">
          <input
            type="checkbox"
            checked={draft.useMmap}
            onChange={(event) => patch({ useMmap: event.target.checked })}
          />
          <span>
            Memory-mapped weights
          </span>
        </label>
      </section>

      {estimate?.isMoe && (
        <section className="field" aria-labelledby="field-moe">
          <h2 className="field__label" id="field-moe">
            Expert tensors
          </h2>
          <p className="field__hint">MoE tensor placement across VRAM and RAM.</p>
          {onGpu ? (
            <>
              <div className="segmented" role="group" aria-labelledby="field-moe">
                <button
                  type="button"
                  className="segmented__item"
                  aria-pressed={draft.moe.mode === "off"}
                  onClick={() => setMoe("off")}
                >
                  In VRAM
                </button>
                <button
                  type="button"
                  className="segmented__item"
                  aria-pressed={draft.moe.mode === "all_experts"}
                  onClick={() => setMoe("all_experts")}
                >
                  In RAM
                </button>
                <button
                  type="button"
                  className="segmented__item"
                  aria-pressed={draft.moe.mode === "first_layers"}
                  onClick={() => {
                    setMoe("first_layers");
                    patch({ moe: { mode: "first_layers", n: expertLayers } });
                  }}
                >
                  First layers in RAM
                </button>
              </div>
              {draft.moe.mode === "first_layers" && (
                <div className="perf__slider-row">
                  <NumberField
                    className="input perf__threads"
                    min={1}
                    step={1}
                    value={draft.moe.n}
                    onChange={(count) => {
                      setExpertLayers(count);
                      patch({ moe: { mode: "first_layers", n: count } });
                    }}
                    label="Number of layers whose experts stay in system RAM"
                  />
                </div>
              )}
            </>
          ) : null}
        </section>
      )}

      <section className="field" aria-labelledby="field-memory">
        <h2 className="field__label" id="field-memory">
          Memory estimate
        </h2>
        <p className="field__hint">Live VRAM/RAM breakdown for the current draft.</p>
        {estimate?.modelShapeKnown ? (
          <>
            <MemoryBudgetBar estimate={estimate} />
            <details className="memory-details">
              <summary />
              <div className="memory-table">
                <span className="memory-table__key">Model weights</span>
                <span className="memory-table__value">{estimate.modelFileGb.toFixed(2)} GB</span>

                <span className="memory-table__key">
                  KV cache ({kvLabel(draft.kvType)}, {formatTokens(runtimeContext)} tokens)
                </span>
                <span className="memory-table__value">{estimate.kvCacheGb.toFixed(2)} GB</span>

                <span className="memory-table__key">Runtime overhead</span>
                <span className="memory-table__value">{estimate.runtimeOverheadGb.toFixed(2)} GB</span>

                <span className="memory-table__key">Estimated need</span>
                <span className="memory-table__value memory-table__value--strong">
                  {onGpu
                    ? `${estimate.vramNeededGb.toFixed(2)} GB VRAM + ${estimate.ramNeededGb.toFixed(2)} GB RAM`
                    : `${estimate.ramNeededGb.toFixed(2)} GB`}
                </span>

                <span className="memory-table__key">Device budget</span>
                <span className="memory-table__value">
                  {onGpu && estimate.vramKnown
                    ? `${estimate.vramBudgetGb.toFixed(2)} GB VRAM · ${estimate.ramBudgetGb.toFixed(2)} GB RAM`
                    : `${estimate.ramBudgetGb.toFixed(2)} GB`}
                </span>

                <span className="memory-table__key">Free right now</span>
                <span className="memory-table__value">
                  {estimate.ramAvailableGb.toFixed(2)} GB RAM
                  {onGpu && estimate.vramKnown ? " · VRAM not measured live" : undefined}
                </span>
              </div>
            </details>
          </>
        ) : (
          <p className="muted">
            {estimate
              ? "The model file has not been parsed yet, so no estimate is possible."
              : "No estimate available (no model selected yet)."}
          </p>
        )}
      </section>

      <RuntimeActions />
    </div>
  );
}
