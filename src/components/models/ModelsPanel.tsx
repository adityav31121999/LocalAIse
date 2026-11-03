/** The Models settings tab: the user's GGUF folders and the models in them.
 *
 * The common layout is `models/llm/<model-name>/<model-name>.gguf`, with the
 * multimodal projector of a vision model sitting in the same `<model-name>`
 * folder. The panel therefore shows folders as groups: the weights of a model
 * and the projector that belongs to them are one thing, because that is how
 * the runtime loads them.
 */

import { useCallback, useEffect, useState } from "react";
import {
  addModelFolder,
  deleteModel,
  getHardwareReport,
  listModelFolders,
  listModelGroups,
  pickModelFolder,
  removeModelFolder,
} from "../../api/models";
import type { HardwareReport, ModelEntry, ModelGroup } from "../../api/models";
import { useRuntimeStore } from "../../state/runtimeStore";
import { FolderIcon, PlayIcon, StopIcon, TrashIcon } from "../ui/icons";

function humanBytes(bytes: number): string {
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(0)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
}

/** Bytes of KV cache for the model at its declared context with an f16 cache,
 * mirroring `budget::kv_cache_bytes` in the core. Only blocks that hold a KV
 * cache price in: hybrid architectures (Mamba + attention mixes) report their
 * attention block count; gemma-style shared blocks reuse the first blocks'
 * cache; sliding-window blocks are bounded by the window with their own
 * (often smaller) head size. */
function kvCacheGb(entry: ModelEntry): number | null {
  if (entry.nLayers === 0 || entry.nKvHeads === 0 || entry.headDim === 0) return null;
  const kvLayers = (entry.nKvLayers ?? 0) > 0
    ? Math.min(entry.nKvLayers ?? entry.nLayers, entry.nLayers)
    : entry.nLayers;
  const shared = Math.min(entry.sharedKvLayers, kvLayers);
  const allocating = entry.sharedKvLayers > 0 ? kvLayers - shared : kvLayers;
  const ctx = entry.maxContext > 0 ? entry.maxContext : 8_192;
  const bpe = 2; // f16
  const window = entry.slidingWindow > 0 ? Math.min(ctx, entry.slidingWindow) : 0;
  const swaLayers = window > 0
    ? Math.min(entry.nSwaLayers ?? allocating, allocating)
    : 0;
  const fullLayers = allocating - swaLayers;
  const swaHeadDim = (entry.swaHeadDim ?? 0) > 0 ? (entry.swaHeadDim ?? entry.headDim) : entry.headDim;
  const elements =
    (fullLayers * ctx * entry.headDim + swaLayers * window * swaHeadDim)
    * entry.nKvHeads * 2;
  return (elements * bpe) / (1024 ** 3);
}

/** The badges that describe a weight file, as far as the file says. */
function FileBadges({ entry }: { entry: ModelEntry }) {
  return (
    <div className="model-file__badges">
      {entry.quantization && <span className="badge">{entry.quantization}</span>}
      {entry.paramsBillion > 0 && (
        <span className="badge badge--budget">{entry.paramsBillion.toFixed(1)}B</span>
      )}
      {entry.architecture && <span className="badge">{entry.architecture}</span>}
    </div>
  );
}

