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
  getSelectedModel,
  listModelFolders,
  listModelGroups,
  pickModelFolder,
  removeModelFolder,
  selectModel,
} from "../../api/models";
import type { HardwareReport, ModelEntry, ModelGroup } from "../../api/models";
import { FolderIcon, TrashIcon } from "../ui/icons";

function humanBytes(bytes: number): string {
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(0)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
}

/** The badges that describe a weight file, as far as the file says. */
function FileBadges({ entry }: { entry: ModelEntry }) {
  return (
    <span className="model-group__badges">
      {entry.quantization && <span className="badge">{entry.quantization}</span>}
      {entry.paramsBillion > 0 && (
        <span className="badge">{entry.paramsBillion.toFixed(1)}B</span>
      )}
      {entry.architecture && <span className="badge">{entry.architecture}</span>}
      {entry.hasProfile && <span className="badge badge--general">Own settings</span>}
    </span>
  );
}

export function ModelsPanel() {
  const [groups, setGroups] = useState<ModelGroup[]>([]);
  const [folders, setFolders] = useState<string[]>([]);
  const [folderDraft, setFolderDraft] = useState("");
  const [hardware, setHardware] = useState<HardwareReport | null>(null);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const [groupList, folderList, selected, report] = await Promise.all([
      listModelGroups(),
      listModelFolders(),
      getSelectedModel(),
      getHardwareReport().catch(() => null),
    ]);
    setGroups(groupList);
    setFolders(folderList);
    setSelectedPath(selected);
    if (report) setHardware(report);
  }, []);

  useEffect(() => {
    void refresh().catch(() => setError("Could not read the models folder."));
  }, [refresh]);

  const choose = async (path: string) => {
    setBusy(path);
    setError(null);
    try {
      await selectModel(path);
      setSelectedPath(path);
    } catch (chooseError) {
      setError(chooseError instanceof Error ? chooseError.message : String(chooseError));
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
        <p className="models__machine">
          This machine: <strong>{hardware.recommendedTier}</strong> · {hardware.cpuCores} threads ·{" "}
          {hardware.totalRamGb.toFixed(0)} GB RAM.
        </p>
      )}

      {error && <p className="field__hint perf__error">{error}</p>}

      <section className="field" aria-labelledby="field-folders">
        <h2 className="field__label" id="field-folders">
          Your models folder
        </h2>
        <p className="field__hint">
          Keep the GGUF files you downloaded anywhere on this machine — type the folder path below
          or pick it with Browse. Every GGUF inside it shows up, including the ones in sub-folders
          such as <span className="mono">llm/&lt;model-name&gt;/</span>.
        </p>
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
          Installed models <span className="badge">{modelCount}</span>
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
                    const isActive = selectedPath === entry.path;
                    return (
                      <li className="model-group__file" key={entry.path}>
                        <div className="model-group__file-main">
                          <span className="model-group__file-name truncate">{entry.name}</span>
                          <FileBadges entry={entry} />
                        </div>
                        <div className="model-group__file-side">
                          <span className="model-group__file-size mono">
                            {humanBytes(entry.sizeBytes)}
                          </span>
                          {isActive ? (
                            <span className="badge badge--local">Active</span>
                          ) : (
                            <button
                              type="button"
                              className="btn btn--secondary btn--sm"
                              disabled={busy === entry.path}
                              onClick={() => choose(entry.path)}
                            >
                              {busy === entry.path ? "Selecting…" : "Use for chat"}
                            </button>
                          )}
                          {group.internal && (
                            <button
                              type="button"
                              className="icon-btn icon-btn--sm"
                              title="Remove from the models folder"
                              disabled={busy === entry.path}
                              onClick={() => remove(entry.path)}
                            >
                              <TrashIcon size={13} />
                              <span className="sr-only">Remove {entry.name}</span>
                            </button>
                          )}
                        </div>
                      </li>
                    );
                  })}
                </ul>

                {group.projectors.length > 0 && (
                  <div className="model-group__projectors">
                    <span className="model-group__projectors-label">
                      Loaded with the weights
                    </span>
                    {group.projectors.map((entry) => (
                      <div className="model-group__projector" key={entry.path}>
                        <span className="truncate">{entry.name}</span>
                        <span className="mono">{humanBytes(entry.sizeBytes)}</span>
                      </div>
                    ))}
                    <p className="field__hint">
                      The projector gives this model its eyes — it is passed to llama-server next to
                      the weights.
                    </p>
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
