import { useMemo, useState } from "react";
import { DocIcon, FolderIcon, SearchIcon } from "../ui/icons";
import { useProjectStore } from "../../state/projectStore";
import { useProjectEvents } from "../../hooks/useProjectEvents";
import { baseName, fileExtension, formatBytes, formatRelativeTime } from "../../util/format";
import type { Project, SourceFile, SourceStatus } from "../../types";

const STATUS_LABEL: Record<SourceStatus, string> = {
  indexed: "Indexed",
  indexing: "Indexing",
  pending: "Queued",
  error: "Failed",
};

function FileRow({ file }: { file: SourceFile }) {
  return (
    <div className="file-row" title={file.name}>
      <span className="file-row__icon">
        <DocIcon size={15} />
      </span>
      <span className="file-row__body">
        <span className="file-row__name truncate">{baseName(file.name)}</span>
        <span className="file-row__meta">
          <span className="mono">{fileExtension(file.name)}</span>
          <span>·</span>
          <span>{formatBytes(file.sizeBytes)}</span>
          <span>·</span>
          <span>{formatRelativeTime(file.modifiedAt)}</span>
        </span>
      </span>
      <span className={`file-row__status file-row__status--${file.status}`}>
        {STATUS_LABEL[file.status]}
      </span>
    </div>
  );
}

export function Sidebar({ project }: { project: Project }) {
  const sources = useProjectStore((state) => state.sources);
  const watching = useProjectEvents(project.id);
  const [query, setQuery] = useState("");

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return sources;
    return sources.filter((file) => file.name.toLowerCase().includes(needle));
  }, [sources, query]);

  const indexed = sources.filter((file) => file.status === "indexed").length;
  const percent = sources.length === 0 ? 0 : Math.round((indexed / sources.length) * 100);

  const reindexSources = useProjectStore((state) => state.reindexSources);
  const createFolder = useProjectStore((state) => state.createFolder);
  const projectError = useProjectStore((state) => state.error);
  const [indexing, setIndexing] = useState(false);
  const [addingFolder, setAddingFolder] = useState(false);
  const [folderName, setFolderName] = useState("");

  const handleReindex = async () => {
    setIndexing(true);
    try {
      await reindexSources();
    } finally {
      setIndexing(false);
    }
  };

  const submitFolder = async () => {
    const name = folderName.trim();
    if (!name) {
      setAddingFolder(false);
      return;
    }
    if (await createFolder(name)) {
      setFolderName("");
      setAddingFolder(false);
    }
  };

  return (
    <div className="panel">
      <div className="panel__head" style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <div style={{ display: "flex", alignItems: "center", gap: "6px" }}>
          <span className="panel__title">Sources</span>
          <span className="badge">{sources.length}</span>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: "4px" }}>
          <button
            className="icon-btn icon-btn--sm"
            onClick={() => setAddingFolder((value) => !value)}
            title="Create a folder in this project"
            aria-pressed={addingFolder}
          >
            <FolderIcon size={15} />
            <span className="sr-only">New folder</span>
          </button>
          <button
            className="btn btn--subtle"
            onClick={handleReindex}
            disabled={indexing || sources.length === 0}
            style={{ fontSize: "11px", padding: "3px 8px", cursor: "pointer" }}
            title="Parse and index files for cited search"
          >
            {indexing ? "Indexing…" : "Index"}
          </button>
        </div>
      </div>

      {addingFolder ? (
        <div className="panel__search">
          <label className="input">
            <FolderIcon size={14} />
            <input
              autoFocus
              value={folderName}
              onChange={(event) => setFolderName(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") void submitFolder();
                if (event.key === "Escape") {
                  setFolderName("");
                  setAddingFolder(false);
                }
              }}
              placeholder="Folder name, then Enter"
              aria-label="New folder name"
            />
          </label>
          {projectError ? <p className="muted mt-1">{projectError}</p> : null}
        </div>
      ) : null}

      <div className="panel__search">
        <label className="input">
          <SearchIcon size={14} />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Filter files"
            aria-label="Filter source files"
          />
        </label>
      </div>

      <div className="panel__scroll">
        {sources.length === 0 ? (
          <p className="muted mt-2">Reading the project folder…</p>
        ) : filtered.length === 0 ? (
          <p className="muted mt-2">No file matches “{query}”.</p>
        ) : (
          filtered.map((file) => <FileRow key={file.id} file={file} />)
        )}
      </div>

      <div className="panel__foot">
        <div>
          {indexed} of {sources.length} indexed
        </div>
        <div className="progress">
          <div className="progress__bar" style={{ width: `${percent}%` }} />
        </div>
        <div className="mt-2">
          {watching
            ? "The folder watcher is updating the index…"
            : project.internetEnabled
              ? "Internet on — web tools available to the agent"
              : "Internet off — local folders only"}
        </div>
      </div>
    </div>
  );
}
