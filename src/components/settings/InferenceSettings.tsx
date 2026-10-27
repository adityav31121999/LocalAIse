/** The Inference settings tab: how the model runs.
 *
 * Every control here maps to one llama-server flag. The draft is measured by
 * the core while the user edits, so the numbers beside the controls are the
 * same ones the server will be started with — and never a guess made in the
 * browser.
 */

import { useState } from "react";
import type { KvCacheType, ProfileScope } from "../../types";
import { RuntimeActions } from "./RuntimeActions";
import { MemoryWarningAlert } from "./MemoryWarningAlert";
import { NumberField } from "../ui/NumberField";
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
  const [expertLayers, setExpertLayers] = useState(4);

  const fit = estimate?.fit ?? "comfortable";
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

  const flashSupport = capabilities?.flashAttn ?? null;

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
      <MemoryWarningAlert />
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
        <div className="perf__slider-row" style={{ gap: "var(--space-2)" }}>
          <NumberField
            className="input"
            style={{ maxWidth: "140px" }}
            min={0}
            step={1024}
            value={draft.contextTokens}
            onChange={(n) => {
              // 0 is "model default"; anything else is at least a 512-token
              // window, the smallest the core accepts.
              patch({ contextTokens: n === 0 ? 0 : Math.max(512, n) });
            }}
            label="Context window in tokens"
          />
          <span className="field__hint">
            {draft.contextTokens === 0 ? "0 = model default" : "tokens"}
          </span>
          {[4_096, 8_192, 16_384, 32_768].map((preset) => (
            <button
              key={preset}
              type="button"
              className="btn btn--ghost btn--sm"
              aria-pressed={draft.contextTokens === preset}
              onClick={() => patch({ contextTokens: preset })}
            >
              {preset >= 1024 ? `${preset / 1024}K` : String(preset)}
            </button>
          ))}
        </div>

        {/* The estimate follows the window chosen above: how much cache it
            needs, and whether that fits on this machine. */}
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

      <section className="field" aria-labelledby="field-budgets">
        <h2 className="field__label" id="field-budgets">
          Token budgets
        </h2>
        <p className="field__hint">
          Caps per role, in tokens. 0 means unlimited — the model's own server settings decide.
          The Model row also controls document summarization. The Agent row governs tool-using chat,
          the Subagent row the map-reduce workers.
        </p>
        <div className="limits-table">
          <span className="limits-table__cell--head" aria-hidden="true" />
          <span className="limits-table__cell--head">Max input</span>
          <span className="limits-table__cell--head">Max output</span>
          <span className="limits-table__cell--head">Thinking</span>

          <span className="limits-table__cell--row">Model</span>
          <NumberField
            className="input"
            min={0}
            step={256}
            value={draft.maxInputTokens ?? 0}
            onChange={(n) => patch({ maxInputTokens: n })}
            label="Max input tokens for the model (also used for summarization)"
          />
          <NumberField
            className="input"
            min={0}
            step={256}
            value={draft.maxOutputTokens ?? 0}
            onChange={(n) => patch({ maxOutputTokens: n })}
            label="Max output tokens for the model (also used for summarization)"
          />
          <NumberField
            className="input"
            min={0}
            step={128}
            value={draft.thinkingBudget}
            onChange={(n) => patch({ thinkingBudget: n })}
            label="Thinking budget for the model"
          />

          <span className="limits-table__cell--row">Agent</span>
          <NumberField
            className="input"
            min={0}
            step={256}
            value={draft.agentMaxInputTokens ?? 0}
            onChange={(n) => patch({ agentMaxInputTokens: n })}
            label="Max input tokens for the agent"
          />
          <NumberField
            className="input"
            min={0}
            step={256}
            value={draft.agentMaxOutputTokens ?? 0}
            onChange={(n) => patch({ agentMaxOutputTokens: n })}
            label="Max output tokens for the agent"
          />
          <NumberField
            className="input"
            min={0}
            step={128}
            value={draft.agentThinkingBudget ?? 0}
            onChange={(n) => patch({ agentThinkingBudget: n })}
            label="Thinking budget for the agent"
          />

          <span className="limits-table__cell--row">Subagent</span>
          <NumberField
            className="input"
            min={0}
            step={256}
            value={draft.subagentMaxInputTokens ?? 0}
            onChange={(n) => patch({ subagentMaxInputTokens: n })}
            label="Max input tokens for the subagent"
          />
          <NumberField
            className="input"
            min={0}
            step={256}
            value={draft.subagentMaxOutputTokens ?? 0}
            onChange={(n) => patch({ subagentMaxOutputTokens: n })}
            label="Max output tokens for the subagent"
          />
          <NumberField
            className="input"
            min={0}
            step={128}
            value={draft.subagentThinkingBudget ?? 0}
            onChange={(n) => patch({ subagentThinkingBudget: n })}
            label="Thinking budget for the subagent"
          />
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
              context automatically instead of failing to load. The window input
              becomes a minimum.
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
              {onGpu && estimate.vramKnown
                ? " · VRAM not measured live"
                : undefined}
            </span>
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
