import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ChevronRightIcon,
  DocIcon,
  FolderIcon,
  FolderPlusIcon,
  FilePlusIcon,
  SearchIcon,
  TrashIcon,
  PencilIcon,
} from "../ui/icons";
import { useProjectStore } from "../../state/projectStore";
import { useProjectEvents } from "../../hooks/useProjectEvents";
import { useNavStore } from "../../state/navStore";
import {
  listDirectory,
  createEntry,
  deleteEntry,
  renameEntry,
} from "../../api/files";
import {
  baseName,
  fileExtension,
  formatBytes,
  formatRelativeTime,
} from "../../util/format";
import type { DirectoryEntry, Project, SourceFile, SourceStatus } from "../../types";

const STATUS_LABEL: Record<SourceStatus, string> = {
  indexed: "INDEXED",
  indexing: "INDEXING",
  pending: "QUEUED",
  error: "FAILED",
};

/** Top-level folders the app owns; the backend refuses to move or delete them. */
const PROTECTED_ROOT_DIRS = new Set(["data", "chats", "diary", "outputs"]);

function getFileIconColor(name: string): { color: string; label: string } {
  const ext = fileExtension(name).toLowerCase();
  switch (ext) {
    case "pdf":
      return { color: "#ef4444", label: "pdf" };
    case "xlsx":
    case "xls":
    case "csv":
      return { color: "#10b981", label: ext };
    case "md":
    case "markdown":
      return { color: "#38bdf8", label: "md" };
    case "json":
    case "ts":
    case "js":
    case "py":
      return { color: "#a855f7", label: ext };
    case "docx":
    case "doc":
    case "txt":
      return { color: "#6366f1", label: "doc" };
    default:
      return { color: "var(--text-muted)", label: ext || "file" };
  }
}

interface TreeNodeItemProps {
  project: Project;
  path: string;
  name: string;
  isDir: boolean;
  sizeBytes: number;
  depth: number;
  expandedFolders: Record<string, boolean>;
  onToggleFolder(path: string): void;
  selectedPath: string | null;
  onSelectPath(path: string): void;
  onOpenDocument(doc: { projectId: string; filePath: string; fileName: string }): void;
  sourcesMap: Map<string, SourceFile>;
  onNewFile(parent: string): void;
  onNewFolder(parent: string): void;
  onRename(entry: { path: string; name: string }): void;
  onDelete(entry: { path: string; name: string }): void;
  query: string;
}

