import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ChevronRightIcon,
  CopyIcon,
  DocIcon,
  FilePlusIcon,
  FolderIcon,
  FolderPlusIcon,
  PencilIcon,
  TrashIcon,
} from "../ui/icons";
import { copyEntry, createEntry, deleteEntry, listDirectory, renameEntry } from "../../api/files";
import { useProjectEvents } from "../../hooks/useProjectEvents";
import { formatBytes, formatRelativeTime } from "../../util/format";
import type { DirectoryEntry, Project } from "../../types";

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

interface Creating {
  isDir: boolean;
  name: string;
}

interface Renaming {
  path: string;
  name: string;
}

export function DirectoryView({ project }: { project: Project }) {
  const [path, setPath] = useState("");
  const [entries, setEntries] = useState<DirectoryEntry[]>([]);
  const [status, setStatus] = useState<"loading" | "ready" | "error">("loading");
  const [revision, setRevision] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [creating, setCreating] = useState<Creating | null>(null);
  const [renaming, setRenaming] = useState<Renaming | null>(null);
  const [confirming, setConfirming] = useState<DirectoryEntry | null>(null);
  const [busy, setBusy] = useState(false);
  const watching = useProjectEvents(project.id);

  const reload = useCallback(() => setRevision((value) => value + 1), []);

  // The folder watcher sees changes made outside the app; when its refresh
  // finishes, reload the directory as well.
  const wasWatching = useRef(false);
  useEffect(() => {
    if (wasWatching.current && !watching) reload();
    wasWatching.current = watching;
  }, [watching, reload]);

  useEffect(() => {
    let active = true;
    setStatus("loading");
    listDirectory(project.id, path)
      .then((items) => {
        if (!active) return;
        setEntries(items);
        setStatus("ready");
        setError(null);
      })
      .catch((loadError) => {
        if (!active) return;
        setEntries([]);
        setError(message(loadError));
        setStatus("error");
      });
    return () => {
      active = false;
    };
  }, [project.id, path, revision]);

  // Changing folders clears the transient row state, never the loaded path.
  useEffect(() => {
    setSelected(null);
    setRenaming(null);
    setCreating(null);
    setConfirming(null);
  }, [path]);

  const breadcrumbs = useMemo(() => {
    if (!path) return [];
    const parts = path.split("/");
    return parts.map((name, index) => ({
      name,
      path: parts.slice(0, index + 1).join("/"),
    }));
  }, [path]);

  const run = async (action: () => Promise<void>, after?: () => void) => {
    setBusy(true);
    setError(null);
    try {
      await action();
      after?.();
      reload();
    } catch (actionError) {
      setError(message(actionError));
    } finally {
      setBusy(false);
    }
  };

  const submitCreate = () => {
    if (!creating) return;
    const name = creating.name.trim();
    if (!name) {
      setCreating(null);
      return;
    }
    void run(
      () => createEntry(project.id, path, name, creating.isDir),
      () => {
        setCreating(null);
        setSelected(null);
      },
    );
  };

  const submitRename = () => {
    if (!renaming) return;
    const name = renaming.name.trim();
    if (!name || name === renaming.path.split("/").pop()) {
      setRenaming(null);
      return;
    }
    void run(
      () => renameEntry(project.id, renaming.path, name),
      () => {
        setSelected(null);
        setRenaming(null);
      },
    );
  };

  const copy = (entry: DirectoryEntry) => {
    void run(() => copyEntry(project.id, entry.path));
  };

  const remove = () => {
    if (!confirming) return;
    const target = confirming;
    void run(
      () => deleteEntry(project.id, target.path),
      () => {
        setConfirming(null);
        setSelected(null);
      },
    );
  };

  const openFolder = (entry: DirectoryEntry) => {
    if (!entry.isDir) return;
    setPath(entry.path);
  };

  const startCreating = (isDir: boolean) => {
    setRenaming(null);
    setCreating({ isDir, name: "" });
  };

  const startRenaming = (entry: DirectoryEntry) => {
    setCreating(null);
    setSelected(entry.path);
    setRenaming({ path: entry.path, name: entry.name });
  };

  const empty = status !== "loading" && entries.length === 0;

  return (
    <div className="files">
      <div className="files__bar">
        <div className="files__crumbs" aria-label="Directory path">
          <button
            type="button"
            className="files__crumb"
            onClick={() => setPath("")}
            disabled={!path}
            title="Project root"
          >
            <FolderIcon size={14} />
            <span className="truncate">{project.name}</span>
          </button>
          {breadcrumbs.map((crumb) => (
            <span className="files__crumb-group" key={crumb.path}>
              <ChevronRightIcon size={12} />
              <button
                type="button"
                className="files__crumb"
                onClick={() => setPath(crumb.path)}
              >
                {crumb.name}
              </button>
            </span>
          ))}
        </div>

        <span className="project-header__spacer" />

        <button
          type="button"
          className="btn btn--secondary btn--sm"
          onClick={() => startCreating(true)}
          disabled={busy}
        >
          <FolderPlusIcon size={14} />
          New folder
        </button>
        <button
          type="button"
          className="btn btn--secondary btn--sm"
          onClick={() => startCreating(false)}
          disabled={busy}
        >
          <FilePlusIcon size={14} />
          New file
        </button>
      </div>

      {error ? (
        <div className="files__error" role="alert">
          {error}
        </div>
      ) : null}

      {creating ? (
        <form
          className="files__editor"
          onSubmit={(event) => {
            event.preventDefault();
            submitCreate();
          }}
        >
          {creating.isDir ? <FolderIcon size={15} /> : <DocIcon size={15} />}
          <input
            autoFocus
            value={creating.name}
            onChange={(event) => setCreating({ ...creating, name: event.target.value })}
            onKeyDown={(event) => {
              if (event.key === "Escape") setCreating(null);
            }}
            placeholder={creating.isDir ? "Folder name" : "File name, e.g. notes.md"}
            aria-label={creating.isDir ? "New folder name" : "New file name"}
          />
          <button type="submit" className="btn btn--primary btn--sm" disabled={busy}>
            Create
          </button>
          <button
            type="button"
            className="btn btn--ghost btn--sm"
            onClick={() => setCreating(null)}
          >
            Cancel
          </button>
        </form>
      ) : null}

      <div className="files__scroll">
        {status === "loading" ? (
          <p className="files__note">Reading the folder…</p>
        ) : status === "error" ? (
          <p className="files__note">This folder could not be read.</p>
        ) : empty ? (
          <div className="files__empty">
            <FolderIcon size={22} />
            <h2>This folder is empty</h2>
            <p>Create a folder or file here, or copy an existing item into this directory.</p>
            <div className="files__empty-actions">
              <button
                type="button"
                className="btn btn--secondary btn--sm"
                onClick={() => startCreating(true)}
              >
                <FolderPlusIcon size={14} />
                New folder
              </button>
              <button
                type="button"
                className="btn btn--secondary btn--sm"
                onClick={() => startCreating(false)}
              >
                <FilePlusIcon size={14} />
                New file
              </button>
            </div>
          </div>
        ) : (
          <div className="files__list" role="list">
            {entries.map((entry) =>
              renaming?.path === entry.path ? (
                <form
                  key={entry.path}
                  className="files__row files__row--editing"
                  onSubmit={(event) => {
                    event.preventDefault();
                    submitRename();
                  }}
                >
                  <span className="files__icon">
                    {entry.isDir ? <FolderIcon size={16} /> : <DocIcon size={16} />}
                  </span>
                  <input
                    autoFocus
                    value={renaming.name}
                    onChange={(event) =>
                      setRenaming({ path: renaming.path, name: event.target.value })
                    }
                    onKeyDown={(event) => {
                      if (event.key === "Escape") setRenaming(null);
                    }}
                    aria-label={`Rename ${entry.name}`}
                  />
                  <button type="submit" className="btn btn--primary btn--sm" disabled={busy}>
                    Save
                  </button>
                  <button
                    type="button"
                    className="btn btn--ghost btn--sm"
                    onClick={() => setRenaming(null)}
                  >
                    Cancel
                  </button>
                </form>
              ) : (
                <div
                  key={entry.path}
                  role="listitem"
                  className={`files__row${selected === entry.path ? " files__row--selected" : ""}`}
                >
                  <button
                    type="button"
                    className="files__item"
                    onClick={() => setSelected(entry.path)}
                    onDoubleClick={() => openFolder(entry)}
                    onKeyDown={(event) => {
                      if (entry.isDir && event.key === "Enter") {
                        event.preventDefault();
                        openFolder(entry);
                      }
                    }}
                    title={entry.path}
                  >
                    <span className={`files__icon${entry.isDir ? " files__icon--folder" : ""}`}>
                      {entry.isDir ? <FolderIcon size={16} /> : <DocIcon size={16} />}
                    </span>
                    <span className="files__name truncate">{entry.name}</span>
                    <span className="files__meta">
                      {entry.isDir ? "Folder" : formatBytes(entry.sizeBytes)}
                    </span>
                    <span className="files__meta files__meta--time">
                      {formatRelativeTime(entry.modifiedAt)}
                    </span>
                  </button>

                  <div className="files__actions">
                    {entry.isDir ? (
                      <button
                        type="button"
                        className="icon-btn icon-btn--sm"
                        onClick={() => openFolder(entry)}
                        title={`Open ${entry.name}`}
                      >
                        <ChevronRightIcon size={14} />
                        <span className="sr-only">Open {entry.name}</span>
                      </button>
                    ) : null}
                    <button
                      type="button"
                      className="icon-btn icon-btn--sm"
                      onClick={() => startRenaming(entry)}
                      disabled={busy}
                      title={`Rename ${entry.name}`}
                    >
                      <PencilIcon size={14} />
                      <span className="sr-only">Rename {entry.name}</span>
                    </button>
                    <button
                      type="button"
                      className="icon-btn icon-btn--sm"
                      onClick={() => copy(entry)}
                      disabled={busy}
                      title={`Copy ${entry.name}`}
                    >
                      <CopyIcon size={14} />
                      <span className="sr-only">Copy {entry.name}</span>
                    </button>
                    <button
                      type="button"
                      className="icon-btn icon-btn--sm files__danger"
                      onClick={() => {
                        setSelected(entry.path);
                        setConfirming(entry);
                      }}
                      disabled={busy}
                      title={`Delete ${entry.name}`}
                    >
                      <TrashIcon size={14} />
                      <span className="sr-only">Delete {entry.name}</span>
                    </button>
                  </div>
                </div>
              ),
            )}
          </div>
        )}
      </div>

      <div className="files__foot">
        <span>
          {entries.length} item{entries.length === 1 ? "" : "s"}
          {path ? ` in ${path}` : " at the project root"}
        </span>
        <span className="files__hint">
          Double-click a folder to open it · every change is written straight to disk
        </span>
      </div>

      {confirming ? (
        <div className="files__overlay" role="presentation">
          <div
            className="files__dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="files-delete-title"
          >
            <h2 id="files-delete-title">Delete “{confirming.name}”?</h2>
            <p>
              {confirming.isDir
                ? "This folder and everything inside it will be removed from the project."
                : "This file will be removed from the project."}{" "}
              This cannot be undone.
            </p>
            <div className="files__dialog-actions">
              <button
                type="button"
                className="btn btn--ghost btn--sm"
                onClick={() => setConfirming(null)}
                disabled={busy}
              >
                Cancel
              </button>
              <button
                type="button"
                className="btn btn--danger btn--sm"
                onClick={remove}
                disabled={busy}
              >
                {busy ? "Deleting…" : "Delete"}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