export function ModelsPanel() {
  const [groups, setGroups] = useState<ModelGroup[]>([]);
  const [folders, setFolders] = useState<string[]>([]);
  const [folderDraft, setFolderDraft] = useState("");
  const [hardware, setHardware] = useState<HardwareReport | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const runtime = useRuntimeStore((state) => state.status);
  const loadModel_ = useRuntimeStore((state) => state.loadModel);
  const unloadModel_ = useRuntimeStore((state) => state.unloadModel);
  const loadedPath = runtime.modelPath ?? null;

  const refresh = useCallback(async () => {
    const [groupList, folderList, report] = await Promise.all([
      listModelGroups(),
      listModelFolders(),
      getHardwareReport().catch(() => null),
    ]);
    setGroups(groupList);
    setFolders(folderList);
    if (report) setHardware(report);
  }, []);

  useEffect(() => {
    void refresh().catch(() => setError("Could not read the models folder."));
  }, [refresh]);

  const load = async (path: string) => {
    setBusy(path);
    setError(null);
    try {
      await loadModel_(path);
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      setBusy(null);
    }
  };

  const unload = async () => {
    setBusy("__unload__");
    setError(null);
    try {
      await unloadModel_();
    } catch (unloadError) {
      setError(unloadError instanceof Error ? unloadError.message : String(unloadError));
    } finally {
      setBusy(null);
    }
  };

  const remove = async (path: string) => {
    if (!window.confirm("Remove this model file from the models folder?")) return;
    setBusy(path);
    setError(null);
    try {
      await deleteModel(path);
      await refresh();
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : String(deleteError));
    } finally {
      setBusy(null);
    }
  };

  /** Registers the folder typed into the address bar. */
  const addFolder = async (raw: string) => {
    const folder = raw.trim();
    if (!folder) return;
    setBusy(folder);
    setError(null);
    try {
      await addModelFolder(folder);
      setFolderDraft("");
      await refresh();
    } catch (folderError) {
      setError(folderError instanceof Error ? folderError.message : String(folderError));
    } finally {
      setBusy(null);
    }
  };

  /** Opens the OS folder picker and registers the picked folder. */
  const browseFolder = async () => {
    const picked = await pickModelFolder();
    if (picked) await addFolder(picked);
  };

  const removeFolder = async (folder: string) => {
    setBusy(folder);
    setError(null);
    try {
      await removeModelFolder(folder);
      await refresh();
    } catch (folderError) {
      setError(folderError instanceof Error ? folderError.message : String(folderError));
    } finally {
      setBusy(null);
    }
  };

  const modelCount = groups.reduce((total, group) => total + group.models.length, 0);

  return (
    <div className="models">
      {hardware && (
        <div className="models__machine">
          This machine: <strong>{hardware.recommendedTier}</strong> · {hardware.cpuCores} threads ·{" "}
          {hardware.totalRamGb.toFixed(0)} GB RAM.
        </div>
      )}

      {error && <p className="field__hint perf__error">{error}</p>}

      <section className="field field--compact" aria-labelledby="field-folders">
        <h2 className="field__label" id="field-folders">
          Your models folder
        </h2>
        <div className="models__folderbar input">
          <FolderIcon size={14} />
          <input
            type="text"
            spellCheck={false}
            placeholder="C:\Users\you\Models"
            value={folderDraft}
            onChange={(event) => setFolderDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void addFolder(folderDraft);
            }}
            aria-label="Path to the folder holding your GGUF models"
          />
          <button
            type="button"
            className="btn btn--secondary btn--sm"
            disabled={busy === folderDraft.trim() || !folderDraft.trim()}
            onClick={() => void addFolder(folderDraft)}
          >
            Add
          </button>
          <button type="button" className="btn btn--ghost btn--sm" onClick={() => void browseFolder()}>
            Browse…
          </button>
        </div>
        {folders.length > 0 && (
          <ul className="models__folders">
            {folders.map((folder) => (
              <li key={folder} className="models__folder-chip mono">
                <span className="truncate">{folder}</span>
                <button
                  type="button"
                  className="icon-btn icon-btn--sm"
                  title="Stop using this folder (files are kept)"
                  disabled={busy === folder}
                  onClick={() => void removeFolder(folder)}
                >
                  <TrashIcon size={12} />
                  <span className="sr-only">Stop using this folder</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="field" aria-labelledby="field-installed">
        <h2 className="field__label" id="field-installed">
          Installed models <span className="badge badge--content">{modelCount}</span>
        </h2>
        {groups.length === 0 ? (
          <p className="muted">
            No models found. Download GGUF files from Hugging Face or elsewhere, put them in a
            folder, and add it above — everything runs from here, on this machine.
          </p>
        ) : (
          <div className="model-groups">
            {groups.map((group) => (
              <article className="model-group" key={group.id}>
                <div className="model-group__head">
                  <div className="model-group__identity">
                    <span className="model-group__name">{group.name}</span>
                    <span className="model-group__path mono truncate">{group.dir}</span>
                  </div>
                  <span className="model-group__size mono">{humanBytes(group.totalBytes)}</span>
                </div>

                <ul className="model-group__files">
                  {group.models.map((entry) => {
                    const isLoaded = loadedPath === entry.path;
                    const kv = kvCacheGb(entry);
                    const inFlight = busy !== null;
                    const loadingThis = busy === entry.path;
                    const unloading = busy === "__unload__";
                    return (
                      <li className="model-file" key={entry.path}>
                        <div className="model-file__row">
                          <div className="model-file__main">
                            <div className="model-file__identity">
                              <span className="model-file__name truncate">{entry.name}</span>
                              <span className="model-file__path mono truncate">{entry.dir}</span>
                            </div>
                            <FileBadges entry={entry} />
                          </div>
                          <div className="model-file__side">
                            <div className="model-file__meta">
                              <span className="model-file__size mono">
                                {humanBytes(entry.sizeBytes)}
                              </span>
                              {kv != null && (
                                <span className="model-file__kv mono muted" title={`KV cache at ${entry.maxContext > 0 ? entry.maxContext : 8192}-token context, f16`}>
                                  ≈ {humanBytes(kv * 1024 ** 3)} KV cache
                                </span>
                              )}
                            </div>
                            <div className="model-file__actions">
                              <button
                                type="button"
                                className={`model-file__btn ${isLoaded ? "model-file__btn--danger" : "model-file__btn--load"}`}
                                disabled={inFlight}
                                title={isLoaded ? "Unload the model and stop the runtime" : "Load the model and start the runtime"}
                                onClick={() => (isLoaded ? void unload() : void load(entry.path))}
                              >
                                {isLoaded
                                  ? unloading ? <><StopIcon size={12} /> Unloading…</> : <><StopIcon size={12} /> Unload</>
                                  : loadingThis ? <><PlayIcon size={12} /> Loading…</> : <><PlayIcon size={12} /> Load</>}
                              </button>
                              {group.internal && (
                                <button
                                  type="button"
                                  className="icon-btn icon-btn--sm model-file__trash"
                                  title="Remove from the models folder"
                                  disabled={inFlight}
                                  onClick={() => remove(entry.path)}
                                >
                                  <TrashIcon size={12} />
                                  <span className="sr-only">Remove {entry.name}</span>
                              </button>
                            )}
                          </div>
                        </div>
                      </div>
                      {isLoaded && (
                          <p className="model-file__loaded">
                            <span className="dot dot--success" aria-hidden="true" /> Loaded for chat
                          </p>
                        )}
                      </li>
                    );
                  })}
                </ul>

                {group.projectors.length > 0 && (
                  <div className="model-group__projectors">
                    <span className="model-group__projectors-label">
                      Multimodal projector
                    </span>
                    {group.projectors.map((entry) => (
                      <div className="model-group__projector" key={entry.path}>
                        <span className="truncate">{entry.name}</span>
                        <span className="mono">{humanBytes(entry.sizeBytes)}</span>
                      </div>
                    ))}
                  </div>
                )}
              </article>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}
