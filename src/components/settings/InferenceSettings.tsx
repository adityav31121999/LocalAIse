/** The Inference settings tab: how the model runs.
 *
 * Every control here maps to one llama-server flag. The draft is measured by
 * the core while the user edits, so the numbers next to the sliders are the
 * same ones the server will be started with — and never a guess made in the
 * browser.
 */

import { useState } from "react";
import type { KvCacheType, ProfileScope } from "../../types";
import { RuntimeActions } from "./RuntimeActions";
import {
  FIT_LABELS,
  deviceLabel,
  effectiveThreads,
  sameProfile,
  useRuntimeConfig,
} from "./runtime-config";
import { DEFAULT_PROFILE } from "../../api/models";

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
function modelName(path: string | null): string {
  if (!path) return "";
  const parts = path.split(/[\\/]/).filter(Boolean);
  const last = parts[parts.length - 1] ?? path;
  return last.replace(/\.gguf$/i, "");
}

export function InferenceSettings() {
  const {
    draft,
    estimate,
    capabilities,
    hardware,
    scope,
    storage,
    effectiveScope,
    modelPath,
    projectId,
    projectName,
    setScope,
    clearScope,
    applying,
    patch,
    setKvType,
    setMoe,
  } = useRuntimeConfig();
  const [budgetMax, setBudgetMax] = useState(16_384);
  const [expertLayers, setExpertLayers] = useState(4);

  const fit = estimate?.fit ?? "comfortable";
  const onGpu = estimate?.device.device === "user_gpu";
  const quantized = draft.kvType !== "f16";

  // What the same window costs under every cache type, so the control can
  // price each option. Absent until the model file has been parsed.
  const kvCosts = estimate?.modelShapeKnown ? (estimate.kvCacheGbByType ?? {}) : {};
  const kvCost = (value: KvCacheType): number | undefined => kvCosts[value];

  // The model declares a context length; the slider cannot offer more than
  // that, but the draft may still hold an older, larger value.
  const declared = estimate?.maxContext ?? 0;
  const ctxMax = declared > 0 ? declared : 131_072;
  const sliderValue = Math.min(draft.contextTokens, ctxMax);

  // The window the server will run with. The core works it out — it knows
  // what the model declares and what the memory allows — so the number here
  // is the number the server gets, not the number the slider happens to sit
  // on. Without an estimate only the draft is known.
  const runtimeContext = estimate
    ? estimate.contextTokens
    : draft.contextTokens === 0
      ? 0
      : draft.contextTokens;

  const flashSupport = capabilities?.flashAttn ?? null;

  const ramPercent = estimate
    ? Math.min(100, Math.round((estimate.ramNeededGb / Math.max(estimate.ramTotalGb, 1)) * 100))
    : 0;
  const vramPercent =
    estimate && estimate.vramKnown && estimate.vramTotalGb > 0
      ? Math.min(100, Math.round((estimate.vramNeededGb / estimate.vramTotalGb) * 100))
      : 0;

  // The three places a configuration can live. The more specific one wins:
  // a project sits on top of a model, and a model on top of the default.
  const scopeOptions: { id: ProfileScope; label: string; available: boolean }[] = [
    { id: "global", label: "Every model", available: true },
    { id: "model", label: "This model", available: Boolean(modelPath) },
    { id: "project", label: "This project", available: Boolean(projectId) },
  ];

  const scopeHint =
    scope === "global"
      ? "The default every model starts from. A model or a project can replace it."
      : scope === "model"
        ? modelPath
          ? `Only ${modelName(modelPath)} uses these settings — every other model keeps the default.`
          : "No model is selected yet, so there is nothing to configure for one."
        : projectName
          ? `Only ${projectName} uses these settings — every other project keeps the default.`
          : "No project is open, so there is nothing to configure for one.";

  // A more specific scope is in effect while a looser one is being edited —
  // a stored project profile governs the model, a stored model or project
  // profile governs the app-wide default. Only then do changes here not
  // reach the runtime yet.
  const shadowed =
    storage !== null &&
    ((scope === "global" && (storage.model != null || storage.project != null)) ||
      (scope === "model" && storage.project != null));

  const canClear =
    scope === "global"
      ? storage !== null && !sameProfile(storage.global, DEFAULT_PROFILE)
      : scope === "model"
        ? storage?.model != null
        : storage?.project != null;

  const clearLabel =
    scope === "global"
      ? "Restore the app default"
      : scope === "model"
        ? "Use the default for every model"
        : "Use the inherited settings";

  return (
    <div className="perf">
      <section className="field" aria-labelledby="field-scope">
        <h2 className="field__label" id="field-scope">
          Applies to
        </h2>
        <div className="segmented" role="group" aria-labelledby="field-scope">
          {scopeOptions.map((option) => (
            <button
              key={option.id}
              type="button"
              className="segmented__item"
              aria-pressed={scope === option.id}
              disabled={!option.available}
              onClick={() => setScope(option.id)}
            >
              {option.label}
            </button>
          ))}
        </div>
        <p className="field__hint">{scopeHint}</p>
        {shadowed && (
          <p className="field__hint">
            Right now the{" "}
            {effectiveScope === "project" ? "project" : effectiveScope === "model" ? "model" : "default"}{" "}
            settings are in effect, so changes here only apply once that one is removed.
          </p>
        )}
        {canClear && (
          <button
            type="button"
            className="btn btn--ghost btn--sm"
            disabled={applying}
            onClick={() => void clearScope()}
          >
            {clearLabel}
          </button>
        )}
      </section>

      <section className="field" aria-labelledby="field-context">
        <h2 className="field__label" id="field-context">
          Context window
        </h2>
        <p className="field__hint">
          How much text the model can see at once — files pasted into chat, long citations and
          multi-turn research all count against this. 0 lets the model use its own default context.
        </p>
        <div className="perf__slider-row">
          <input
            className="slider"
            type="range"
            min={0}
            max={ctxMax}
            step={512}
            value={sliderValue}
            onChange={(event) => {
              const raw = Number(event.target.value);
              // 0 is "model default"; small hands-on values round up to the
              // per-profile floor instead of failing validation later.
              patch({ contextTokens: raw === 0 ? 0 : Math.max(512, raw) });
            }}
            aria-labelledby="field-context"
          />
          <span className="perf__slider-value mono">
            {draft.contextTokens === 0 ? "Model default" : formatTokens(draft.contextTokens)}
          </span>
        </div>

        {/* The estimate sits beside the slider: how much cache the chosen
            window needs, and whether that fits on this machine. */}
        <div className="ctx-readout">
          <div className="ctx-readout__row">
            <span className="ctx-readout__key">Runtime context</span>
            <span className="ctx-readout__value mono">
              {runtimeContext === 0
                ? "Model default"
                : `${formatTokens(runtimeContext)} tokens`}
            </span>
          </div>
          <div className="ctx-readout__row">
            <span className="ctx-readout__key">KV cache ({kvLabel(draft.kvType)}, estimated)</span>
            <span className="ctx-readout__value mono">
              {estimate && estimate.modelShapeKnown
                ? `≈ ${gigabytes(estimate.kvCacheGb)}`
                : "not known yet"}
            </span>
          </div>
          <div className="ctx-readout__row">
            <span className="ctx-readout__key">Total memory used</span>
            <span className="ctx-readout__value mono">
              {estimate && estimate.modelShapeKnown
                ? onGpu
                  ? `≈ ${gigabytes(estimate.vramNeededGb)} VRAM + ${gigabytes(estimate.ramNeededGb)} RAM`
                  : `≈ ${gigabytes(estimate.ramNeededGb)} RAM`
                : "not known yet"}
            </span>
          </div>
          <div className="ctx-readout__row">
            <span className="ctx-readout__key">Model declares</span>
            <span className="ctx-readout__value mono">
              {declared > 0 ? `${formatTokens(declared)} tokens` : "no limit in the file"}
            </span>
          </div>
          <p className={`ctx-readout__fit ctx-readout__fit--${fit}`}>{FIT_LABELS[fit]}</p>
          <p className="ctx-readout__note">
            Estimates are approximate — they are worked out from the model file and the free memory,
            not measured from a running server.
          </p>
        </div>
      </section>

      <section className="field" aria-labelledby="field-kv">
        <h2 className="field__label" id="field-kv">
          KV cache type
        </h2>
        <p className="field__hint">
          A quantized cache keeps long contexts affordable on CPU-sized memory. It costs a little
          answer quality, and it only works together with flash attention.
        </p>
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
        {estimate?.modelShapeKnown && kvCost(draft.kvType) != null && kvCost("f16") != null && (
          <p className="field__hint">
            Cache at {formatTokens(runtimeContext)} tokens.{" "}
            {draft.kvType === "f16"
              ? "Every quantized type below trades a little quality for memory."
              : `${kvLabel(draft.kvType)} saves ${gigabytes((kvCost("f16") ?? 0) - (kvCost(draft.kvType) ?? 0))} of cache versus f16 (${gigabytes(kvCost("f16") ?? 0)}).`}
          </p>
        )}

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
        {flashSupport === false && (
          <p className="field__hint perf__error">
            The llama-server build in use does not offer flash attention. Turn it off, or point the
            app at a build that supports it.
          </p>
        )}
        {flashSupport === null && (
          <p className="field__hint">
            The build did not answer when it was asked about flash attention, so the app cannot
            promise that the setting takes effect.
          </p>
        )}
        {flashSupport === true && (
          <p className="field__hint">This build supports flash attention.</p>
        )}
      </section>

      <section className="field" aria-labelledby="field-thinking">
        <h2 className="field__label" id="field-thinking">
          Thinking budget
        </h2>
        <p className="field__hint">
          How many tokens a thinking model may spend reasoning before it answers. Auto lets the
          model decide; a limit trades deeper reasoning for earlier answers.
        </p>
        <div className="perf__slider-row">
          <input
            className="slider"
            type="range"
            min={0}
            max={budgetMax}
            step={128}
            value={Math.min(draft.thinkingBudget, budgetMax)}
            onChange={(event) => {
              const budget = Number(event.target.value);
              // 0 is "auto (unrestricted)"; anything else is a hard cap.
              patch({ thinkingBudget: budget === 0 ? 0 : Math.max(1, budget) });
            }}
            aria-labelledby="field-thinking"
          />
          <span className="perf__slider-value mono">
            {draft.thinkingBudget === 0 ? "Auto" : `${formatTokens(draft.thinkingBudget)} tokens`}
          </span>
        </div>
        <div className="perf__slider-row">
          <span className="field__hint">Slider max:</span>
          <input
            className="input perf__threads"
            type="number"
            min={1}
            step={128}
            value={budgetMax}
            onChange={(event) => {
              const max = Math.max(1, Number(event.target.value) || 1);
              setBudgetMax(max);
              if (draft.thinkingBudget > max) patch({ thinkingBudget: max });
            }}
            aria-label="Maximum thinking budget shown on the slider"
          />
          <span className="field__hint mono">tokens</span>
        </div>
      </section>

      <section className="field" aria-labelledby="field-threads">
        <h2 className="field__label" id="field-threads">
          CPU threads
        </h2>
        <p className="field__hint">
          Auto leaves one core free for the interface. Resolved right now:{" "}
          {effectiveThreads(draft, hardware?.cpuCores)} threads.
        </p>
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
            <input
              className="input perf__threads"
              type="number"
              min={1}
              max={hardware?.cpuCores ?? 64}
              value={draft.threads}
              onChange={(event) => patch({ threads: Math.max(1, Number(event.target.value) || 1) })}
              aria-label="Thread count"
            />
          )}
        </div>
      </section>

      <section className="field" aria-labelledby="field-loading">
        <h2 className="field__label" id="field-loading">
          Loading
        </h2>
        <label className="perf__check">
          <input
            type="checkbox"
            checked={draft.partialLoad}
            onChange={(event) => patch({ partialLoad: event.target.checked })}
          />
          <span>
            Partial loading
            <span className="field__hint">
              {" "}— when the context bloats the KV cache past what fits, llama-server lowers the
              context automatically instead of failing to load. The slider above becomes a minimum.
            </span>
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
            <span className="field__hint">
              {" "}— the model file is read straight from disk, which keeps start-up fast. Turning
              it off loads the whole file into memory.
            </span>
          </span>
        </label>
      </section>

      {estimate?.isMoe && (
        <section className="field" aria-labelledby="field-moe">
          <h2 className="field__label" id="field-moe">
            Expert tensors
          </h2>
          <p className="field__hint">
            This model is mixture-of-experts: most of its parameters sit in expert feed-forward
            tensors, and only a few of them are used per token.
          </p>
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
                  <span className="field__hint">Layers in RAM:</span>
                  <input
                    className="input perf__threads"
                    type="number"
                    min={1}
                    step={1}
                    value={draft.moe.n}
                    onChange={(event) => {
                      const count = Math.max(1, Number(event.target.value) || 1);
                      setExpertLayers(count);
                      patch({ moe: { mode: "first_layers", n: count } });
                    }}
                    aria-label="Number of layers whose experts stay in system RAM"
                  />
                </div>
              )}
              <p className="field__hint">
                Keeping experts in system RAM frees VRAM for the cache but slows generation
                somewhat.
              </p>
            </>
          ) : (
            <p className="field__hint">
              On the CPU every tensor already lives in system RAM, so there is nothing to move. Turn
              on the GPU in the hardware settings to place the experts separately.
            </p>
          )}
        </section>
      )}

      <section className="field" aria-labelledby="field-memory">
        <h2 className="field__label" id="field-memory">
          Memory estimate
        </h2>
        <p className="field__hint">
          Follows the controls above, live. Measured against{" "}
          {estimate ? deviceLabel(estimate.device) : "the selected device"}.
        </p>
        {estimate?.modelShapeKnown ? (
          <div className="memory">
            {onGpu && estimate.vramKnown && (
              <>
                <div className="memory__scale">
                  <span>0 GB</span>
                  <span>{estimate.vramTotalGb.toFixed(1)} GB VRAM</span>
                </div>
                <div
                  className="memory__bar"
                  role="meter"
                  aria-valuemin={0}
                  aria-valuemax={Math.round(estimate.vramTotalGb)}
                  aria-valuenow={Math.round(estimate.vramNeededGb)}
                  aria-label="VRAM needed for this profile"
                >
                  <div
                    className={`memory__fill${fit !== "comfortable" ? " memory__fill--tight" : ""}`}
                    style={{ width: `${vramPercent}%` }}
                  />
                </div>
              </>
            )}
            <div className="memory__scale">
              <span>0 GB</span>
              <span>{estimate.ramTotalGb.toFixed(1)} GB RAM</span>
            </div>
            <div
              className="memory__bar"
              role="meter"
              aria-valuemin={0}
              aria-valuemax={Math.round(estimate.ramTotalGb)}
              aria-valuenow={Math.round(estimate.ramNeededGb)}
              aria-label="RAM needed for this profile"
            >
              <div
                className={`memory__fill${fit !== "comfortable" ? " memory__fill--tight" : ""}`}
                style={{ width: `${ramPercent}%` }}
              />
            </div>
            <div className="memory__detail">
              <span>
                Model <strong>{estimate.modelFileGb.toFixed(1)}</strong> GB + KV cache{" "}
                <strong>{estimate.kvCacheGb.toFixed(2)}</strong> GB + runtime{" "}
                <strong>{estimate.runtimeOverheadGb.toFixed(2)}</strong> GB
              </span>
              <span>
                ≈ <strong>{estimate.ramNeededGb.toFixed(1)}</strong> of{" "}
                <strong>{estimate.ramTotalGb.toFixed(1)}</strong> GB RAM ·{" "}
                {estimate.ramAvailableGb.toFixed(1)} GB free right now
              </span>
              {onGpu && (
                <span>
                  VRAM ≈ <strong>{estimate.vramNeededGb.toFixed(1)}</strong>
                  {estimate.vramKnown ? ` of ${estimate.vramTotalGb.toFixed(1)}` : " (size unknown)"}{" "}
                  GB
                </span>
              )}
            </div>
          </div>
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