function TreeNodeItem({
  project,
  path,
  name,
  isDir,
  sizeBytes,
  depth,
  expandedFolders,
  onToggleFolder,
  selectedPath,
  onSelectPath,
  onOpenDocument,
  sourcesMap,
  onNewFile,
  onNewFolder,
  onRename,
  onDelete,
  query,
}: TreeNodeItemProps) {
  const isExpanded = Boolean(expandedFolders[path]);
  const isSelected = selectedPath === path;
  const isProtected =
    isDir && !path.includes("/") && PROTECTED_ROOT_DIRS.has(name.toLowerCase());
  const [children, setChildren] = useState<DirectoryEntry[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!isDir || !isExpanded) return;
    let active = true;
    setLoading(true);
    listDirectory(project.id, path)
      .then((items) => {
        if (active) setChildren(items);
      })
      .catch(() => {
        if (active) setChildren([]);
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [project.id, path, isDir, isExpanded]);

  const source = sourcesMap.get(path);
  const fileMeta = getFileIconColor(name);

  const needle = query.trim().toLowerCase();
  const matchesSelf = !needle || name.toLowerCase().includes(needle);
  const hasMatchingDescendant =
    needle &&
    isDir &&
    children.some((c) => c.name.toLowerCase().includes(needle));

  if (needle && !matchesSelf && !hasMatchingDescendant) {
    return null;
  }

  if (isDir) {
    return (
      <div style={{ display: "flex", flexDirection: "column" }}>
        <div
          className={`ide-tree-row ${isSelected ? "ide-tree-row--active" : ""}`}
          style={{ paddingLeft: `${depth * 14 + 6}px` }}
          onClick={() => {
            onToggleFolder(path);
            onSelectPath(path);
          }}
          title={path}
        >
          <span
            className={`ide-tree-chevron ${isExpanded ? "ide-tree-chevron--expanded" : ""}`}
            style={{ marginRight: "2px" }}
          >
            <ChevronRightIcon size={12} />
          </span>

          <span style={{ color: "#f59e0b", display: "inline-flex", flexShrink: 0 }}>
            <FolderIcon size={15} />
          </span>

          <span
            className="truncate"
            style={{
              fontWeight: isExpanded ? "600" : "500",
              color: isSelected ? "var(--accent-primary)" : "var(--text)",
              fontSize: "12.5px",
            }}
          >
            {name}
          </span>

          {children.length > 0 && (
            <span
              style={{
                fontSize: "10px",
                color: "var(--text-faint)",
                fontFamily: "var(--font-mono)",
                marginLeft: "2px",
              }}
            >
              ({children.length})
            </span>
          )}

          {/* Hover Actions */}
          <div className="ide-tree-actions" onClick={(e) => e.stopPropagation()}>
            <button
              type="button"
              className="icon-btn icon-btn--sm"
              style={{ width: "20px", height: "20px", padding: 0 }}
              onClick={() => onNewFile(path)}
              title="New file in this folder"
            >
              <FilePlusIcon size={12} />
            </button>
            <button
              type="button"
              className="icon-btn icon-btn--sm"
              style={{ width: "20px", height: "20px", padding: 0 }}
              onClick={() => onNewFolder(path)}
              title="New subfolder"
            >
              <FolderPlusIcon size={12} />
            </button>
            {isProtected ? null : (
              <button
                type="button"
                className="icon-btn icon-btn--sm"
                style={{ width: "20px", height: "20px", padding: 0, color: "#ef4444" }}
                onClick={() => onDelete({ path, name })}
                title="Delete folder"
              >
                <TrashIcon size={12} />
              </button>
            )}
          </div>
        </div>

        {/* Subtree */}
        {isExpanded && (
          <div className="ide-tree-children" style={{ marginLeft: `${depth * 14 + 13}px` }}>
            {loading ? (
              <div
                style={{
                  paddingLeft: "16px",
                  fontSize: "11px",
                  color: "var(--text-faint)",
                  height: "22px",
                  display: "flex",
                  alignItems: "center",
                }}
              >
                loading…
              </div>
            ) : children.length === 0 ? (
              <div
                style={{
                  paddingLeft: "16px",
                  fontSize: "11px",
                  color: "var(--text-faint)",
                  height: "22px",
                  display: "flex",
                  alignItems: "center",
                }}
              >
                (empty)
              </div>
            ) : (
              children
                .filter((child) => !child.name.startsWith("."))
                .map((child) => (
                <TreeNodeItem
                  key={child.path}
                  project={project}
                  path={child.path}
                  name={child.name}
                  isDir={child.isDir}
                  sizeBytes={child.sizeBytes}
                  depth={0}
                  expandedFolders={expandedFolders}
                  onToggleFolder={onToggleFolder}
                  selectedPath={selectedPath}
                  onSelectPath={onSelectPath}
                  onOpenDocument={onOpenDocument}
                  sourcesMap={sourcesMap}
                  onNewFile={onNewFile}
                  onNewFolder={onNewFolder}
                  onRename={onRename}
                  onDelete={onDelete}
                  query={query}
                />
              ))
            )}
          </div>
        )}
      </div>
    );
  }

  // File row
  return (
    <div
      className={`ide-tree-row ${isSelected ? "ide-tree-row--active" : ""}`}
      style={{ paddingLeft: `${depth * 14 + 20}px` }}
      onClick={() => onSelectPath(path)}
      onDoubleClick={() => onOpenDocument({ projectId: project.id, filePath: path, fileName: name })}
      title={`${path} (${formatBytes(sizeBytes || source?.sizeBytes || 0)})`}
    >
      <span style={{ color: fileMeta.color, display: "inline-flex", flexShrink: 0 }}>
        <DocIcon size={14} />
      </span>

      <span
        className="truncate"
        style={{
          color: isSelected ? "var(--accent-primary)" : "var(--text)",
          fontSize: "12.5px",
        }}
      >
        {name}
      </span>

      {source && (
        <span
          className={`file-row__status file-row__status--${source.status}`}
          style={{
            marginLeft: "auto",
            marginRight: "4px",
            fontSize: "9px",
            padding: "1px 4px",
            borderRadius: "2px",
            background:
              source.status === "indexed"
                ? "rgba(117, 214, 156, 0.12)"
                : "rgba(255, 255, 255, 0.06)",
          }}
        >
          {STATUS_LABEL[source.status]}
        </span>
      )}

      {/* Hover Actions */}
      <div className="ide-tree-actions" onClick={(e) => e.stopPropagation()}>
        <button
          type="button"
          className="icon-btn icon-btn--sm"
          style={{ width: "20px", height: "20px", padding: 0 }}
          onClick={() => onRename({ path, name })}
          title="Rename"
        >
          <PencilIcon size={12} />
        </button>
        <button
          type="button"
          className="icon-btn icon-btn--sm"
          style={{ width: "20px", height: "20px", padding: 0, color: "#ef4444" }}
          onClick={() => onDelete({ path, name })}
          title="Delete file"
        >
          <TrashIcon size={12} />
        </button>
      </div>
    </div>
  );
}

export function Sidebar({ project }: { project: Project }) {
  const sources = useProjectStore((state) => state.sources);
  const watching = useProjectEvents(project.id);
  const reindexSources = useProjectStore((state) => state.reindexSources);
  const [indexing, setIndexing] = useState(false);

  const [sidebarMode, setSidebarMode] = useState<"tree" | "sources">("tree");
  const [query, setQuery] = useState("");
  const [rootEntries, setRootEntries] = useState<DirectoryEntry[]>([]);
  const [expandedFolders, setExpandedFolders] = useState<Record<string, boolean>>({
    invoices: true,
    data: true,
    outputs: true,
  });
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const setOpenDocument = useNavStore((state) => state.setOpenDocument);
  const setActiveProjectTab = useNavStore((state) => state.setActiveProjectTab);

  const handleOpenDocument = (doc: { projectId: string; filePath: string; fileName: string }) => {
    setActiveProjectTab("chat");
    setOpenDocument(doc);
  };

  // Modals
  const [modalType, setModalType] = useState<"folder" | "file" | "rename" | "delete" | null>(null);
  const [modalTargetParent, setModalTargetParent] = useState("");
  const [modalTargetEntry, setModalTargetEntry] = useState<{ path: string; name: string } | null>(null);
  const [modalInputVal, setModalInputVal] = useState("");

  const sourcesMap = useMemo(() => {
    const map = new Map<string, SourceFile>();
    for (const src of sources) {
      map.set(src.name, src);
      map.set(baseName(src.name), src);
    }
    return map;
  }, [sources]);

  const loadRoot = useCallback(async () => {
    try {
      const items = await listDirectory(project.id, "");
      setRootEntries(items);
      // Auto expand first level folders
      setExpandedFolders((prev) => {
        const next = { ...prev };
        items.forEach((item) => {
          if (item.isDir && next[item.path] === undefined) {
            next[item.path] = true;
          }
        });
        return next;
      });
    } catch {
      setRootEntries([]);
    }
  }, [project.id]);

  useEffect(() => {
    void loadRoot();
  }, [loadRoot]);

  const wasWatching = useRef(false);
  useEffect(() => {
    if (wasWatching.current && !watching) void loadRoot();
    wasWatching.current = watching;
  }, [watching, loadRoot]);

  const handleToggleFolder = (folderPath: string) => {
    setExpandedFolders((prev) => ({
      ...prev,
      [folderPath]: !prev[folderPath],
    }));
  };

  const handleReindex = async () => {
    setIndexing(true);
    try {
      await reindexSources();
      await loadRoot();
    } finally {
      setIndexing(false);
    }
  };

  const handleCollapseAll = () => {
    setExpandedFolders({});
  };

  const handleExpandAll = () => {
    const next: Record<string, boolean> = {};
    rootEntries.forEach((e) => {
      if (e.isDir) next[e.path] = true;
    });
    setExpandedFolders(next);
  };

  // Modal actions
  const openNewFile = (parent: string) => {
    setModalTargetParent(parent);
    setModalInputVal("");
    setModalType("file");
  };

  const openNewFolder = (parent: string) => {
    setModalTargetParent(parent);
    setModalInputVal("");
    setModalType("folder");
  };

  const openRename = (entry: { path: string; name: string }) => {
    setModalTargetEntry(entry);
    setModalInputVal(entry.name);
    setModalType("rename");
  };

  const openDelete = (entry: { path: string; name: string }) => {
    setModalTargetEntry(entry);
    setModalType("delete");
  };

  const handleModalSubmit = async () => {
    const val = modalInputVal.trim();
    try {
      if (modalType === "folder") {
        if (!val) return;
        await createEntry(project.id, modalTargetParent, val, true);
      } else if (modalType === "file") {
        if (!val) return;
        await createEntry(project.id, modalTargetParent, val, false);
      } else if (modalType === "rename" && modalTargetEntry) {
        if (!val || val === modalTargetEntry.name) return;
        await renameEntry(project.id, modalTargetEntry.path, val);
      } else if (modalType === "delete" && modalTargetEntry) {
        await deleteEntry(project.id, modalTargetEntry.path);
        if (selectedPath === modalTargetEntry.path) setSelectedPath(null);
      }
      setModalType(null);
      await loadRoot();
    } catch (err) {
      alert(err instanceof Error ? err.message : String(err));
    }
  };

  const indexed = sources.filter((file) => file.status === "indexed").length;
  const percent = sources.length === 0 ? 0 : Math.round((indexed / sources.length) * 100);

  return (
    <div className="panel" style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      {/* IDE Top Bar Header */}
      <div
        className="panel__head"
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          padding: "8px 10px",
          borderBottom: "1px solid var(--border)",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: "6px" }}>
          {/* Segmented Mode: Tree vs Sources */}
          <div
            style={{
              display: "inline-flex",
              padding: "2px",
              background: "var(--surface-2)",
              borderRadius: "var(--radius-sm)",
            }}
          >
            <button
              type="button"
              onClick={() => setSidebarMode("tree")}
              style={{
                padding: "2px 8px",
                fontSize: "11px",
                fontWeight: sidebarMode === "tree" ? "600" : "500",
                borderRadius: "var(--radius-xs)",
                background: sidebarMode === "tree" ? "var(--surface)" : "transparent",
                color: sidebarMode === "tree" ? "var(--text)" : "var(--text-muted)",
                cursor: "pointer",
              }}
            >
              Explorer Tree
            </button>
            <button
              type="button"
              onClick={() => setSidebarMode("sources")}
              style={{
                padding: "2px 8px",
                fontSize: "11px",
                fontWeight: sidebarMode === "sources" ? "600" : "500",
                borderRadius: "var(--radius-xs)",
                background: sidebarMode === "sources" ? "var(--surface)" : "transparent",
                color: sidebarMode === "sources" ? "var(--text)" : "var(--text-muted)",
                cursor: "pointer",
              }}
            >
              Sources ({sources.length})
            </button>
          </div>
        </div>

        {/* Action icons like VS Code Explorer */}
        <div style={{ display: "flex", alignItems: "center", gap: "2px" }}>
          {sidebarMode === "tree" && (
            <>
              <button
                type="button"
                className="icon-btn icon-btn--sm"
                onClick={() => openNewFile("")}
                title="New File in project root"
                style={{ width: "24px", height: "24px" }}
              >
                <FilePlusIcon size={14} />
              </button>
              <button
                type="button"
                className="icon-btn icon-btn--sm"
                onClick={() => openNewFolder("")}
                title="New Folder in project root"
                style={{ width: "24px", height: "24px" }}
              >
                <FolderPlusIcon size={14} />
              </button>
              <button
                type="button"
                className="icon-btn icon-btn--sm"
                onClick={Object.keys(expandedFolders).length > 0 ? handleCollapseAll : handleExpandAll}
                title={Object.keys(expandedFolders).length > 0 ? "Collapse all folders" : "Expand all folders"}
                style={{ width: "24px", height: "24px", fontSize: "12px" }}
              >
                {Object.keys(expandedFolders).length > 0 ? "−" : "+"}
              </button>
            </>
          )}

          <button
            type="button"
            className="btn btn--subtle"
            onClick={handleReindex}
            disabled={indexing || sources.length === 0}
            style={{ fontSize: "11px", padding: "2px 7px", cursor: "pointer", height: "24px" }}
            title="Parse and re-index project sources"
          >
            {indexing ? "…" : "Index"}
          </button>
        </div>
      </div>

      {/* Search Input */}
      <div className="panel__search" style={{ padding: "6px 10px" }}>
        <label className="input" style={{ height: "28px" }}>
          <SearchIcon size={13} />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={sidebarMode === "tree" ? "Filter tree…" : "Filter source files…"}
            aria-label="Filter project files"
            style={{ fontSize: "12px" }}
          />
          {query && (
            <button
              type="button"
              onClick={() => setQuery("")}
              className="muted"
              style={{ cursor: "pointer", fontSize: "12px" }}
            >
              ×
            </button>
          )}
        </label>
      </div>

      {/* Body: IDE Tree or Sources List */}
      <div className="panel__scroll" style={{ padding: "4px 6px" }}>
        {sidebarMode === "tree" ? (
          <div className="ide-tree">
            {/* Root Project Node */}
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: "6px",
                padding: "4px 8px",
                fontSize: "11.5px",
                fontWeight: "700",
                textTransform: "uppercase",
                letterSpacing: "0.06em",
                color: "var(--text-muted)",
              }}
            >
              <span className="truncate">{project.name}</span>
              <span style={{ fontSize: "10px", color: "var(--text-faint)", marginLeft: "auto" }}>
                {rootEntries.length} items
              </span>
            </div>

            {/* Tree Children */}
            {rootEntries.length === 0 ? (
              <p className="muted" style={{ padding: "12px 8px", fontSize: "12px" }}>
                Reading workspace directory…
              </p>
            ) : (
              rootEntries
                .filter((entry) => !entry.name.startsWith("."))
                .map((entry) => (
                <TreeNodeItem
                  key={entry.path}
                  project={project}
                  path={entry.path}
                  name={entry.name}
                  isDir={entry.isDir}
                  sizeBytes={entry.sizeBytes}
                  depth={0}
                  expandedFolders={expandedFolders}
                  onToggleFolder={handleToggleFolder}
                  selectedPath={selectedPath}
                  onSelectPath={setSelectedPath}
                  onOpenDocument={handleOpenDocument}
                  sourcesMap={sourcesMap}
                  onNewFile={openNewFile}
                  onNewFolder={openNewFolder}
                  onRename={openRename}
                  onDelete={openDelete}
                  query={query}
                />
              ))
            )}
          </div>
        ) : (
          /* Flat Sources list mode */
          <div>
            {sources.length === 0 ? (
              <p className="muted mt-2" style={{ padding: "0 8px" }}>Reading the project folder…</p>
            ) : (
              sources
                .filter((f) => !query || f.name.toLowerCase().includes(query.toLowerCase()))
                .map((file) => (
                  <div
                    className="file-row"
                    key={file.id}
                    title={file.name}
                    onDoubleClick={() => {
                      setActiveProjectTab("chat");
                      setOpenDocument({ projectId: project.id, filePath: file.name, fileName: file.name });
                    }}
                    style={{ cursor: "pointer" }}
                  >
                    <span className="file-row__icon">
                      <DocIcon size={15} />
                    </span>
                    <span className="file-row__body">
                      <span className="file-row__name truncate">{file.name}</span>
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
                ))
            )}
          </div>
        )}
      </div>

      {/* Panel Foot: Indexing & Storage telemetry */}
      <div className="panel__foot" style={{ padding: "8px 10px", fontSize: "11px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "4px" }}>
          <span>{indexed} of {sources.length} indexed</span>
          <span className="mono">{percent}%</span>
        </div>
        <div className="progress" style={{ height: "3px" }}>
          <div className="progress__bar" style={{ width: `${percent}%` }} />
        </div>
        <div style={{ marginTop: "6px", color: "var(--text-faint)", fontSize: "10.5px" }}>
          {watching
            ? "Watcher active: live index refresh"
            : project.internetEnabled
              ? "Local first · Network permitted for research"
              : "Strictly offline · No cloud sync"}
        </div>
      </div>

      {/* Common Modals */}
      {modalType && (
        <div
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(0, 0, 0, 0.65)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 120,
          }}
          onClick={() => setModalType(null)}
        >
          <div
            style={{
              background: "var(--surface)",
              border: "1px solid var(--border)",
              borderRadius: "var(--radius-lg)",
              padding: "20px",
              width: "100%",
              maxWidth: "360px",
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <h3 style={{ fontSize: "15px", fontWeight: "600", marginBottom: "8px" }}>
              {modalType === "folder"
                ? "New Folder"
                : modalType === "file"
                  ? "New File"
                  : modalType === "rename"
                    ? "Rename Item"
                    : "Confirm Deletion"}
            </h3>

            {modalType === "delete" ? (
              <p style={{ fontSize: "13px", color: "var(--text-secondary)", marginBottom: "16px" }}>
                Are you sure you want to delete <strong>{modalTargetEntry?.name}</strong>?
              </p>
            ) : (
              <input
                type="text"
                autoFocus
                value={modalInputVal}
                onChange={(e) => setModalInputVal(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void handleModalSubmit();
                  if (e.key === "Escape") setModalType(null);
                }}
                placeholder={
                  modalType === "folder"
                    ? "Folder name"
                    : modalType === "file"
                      ? "File name (e.g. notes.md)"
                      : "New name"
                }
                style={{
                  width: "100%",
                  padding: "7px 10px",
                  borderRadius: "var(--radius-md)",
                  border: "1px solid var(--border)",
                  background: "var(--background)",
                  color: "var(--text)",
                  fontSize: "13px",
                  marginBottom: "16px",
                }}
              />
            )}

            <div style={{ display: "flex", justifyContent: "flex-end", gap: "8px" }}>
              <button
                type="button"
                className="btn btn--subtle btn--sm"
                onClick={() => setModalType(null)}
              >
                Cancel
              </button>
              <button
                type="button"
                className="btn btn--primary btn--sm"
                onClick={() => void handleModalSubmit()}
                style={modalType === "delete" ? { background: "#dc2626", color: "#fff" } : undefined}
              >
                {modalType === "delete" ? "Delete" : "Save"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
