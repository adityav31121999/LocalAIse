import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ChevronLeftIcon,
  ChevronRightIcon,
  DocIcon,
  FolderIcon,
  SearchIcon,
  TrashIcon,
  PencilIcon,
  CopyIcon,
  PlusIcon,
  FolderPlusIcon,
  FilePlusIcon,
  SparkIcon,
} from "../ui/icons";
import {
  listDirectory,
  createEntry,
  renameEntry,
  deleteEntry,
  copyEntry,
  importFiles,
} from "../../api/files";
import { isTauri } from "../../api";
import { useProjectEvents } from "../../hooks/useProjectEvents";
import { useNavStore } from "../../state/navStore";
import { formatBytes, formatRelativeTime, fileExtension } from "../../util/format";
import type { DirectoryEntry, Project } from "../../types";

type ViewMode = "table" | "grid";
type SortField = "name" | "size" | "date" | "type";
type SortDirection = "asc" | "desc";

/** Top-level folders the app owns; the backend refuses to move or delete them. */
const PROTECTED_ROOT_DIRS = new Set(["data", "chats", "diary", "outputs"]);

function isProtectedRootEntry(entry: DirectoryEntry, currentPath: string): boolean {
  return currentPath === "" && entry.isDir && PROTECTED_ROOT_DIRS.has(entry.name.toLowerCase());
}

function getFileCategory(name: string, isDir: boolean): {
  label: string;
  iconBg: string;
  iconColor: string;
  typeBadge: string;
} {
  if (isDir) {
    return {
      label: "Folder",
      iconBg: "rgba(245, 158, 11, 0.12)",
      iconColor: "#f59e0b",
      typeBadge: "Folder",
    };
  }
  const ext = fileExtension(name).toLowerCase();
  switch (ext) {
    case "pdf":
      return {
        label: "PDF Document",
        iconBg: "rgba(239, 68, 68, 0.12)",
        iconColor: "#ef4444",
        typeBadge: "PDF",
      };
    case "xlsx":
    case "xls":
    case "csv":
      return {
        label: "Spreadsheet",
        iconBg: "rgba(16, 185, 129, 0.12)",
        iconColor: "#10b981",
        typeBadge: "DATA",
      };
    case "md":
    case "markdown":
      return {
        label: "Markdown Document",
        iconBg: "rgba(56, 189, 248, 0.12)",
        iconColor: "#38bdf8",
        typeBadge: "MD",
      };
    case "json":
    case "ts":
    case "js":
    case "py":
      return {
        label: "Code & Schema",
        iconBg: "rgba(168, 85, 247, 0.12)",
        iconColor: "#a855f7",
        typeBadge: "CODE",
      };
    case "docx":
    case "doc":
    case "txt":
      return {
        label: "Text Document",
        iconBg: "rgba(99, 102, 241, 0.12)",
        iconColor: "#6366f1",
        typeBadge: "DOC",
      };
    default:
      return {
        label: "File",
        iconBg: "rgba(148, 163, 184, 0.12)",
        iconColor: "var(--text-muted)",
        typeBadge: ext ? ext.toUpperCase() : "FILE",
      };
  }
}

function getSamplePreview(name: string, isDir: boolean, project: Project): string {
  if (isDir) {
    return `Folder: ${name}\nContains project assets and indexed documents for ${project.name}.`;
  }
  const ext = fileExtension(name).toLowerCase();
  if (ext === "json") {
    return JSON.stringify(
      {
        project: project.name,
        template: project.template,
        system_prompt: project.systemPrompt || "Default local privacy model",
        local_rag_enabled: true,
        indexed_at: new Date().toISOString(),
      },
      null,
      2,
    );
  }
  if (ext === "csv" || ext === "xlsx") {
    return `Date,Item,Category,Amount_USD,Status\n2026-01-15,Server Hardware Tier 1,Infrastructure,1240.00,Approved\n2026-01-22,On-device NPU DevKit,Hardware,499.00,Completed\n2026-02-04,Vector Store Benchmark,Research,150.00,Verified\n2026-02-18,Local Memory Profiling,Diagnostics,320.00,Archived`;
  }
  if (ext === "pdf") {
    return `# Document Abstract: ${name}\n\nThis publication examines local on-device neural network deployment and private retrieval-augmented generation (RAG).\n\nKey Findings:\n- Latency drops by 4.2x with dynamic KV cache quantization (Q4_0).\n- CPU microarchitecture AVX2/AVX-512 vectorization offloads 8B parameter models with zero cloud roundtrips.\n- User documents remain strictly in local storage without network transmission.`;
  }
  return `# Notes: ${name}\n\nDocument tracked in localAIse project "${project.name}".\n\n- All content is parsed locally using BM25 tokenization.\n- Model citations reference passages directly from this file.\n- Last updated ${new Date().toLocaleDateString()}.`;
}

export function DirectoryView({ project }: { project: Project }) {
  const setOpenDocument = useNavStore((state) => state.setOpenDocument);
  const setActiveProjectTab = useNavStore((state) => state.setActiveProjectTab);
  const [currentPath, setCurrentPath] = useState<string>("");
  const [history, setHistory] = useState<string[]>([""]);
  const [historyIdx, setHistoryIdx] = useState<number>(0);

  const [entries, setEntries] = useState<DirectoryEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [query, setQuery] = useState("");
  const [viewMode, setViewMode] = useState<ViewMode>("table");
  const [sortField, setSortField] = useState<SortField>("name");
  const [sortDir, setSortDir] = useState<SortDirection>("asc");
  const [selectedEntry, setSelectedEntry] = useState<DirectoryEntry | null>(null);
  const [copied, setCopied] = useState(false);

  // Modals
  const [showNewFolder, setShowNewFolder] = useState(false);
  const [newFolderName, setNewFolderName] = useState("");
  const [showNewFile, setShowNewFile] = useState(false);
  const [newFileName, setNewFileName] = useState("");
  const [newFileExt, setNewFileExt] = useState(".md");
  const [showRename, setShowRename] = useState<DirectoryEntry | null>(null);
  const [renameVal, setRenameVal] = useState("");
  const [showDelete, setShowDelete] = useState<DirectoryEntry | null>(null);
  const [previewModal, setPreviewModal] = useState<DirectoryEntry | null>(null);

  const loadFolder = useCallback(
    async (path: string) => {
      setLoading(true);
      setError(null);
      try {
        const items = await listDirectory(project.id, path);
        setEntries(items);
        setSelectedEntry(null);
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
        setEntries([]);
      } finally {
        setLoading(false);
      }
    },
    [project.id],
  );

  useEffect(() => {
    void loadFolder(currentPath);
  }, [currentPath, loadFolder]);

  // Project events watcher
  const watching = useProjectEvents(project.id);
  const wasWatching = useRef(false);
  useEffect(() => {
    if (wasWatching.current && !watching) void loadFolder(currentPath);
    wasWatching.current = watching;
  }, [watching, currentPath, loadFolder]);

  const navigateTo = (path: string) => {
    if (path === currentPath) return;
    const nextHistory = history.slice(0, historyIdx + 1);
    nextHistory.push(path);
    setHistory(nextHistory);
    setHistoryIdx(nextHistory.length - 1);
    setCurrentPath(path);
  };

  const handleBack = () => {
    if (historyIdx > 0) {
      const nextIdx = historyIdx - 1;
      setHistoryIdx(nextIdx);
      setCurrentPath(history[nextIdx]);
    }
  };

  const handleForward = () => {
    if (historyIdx < history.length - 1) {
      const nextIdx = historyIdx + 1;
      setHistoryIdx(nextIdx);
      setCurrentPath(history[nextIdx]);
    }
  };

  const handleUp = () => {
    if (!currentPath) return;
    const parts = currentPath.split("/").filter(Boolean);
    parts.pop();
    navigateTo(parts.join("/"));
  };

  const breadcrumbs = useMemo(() => {
    if (!currentPath) return [];
    const parts = currentPath.split("/").filter(Boolean);
    return parts.map((part, index) => ({
      name: part,
      path: parts.slice(0, index + 1).join("/"),
    }));
  }, [currentPath]);

  const handleCopyPath = () => {
    const full = selectedEntry ? `${project.path}\\${selectedEntry.path.replace(/\//g, "\\")}` : `${project.path}\\${currentPath.replace(/\//g, "\\")}`;
    void navigator.clipboard.writeText(full);
    setCopied(true);
    setTimeout(() => setCopied(false), 1800);
  };

  // Filter & Sort
  const filteredEntries = useMemo(() => {
    const needle = query.trim().toLowerCase();
    let result = entries.filter((e) => !e.name.startsWith("."));
    if (needle) {
      result = result.filter((e) => e.name.toLowerCase().includes(needle));
    }
    return [...result].sort((a, b) => {
      // Folders always first unless sorting explicitly
      if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
      if (sortField === "size") {
        return sortDir === "asc" ? a.sizeBytes - b.sizeBytes : b.sizeBytes - a.sizeBytes;
      }
      if (sortField === "date") {
        const da = new Date(a.modifiedAt).getTime();
        const db = new Date(b.modifiedAt).getTime();
        return sortDir === "asc" ? da - db : db - da;
      }
      if (sortField === "type") {
        const extA = fileExtension(a.name);
        const extB = fileExtension(b.name);
        return sortDir === "asc" ? extA.localeCompare(extB) : extB.localeCompare(extA);
      }
      return sortDir === "asc"
        ? a.name.localeCompare(b.name)
        : b.name.localeCompare(a.name);
    });
  }, [entries, query, sortField, sortDir]);

  // Operations
  const handleCreateFolder = async () => {
    const name = newFolderName.trim();
    if (!name) return;
    try {
      await createEntry(project.id, currentPath, name, true);
      setNewFolderName("");
      setShowNewFolder(false);
      await loadFolder(currentPath);
    } catch (err) {
      alert(err instanceof Error ? err.message : String(err));
    }
  };

  const handleCreateFile = async () => {
    let name = newFileName.trim();
    if (!name) return;
    if (!name.includes(".")) name += newFileExt;
    try {
      await createEntry(project.id, currentPath, name, false);
      setNewFileName("");
      setShowNewFile(false);
      await loadFolder(currentPath);
    } catch (err) {
      alert(err instanceof Error ? err.message : String(err));
    }
  };

  const handleRename = async () => {
    if (!showRename) return;
    const name = renameVal.trim();
    if (!name || name === showRename.name) {
      setShowRename(null);
      return;
    }
    try {
      await renameEntry(project.id, showRename.path, name);
      setShowRename(null);
      await loadFolder(currentPath);
    } catch (err) {
      alert(err instanceof Error ? err.message : String(err));
    }
  };

  const handleDelete = async () => {
    if (!showDelete) return;
    try {
      await deleteEntry(project.id, showDelete.path);
      setShowDelete(null);
      if (selectedEntry?.path === showDelete.path) setSelectedEntry(null);
      await loadFolder(currentPath);
    } catch (err) {
      alert(err instanceof Error ? err.message : String(err));
    }
  };

  const handleCopy = async (entry: DirectoryEntry) => {
    try {
      await copyEntry(project.id, entry.path);
      await loadFolder(currentPath);
    } catch (err) {
      alert(err instanceof Error ? err.message : String(err));
    }
  };

  const handleImport = async () => {
    try {
      await importFiles(project.id, currentPath);
      await loadFolder(currentPath);
    } catch (err) {
      alert(err instanceof Error ? err.message : String(err));
    }
  };

  return (
    <div
      className="file-explorer"
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100%",
        width: "100%",
        background: "var(--background)",
        color: "var(--text)",
        overflow: "hidden",
      }}
    >
      {/* Explorer Top Bar & Breadcrumbs */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "8px",
          padding: "8px 16px",
          background: "var(--background-elevated)",
          borderBottom: "1px solid var(--border)",
        }}
      >
        <button
          type="button"
          className="icon-btn icon-btn--sm"
          disabled={historyIdx <= 0}
          onClick={handleBack}
          title="Back"
        >
          <ChevronLeftIcon size={16} />
        </button>
        <button
          type="button"
          className="icon-btn icon-btn--sm"
          disabled={historyIdx >= history.length - 1}
          onClick={handleForward}
          title="Forward"
        >
          <ChevronRightIcon size={16} />
        </button>
        <button
          type="button"
          className="icon-btn icon-btn--sm"
          disabled={!currentPath}
          onClick={handleUp}
          title="Up to parent folder"
        >
          <span style={{ fontSize: "14px", fontWeight: "bold" }}>↑</span>
        </button>

        {/* Path Breadcrumbs Box */}
        <div
          style={{
            flex: 1,
            display: "flex",
            alignItems: "center",
            gap: "4px",
            background: "var(--surface)",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius-md)",
            padding: "3px 10px",
            overflowX: "auto",
            fontSize: "12.5px",
          }}
        >
          <button
            type="button"
            onClick={() => navigateTo("")}
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: "5px",
              color: currentPath === "" ? "var(--accent-primary)" : "var(--text-secondary)",
              fontWeight: currentPath === "" ? "600" : "500",
              cursor: "pointer",
            }}
          >
            <FolderIcon size={14} />
            <span>{project.name}</span>
          </button>

          {breadcrumbs.map((crumb) => (
            <span key={crumb.path} style={{ display: "inline-flex", alignItems: "center", gap: "4px" }}>
              <span className="muted" style={{ fontSize: "11px" }}>/</span>
              <button
                type="button"
                onClick={() => navigateTo(crumb.path)}
                style={{
                  color: crumb.path === currentPath ? "var(--accent-primary)" : "var(--text-secondary)",
                  fontWeight: crumb.path === currentPath ? "600" : "500",
                  cursor: "pointer",
                }}
              >
                {crumb.name}
              </button>
            </span>
          ))}
        </div>

        <button
          type="button"
          className="btn btn--subtle btn--sm"
          onClick={handleCopyPath}
          title="Copy full OS path"
        >
          <CopyIcon size={13} />
          <span>{copied ? "Copied" : "Copy Path"}</span>
        </button>
      </div>

      {/* Explorer Toolbar */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          flexWrap: "wrap",
          gap: "10px",
          padding: "8px 16px",
          background: "var(--surface)",
          borderBottom: "1px solid var(--border)",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: "8px", flex: 1, minWidth: "220px", maxWidth: "380px" }}>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: "6px",
              background: "var(--background)",
              border: "1px solid var(--border)",
              borderRadius: "var(--radius-md)",
              padding: "0 10px",
              width: "100%",
              height: "30px",
            }}
          >
            <SearchIcon size={13} className="muted" />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search current folder…"
              style={{
                background: "transparent",
                border: "none",
                outline: "none",
                fontSize: "12.5px",
                color: "var(--text)",
                width: "100%",
              }}
            />
            {query && (
              <button
                type="button"
                onClick={() => setQuery("")}
                className="muted"
                style={{ cursor: "pointer", fontSize: "13px" }}
              >
                ×
              </button>
            )}
          </div>
        </div>

        <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
          {/* Actions */}
          <button
            type="button"
            className="btn btn--primary btn--sm"
            onClick={() => setShowNewFolder(true)}
            title="Create a new folder"
          >
            <FolderPlusIcon size={14} />
            <span>New Folder</span>
          </button>

          <button
            type="button"
            className="btn btn--subtle btn--sm"
            onClick={() => setShowNewFile(true)}
            title="Create a new markdown, code, or document file"
          >
            <FilePlusIcon size={14} />
            <span>New File</span>
          </button>

          <button
            type="button"
            className="btn btn--subtle btn--sm"
            onClick={() => void handleImport()}
            disabled={!isTauri}
            title={
              isTauri
                ? "Copy files from your machine into this folder"
                : "Copying files needs the desktop app"
            }
          >
            <span>Add files</span>
          </button>

          <div style={{ width: "1px", height: "18px", background: "var(--border)" }} />

          {/* Sort dropdown */}
          <select
            value={`${sortField}-${sortDir}`}
            onChange={(e) => {
              const [f, d] = e.target.value.split("-") as [SortField, SortDirection];
              setSortField(f);
              setSortDir(d);
            }}
            className="input"
            style={{
              height: "30px",
              fontSize: "12px",
              padding: "0 8px",
              borderRadius: "var(--radius-md)",
              background: "var(--background)",
              border: "1px solid var(--border)",
              color: "var(--text)",
              cursor: "pointer",
            }}
          >
            <option value="name-asc">Sort: Name (A-Z)</option>
            <option value="name-desc">Sort: Name (Z-A)</option>
            <option value="size-desc">Sort: Size (Largest)</option>
            <option value="size-asc">Sort: Size (Smallest)</option>
            <option value="date-desc">Sort: Date (Newest)</option>
            <option value="date-asc">Sort: Date (Oldest)</option>
            <option value="type-asc">Sort: File Type</option>
          </select>

          {/* View Mode Toggle */}
          <div
            style={{
              display: "inline-flex",
              padding: "2px",
              background: "var(--background)",
              borderRadius: "var(--radius-md)",
              border: "1px solid var(--border)",
            }}
          >
            <button
              type="button"
              onClick={() => setViewMode("table")}
              style={{
                padding: "3px 8px",
                fontSize: "11px",
                fontWeight: "500",
                borderRadius: "var(--radius-sm)",
                background: viewMode === "table" ? "var(--surface)" : "transparent",
                color: viewMode === "table" ? "var(--text)" : "var(--text-muted)",
                cursor: "pointer",
              }}
            >
              List
            </button>
            <button
              type="button"
              onClick={() => setViewMode("grid")}
              style={{
                padding: "3px 8px",
                fontSize: "11px",
                fontWeight: "500",
                borderRadius: "var(--radius-sm)",
                background: viewMode === "grid" ? "var(--surface)" : "transparent",
                color: viewMode === "grid" ? "var(--text)" : "var(--text-muted)",
                cursor: "pointer",
              }}
            >
              Grid
            </button>
          </div>
        </div>
      </div>

      {/* Explorer Split: Left Quick Tree + Center Content + Right Preview */}
      <div style={{ flex: 1, display: "flex", minHeight: 0, overflow: "hidden" }}>
        {/* Quick Folder Navigation Pane */}
        <div
          style={{
            width: "210px",
            minWidth: "170px",
            background: "var(--background-elevated)",
            borderRight: "1px solid var(--border)",
            display: "flex",
            flexDirection: "column",
            padding: "10px 8px",
            gap: "4px",
            overflowY: "auto",
            fontSize: "12.5px",
          }}
        >
          <div className="muted" style={{ fontSize: "11px", fontWeight: "600", textTransform: "uppercase", padding: "4px 8px" }}>
            Folders
          </div>

          {[
            { name: "Root Directory", path: "" },
            { name: "data (Sources)", path: "data" },
            { name: "chats (Sessions)", path: "chats" },
            { name: "diary (Daily logs)", path: "diary" },
            { name: "outputs (Reports)", path: "outputs" },
          ].map((item) => {
            const isActive = currentPath === item.path;
            return (
              <button
                key={item.path}
                type="button"
                onClick={() => navigateTo(item.path)}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "8px",
                  padding: "6px 10px",
                  borderRadius: "var(--radius-md)",
                  background: isActive ? "var(--accent-primary-soft)" : "transparent",
                  color: isActive ? "var(--accent-primary)" : "var(--text-secondary)",
                  fontWeight: isActive ? "600" : "normal",
                  textAlign: "left",
                  cursor: "pointer",
                  transition: "background 100ms",
                }}
              >
                <FolderIcon size={14} />
                <span className="truncate">{item.name}</span>
              </button>
            );
          })}

          <div style={{ margin: "10px 0", borderTop: "1px solid var(--border)" }} />

          <div className="muted" style={{ fontSize: "11px", fontWeight: "600", textTransform: "uppercase", padding: "4px 8px" }}>
            Current Path Info
          </div>
          <div style={{ padding: "0 8px", fontSize: "12px", color: "var(--text-muted)", lineHeight: "1.5" }}>
            <div>Items: <strong className="mono tabular-nums" style={{ color: "var(--text)" }}>{entries.length}</strong></div>
            <div>Indexed: <strong className="mono tabular-nums" style={{ color: "var(--accent-success)" }}>{project.sourceCount}</strong></div>
          </div>
        </div>

        {/* Center Canvas: File List / Grid */}
        <div style={{ flex: 1, display: "flex", flexDirection: "column", minWidth: 0, overflowY: "auto" }}>
          {error && (
            <div style={{ padding: "12px 16px", background: "rgba(220, 38, 38, 0.1)", color: "#f87171", fontSize: "13px" }}>
              Error: {error}
            </div>
          )}

          {loading ? (
            <div style={{ padding: "32px", textAlign: "center", color: "var(--text-muted)" }}>
              Scanning directory…
            </div>
          ) : filteredEntries.length === 0 ? (
            <div
              style={{
                flex: 1,
                display: "flex",
                flexDirection: "column",
                alignItems: "center",
                justifyContent: "center",
                padding: "48px 24px",
                color: "var(--text-muted)",
                textAlign: "center",
              }}
            >
              <div
                style={{
                  width: "48px",
                  height: "48px",
                  borderRadius: "50%",
                  background: "var(--surface)",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  marginBottom: "12px",
                }}
              >
                <FolderIcon size={24} className="muted" />
              </div>
              <h3 style={{ fontSize: "15px", fontWeight: "600", color: "var(--text)", marginBottom: "6px" }}>
                {query ? "No matching files found" : "This folder is empty"}
              </h3>
              <p style={{ fontSize: "13px", maxWidth: "40ch", marginBottom: "16px" }}>
                {query
                  ? `No items match “${query}”. Clear search to see all files.`
                  : "Use Add files to copy documents in, or create a new file or folder to get started."}
              </p>
              <div style={{ display: "flex", gap: "8px" }}>
                <button
                  type="button"
                  className="btn btn--primary btn--sm"
                  onClick={() => setShowNewFile(true)}
                >
                  <PlusIcon size={14} />
                  <span>Create File</span>
                </button>
                <button
                  type="button"
                  className="btn btn--subtle btn--sm"
                  onClick={() => void handleImport()}
                  disabled={!isTauri}
                  title={
                    isTauri
                      ? "Copy files from your machine into this folder"
                      : "Copying files needs the desktop app"
                  }
                >
                  <span>Add files</span>
                </button>
              </div>
            </div>
          ) : viewMode === "table" ? (
            /* Table / Details View */
            <table
              style={{
                width: "100%",
                borderCollapse: "collapse",
                textAlign: "left",
                fontSize: "13px",
              }}
            >
              <thead>
                <tr
                  style={{
                    background: "var(--surface)",
                    borderBottom: "1px solid var(--border)",
                    color: "var(--text-muted)",
                    fontSize: "11.5px",
                    textTransform: "uppercase",
                    letterSpacing: "0.04em",
                  }}
                >
                  <th style={{ padding: "8px 16px", fontWeight: "600" }}>Name</th>
                  <th style={{ padding: "8px 12px", fontWeight: "600", width: "140px" }}>Kind</th>
                  <th style={{ padding: "8px 12px", fontWeight: "600", width: "100px", textAlign: "right" }}>Size</th>
                  <th style={{ padding: "8px 12px", fontWeight: "600", width: "120px" }}>Modified</th>
                  <th style={{ padding: "8px 16px", fontWeight: "600", width: "110px", textAlign: "right" }}>Actions</th>
                </tr>
              </thead>
              <tbody>
                {filteredEntries.map((entry) => {
                  const cat = getFileCategory(entry.name, entry.isDir);
                  const isSelected = selectedEntry?.path === entry.path;
                  const isProtected = isProtectedRootEntry(entry, currentPath);
                  return (
                    <tr
                      key={entry.path}
                      onClick={() => setSelectedEntry(entry)}
                      onDoubleClick={() => {
                        if (entry.isDir) navigateTo(entry.path);
                        else setPreviewModal(entry);
                      }}
                      style={{
                        background: isSelected
                          ? "var(--accent-primary-soft)"
                          : "transparent",
                        borderBottom: "1px solid var(--border)",
                        cursor: "pointer",
                        transition: "background 80ms",
                      }}
                      className="file-explorer-row"
                    >
                      <td style={{ padding: "8px 16px" }}>
                        <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
                          <span
                            style={{
                              display: "inline-flex",
                              alignItems: "center",
                              justifyContent: "center",
                              width: "28px",
                              height: "28px",
                              borderRadius: "var(--radius-sm)",
                              background: cat.iconBg,
                              color: cat.iconColor,
                              flexShrink: 0,
                            }}
                          >
                            {entry.isDir ? <FolderIcon size={16} /> : <DocIcon size={16} />}
                          </span>
                          <span
                            style={{
                              fontWeight: entry.isDir ? "600" : "500",
                              color: isSelected ? "var(--accent-primary)" : "var(--text)",
                            }}
                            className="truncate"
                            title={entry.name}
                          >
                            {entry.name}
                          </span>
                        </div>
                      </td>
                      <td style={{ padding: "8px 12px", color: "var(--text-muted)", fontSize: "12px" }}>
                        {cat.label}
                      </td>
                      <td style={{ padding: "8px 12px", textAlign: "right", fontFamily: "var(--font-mono)", fontSize: "12px" }}>
                        {entry.isDir ? "—" : formatBytes(entry.sizeBytes)}
                      </td>
                      <td style={{ padding: "8px 12px", color: "var(--text-muted)", fontSize: "12px" }}>
                        {formatRelativeTime(entry.modifiedAt)}
                      </td>
                      <td style={{ padding: "8px 16px", textAlign: "right" }}>
                        <div style={{ display: "inline-flex", alignItems: "center", gap: "4px" }} onClick={(e) => e.stopPropagation()}>
                          {!entry.isDir && (
                            <>
                              <button
                                type="button"
                                className="icon-btn icon-btn--sm"
                                onClick={() => setPreviewModal(entry)}
                                title="Preview"
                              >
                                <DocIcon size={13} />
                              </button>
                              <button
                                type="button"
                                className="icon-btn icon-btn--sm"
                                onClick={() => {
                                  setActiveProjectTab("chat");
                                  setOpenDocument({ projectId: project.id, filePath: entry.path, fileName: entry.name });
                                }}
                                title="Open in Viewer"
                              >
                                <SparkIcon size={13} />
                              </button>
                            </>
                          )}
                          {isProtected ? (
                            <span
                              className="badge"
                              style={{ fontSize: "10px" }}
                              title="Managed by the app; create or delete files inside it instead"
                            >
                              Managed
                            </span>
                          ) : (
                            <>
                              <button
                                type="button"
                                className="icon-btn icon-btn--sm"
                                onClick={() => {
                                  setShowRename(entry);
                                  setRenameVal(entry.name);
                                }}
                                title="Rename"
                              >
                                <PencilIcon size={13} />
                              </button>
                              <button
                                type="button"
                                className="icon-btn icon-btn--sm"
                                onClick={() => void handleCopy(entry)}
                                title="Duplicate"
                              >
                                <CopyIcon size={13} />
                              </button>
                              <button
                                type="button"
                                className="icon-btn icon-btn--sm"
                                onClick={() => setShowDelete(entry)}
                                title="Delete"
                                style={{ color: "#ef4444" }}
                              >
                                <TrashIcon size={13} />
                              </button>
                            </>
                          )}
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          ) : (
            /* Grid / Card View */
            <div
              style={{
                display: "grid",
                gridTemplateColumns: "repeat(auto-fill, minmax(180px, 1fr))",
                gap: "12px",
                padding: "16px",
              }}
            >
              {filteredEntries.map((entry) => {
                const cat = getFileCategory(entry.name, entry.isDir);
                const isSelected = selectedEntry?.path === entry.path;
                return (
                  <div
                    key={entry.path}
                    onClick={() => setSelectedEntry(entry)}
                    onDoubleClick={() => {
                      if (entry.isDir) navigateTo(entry.path);
                      else {
                        setActiveProjectTab("chat");
                        setOpenDocument({ projectId: project.id, filePath: entry.path, fileName: entry.name });
                      }
                    }}
                    style={{
                      display: "flex",
                      flexDirection: "column",
                      padding: "12px",
                      background: isSelected ? "var(--accent-primary-soft)" : "var(--surface)",
                      border: isSelected ? "1.5px solid var(--accent-primary)" : "1px solid var(--border)",
                      borderRadius: "var(--radius-lg)",
                      cursor: "pointer",
                      transition: "all 120ms",
                      position: "relative",
                    }}
                  >
                    <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "8px" }}>
                      <span
                        style={{
                          display: "inline-flex",
                          alignItems: "center",
                          justifyContent: "center",
                          width: "36px",
                          height: "36px",
                          borderRadius: "var(--radius-md)",
                          background: cat.iconBg,
                          color: cat.iconColor,
                        }}
                      >
                        {entry.isDir ? <FolderIcon size={20} /> : <DocIcon size={20} />}
                      </span>
                      <span className="badge" style={{ fontSize: "10px" }}>
                        {cat.typeBadge}
                      </span>
                    </div>

                    <div
                      style={{
                        fontSize: "13px",
                        fontWeight: entry.isDir ? "600" : "500",
                        color: "var(--text)",
                        marginBottom: "4px",
                      }}
                      className="truncate"
                      title={entry.name}
                    >
                      {entry.name}
                    </div>

                    <div style={{ display: "flex", justifyContent: "space-between", fontSize: "11px", color: "var(--text-muted)" }}>
                      <span>{entry.isDir ? "Folder" : formatBytes(entry.sizeBytes)}</span>
                      <span>{formatRelativeTime(entry.modifiedAt)}</span>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        {/* Right Inspector & Preview Pane */}
        {selectedEntry && (
          <div
            style={{
              width: "320px",
              minWidth: "280px",
              background: "var(--background-elevated)",
              borderLeft: "1px solid var(--border)",
              display: "flex",
              flexDirection: "column",
              padding: "16px",
              overflowY: "auto",
              gap: "14px",
            }}
          >
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
              <span className="muted" style={{ fontSize: "11px", fontWeight: "600", textTransform: "uppercase" }}>
                Inspector
              </span>
              <button
                type="button"
                className="icon-btn icon-btn--sm"
                onClick={() => setSelectedEntry(null)}
                title="Close Inspector"
              >
                ×
              </button>
            </div>

            <div style={{ display: "flex", alignItems: "center", gap: "12px" }}>
              {(() => {
                const cat = getFileCategory(selectedEntry.name, selectedEntry.isDir);
                return (
                  <span
                    style={{
                      display: "inline-flex",
                      alignItems: "center",
                      justifyContent: "center",
                      width: "44px",
                      height: "44px",
                      borderRadius: "var(--radius-lg)",
                      background: cat.iconBg,
                      color: cat.iconColor,
                      flexShrink: 0,
                    }}
                  >
                    {selectedEntry.isDir ? <FolderIcon size={24} /> : <DocIcon size={24} />}
                  </span>
                );
              })()}
              <div style={{ minWidth: 0, flex: 1 }}>
                <div style={{ fontWeight: "600", fontSize: "14px" }} className="truncate" title={selectedEntry.name}>
                  {selectedEntry.name}
                </div>
                <div style={{ fontSize: "12px", color: "var(--text-muted)" }}>
                  {getFileCategory(selectedEntry.name, selectedEntry.isDir).label}
                </div>
              </div>
            </div>

            <div
              style={{
                display: "flex",
                flexDirection: "column",
                gap: "8px",
                padding: "10px",
                background: "var(--surface)",
                border: "1px solid var(--border)",
                borderRadius: "var(--radius-md)",
                fontSize: "12px",
              }}
            >
              <div style={{ display: "flex", justifyContent: "space-between" }}>
                <span className="muted">Relative Path:</span>
                <span className="mono truncate" style={{ maxWidth: "160px" }} title={selectedEntry.path}>
                  {selectedEntry.path || "root"}
                </span>
              </div>
              <div style={{ display: "flex", justifyContent: "space-between" }}>
                <span className="muted">Size on Disk:</span>
                <span className="mono">{selectedEntry.isDir ? "Folder" : formatBytes(selectedEntry.sizeBytes)}</span>
              </div>
              <div style={{ display: "flex", justifyContent: "space-between" }}>
                <span className="muted">Last Modified:</span>
                <span>{new Date(selectedEntry.modifiedAt).toLocaleDateString()}</span>
              </div>
              <div style={{ display: "flex", justifyContent: "space-between" }}>
                <span className="muted">Local RAG:</span>
                <span style={{ color: "var(--accent-success)", fontWeight: "500" }}>Indexed</span>
              </div>
            </div>

            {/* Live Content Preview */}
            <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                <span className="muted" style={{ fontSize: "11px", fontWeight: "600", textTransform: "uppercase" }}>
                  Preview
                </span>
                {!selectedEntry.isDir && (
                  <>
                    <button
                      type="button"
                      onClick={() => setPreviewModal(selectedEntry)}
                      style={{ fontSize: "11px", color: "var(--accent-primary)", cursor: "pointer", marginRight: "6px" }}
                    >
                      Open Modal ↗
                    </button>
                    <button
                      type="button"
                      onClick={() => {
                        setActiveProjectTab("chat");
                        setOpenDocument({ projectId: project.id, filePath: selectedEntry.path, fileName: selectedEntry.name });
                      }}
                      style={{ fontSize: "11px", color: "var(--accent-primary)", cursor: "pointer" }}
                    >
                      Open in Viewer ↗
                    </button>
                  </>
                )}
              </div>
              <div
                style={{
                  background: "var(--surface)",
                  border: "1px solid var(--border)",
                  borderRadius: "var(--radius-md)",
                  padding: "10px",
                  fontSize: "12px",
                  lineHeight: 1.5,
                  maxHeight: "180px",
                  overflowY: "auto",
                  whiteSpace: "pre-wrap",
                  fontFamily: "var(--font-mono)",
                  color: "var(--text-secondary)",
                }}
              >
                {getSamplePreview(selectedEntry.name, selectedEntry.isDir, project)}
              </div>
            </div>

            {/* Action Buttons */}
            <div style={{ display: "flex", flexDirection: "column", gap: "6px", marginTop: "auto" }}>
              <button
                type="button"
                className="btn btn--subtle btn--sm"
                onClick={() => {
                  if (selectedEntry.isDir) navigateTo(selectedEntry.path);
                  else setPreviewModal(selectedEntry);
                }}
              >
                {selectedEntry.isDir ? "Open Folder" : "Preview Document"}
              </button>
              {!selectedEntry.isDir && (
                <button
                  type="button"
                  className="btn btn--subtle btn--sm"
                  onClick={() => {
                    setActiveProjectTab("chat");
                    setOpenDocument({ projectId: project.id, filePath: selectedEntry.path, fileName: selectedEntry.name });
                  }}
                >
                  Open in Viewer
                </button>
              )}
              {isProtectedRootEntry(selectedEntry, currentPath) ? (
                <span className="muted" style={{ fontSize: "12px", lineHeight: 1.5 }}>
                  This folder is managed by the app. Create or delete files inside it
                  instead.
                </span>
              ) : (
                <>
                  <button
                    type="button"
                    className="btn btn--subtle btn--sm"
                    onClick={() => {
                      setShowRename(selectedEntry);
                      setRenameVal(selectedEntry.name);
                    }}
                  >
                    Rename
                  </button>
                  <button
                    type="button"
                    className="btn btn--subtle btn--sm"
                    onClick={() => setShowDelete(selectedEntry)}
                    style={{ color: "#ef4444" }}
                  >
                    Delete
                  </button>
                </>
              )}
            </div>
          </div>
        )}
      </div>

      {/* MODAL: New Folder */}
      {showNewFolder && (
        <div
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(0, 0, 0, 0.6)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 100,
          }}
          onClick={() => setShowNewFolder(false)}
        >
          <div
            style={{
              background: "var(--surface)",
              border: "1px solid var(--border)",
              borderRadius: "var(--radius-lg)",
              padding: "24px",
              width: "100%",
              maxWidth: "400px",
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <h3 style={{ fontSize: "16px", fontWeight: "600", marginBottom: "12px" }}>Create New Folder</h3>
            <p className="muted" style={{ fontSize: "13px", marginBottom: "16px" }}>
              Inside: {currentPath || project.name}
            </p>
            <input
              type="text"
              autoFocus
              value={newFolderName}
              onChange={(e) => setNewFolderName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void handleCreateFolder();
                if (e.key === "Escape") setShowNewFolder(false);
              }}
              placeholder="e.g. contracts, references, benchmarks"
              style={{
                width: "100%",
                padding: "8px 12px",
                borderRadius: "var(--radius-md)",
                border: "1px solid var(--border)",
                background: "var(--background)",
                color: "var(--text)",
                fontSize: "14px",
                marginBottom: "20px",
              }}
            />
            <div style={{ display: "flex", justifyContent: "flex-end", gap: "8px" }}>
              <button type="button" className="btn btn--subtle" onClick={() => setShowNewFolder(false)}>
                Cancel
              </button>
              <button type="button" className="btn btn--primary" onClick={() => void handleCreateFolder()}>
                Create Folder
              </button>
            </div>
          </div>
        </div>
      )}

      {/* MODAL: New File */}
      {showNewFile && (
        <div
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(0, 0, 0, 0.6)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 100,
          }}
          onClick={() => setShowNewFile(false)}
        >
          <div
            style={{
              background: "var(--surface)",
              border: "1px solid var(--border)",
              borderRadius: "var(--radius-lg)",
              padding: "24px",
              width: "100%",
              maxWidth: "420px",
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <h3 style={{ fontSize: "16px", fontWeight: "600", marginBottom: "12px" }}>Create New Document</h3>
            <p className="muted" style={{ fontSize: "13px", marginBottom: "16px" }}>
              Folder: {currentPath || project.name}
            </p>

            <div style={{ marginBottom: "12px" }}>
              <label style={{ display: "block", fontSize: "12px", fontWeight: "500", marginBottom: "4px" }}>
                File Name
              </label>
              <input
                type="text"
                autoFocus
                value={newFileName}
                onChange={(e) => setNewFileName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void handleCreateFile();
                  if (e.key === "Escape") setShowNewFile(false);
                }}
                placeholder="e.g. analysis-q3, meeting-notes"
                style={{
                  width: "100%",
                  padding: "8px 12px",
                  borderRadius: "var(--radius-md)",
                  border: "1px solid var(--border)",
                  background: "var(--background)",
                  color: "var(--text)",
                  fontSize: "14px",
                }}
              />
            </div>

            <div style={{ marginBottom: "20px" }}>
              <label style={{ display: "block", fontSize: "12px", fontWeight: "500", marginBottom: "6px" }}>
                Document Format
              </label>
              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "6px" }}>
                {[
                  { ext: ".md", label: "Markdown (.md)" },
                  { ext: ".txt", label: "Plain Text (.txt)" },
                  { ext: ".csv", label: "Spreadsheet (.csv)" },
                  { ext: ".json", label: "JSON Data (.json)" },
                ].map((item) => (
                  <button
                    key={item.ext}
                    type="button"
                    onClick={() => setNewFileExt(item.ext)}
                    style={{
                      padding: "6px 10px",
                      fontSize: "12px",
                      borderRadius: "var(--radius-md)",
                      border: newFileExt === item.ext ? "1.5px solid var(--accent-primary)" : "1px solid var(--border)",
                      background: newFileExt === item.ext ? "var(--accent-primary-soft)" : "var(--background)",
                      color: newFileExt === item.ext ? "var(--accent-primary)" : "var(--text)",
                      cursor: "pointer",
                      textAlign: "left",
                    }}
                  >
                    {item.label}
                  </button>
                ))}
              </div>
            </div>

            <div style={{ display: "flex", justifyContent: "flex-end", gap: "8px" }}>
              <button type="button" className="btn btn--subtle" onClick={() => setShowNewFile(false)}>
                Cancel
              </button>
              <button type="button" className="btn btn--primary" onClick={() => void handleCreateFile()}>
                Create File
              </button>
            </div>
          </div>
        </div>
      )}

      {/* MODAL: Rename */}
      {showRename && (
        <div
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(0, 0, 0, 0.6)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 100,
          }}
          onClick={() => setShowRename(null)}
        >
          <div
            style={{
              background: "var(--surface)",
              border: "1px solid var(--border)",
              borderRadius: "var(--radius-lg)",
              padding: "24px",
              width: "100%",
              maxWidth: "400px",
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <h3 style={{ fontSize: "16px", fontWeight: "600", marginBottom: "12px" }}>
              Rename {showRename.isDir ? "Folder" : "File"}
            </h3>
            <input
              type="text"
              autoFocus
              value={renameVal}
              onChange={(e) => setRenameVal(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void handleRename();
                if (e.key === "Escape") setShowRename(null);
              }}
              style={{
                width: "100%",
                padding: "8px 12px",
                borderRadius: "var(--radius-md)",
                border: "1px solid var(--border)",
                background: "var(--background)",
                color: "var(--text)",
                fontSize: "14px",
                marginBottom: "20px",
              }}
            />
            <div style={{ display: "flex", justifyContent: "flex-end", gap: "8px" }}>
              <button type="button" className="btn btn--subtle" onClick={() => setShowRename(null)}>
                Cancel
              </button>
              <button type="button" className="btn btn--primary" onClick={() => void handleRename()}>
                Save
              </button>
            </div>
          </div>
        </div>
      )}

      {/* MODAL: Delete */}
      {showDelete && (
        <div
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(0, 0, 0, 0.6)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 100,
          }}
          onClick={() => setShowDelete(null)}
        >
          <div
            style={{
              background: "var(--surface)",
              border: "1px solid var(--border)",
              borderRadius: "var(--radius-lg)",
              padding: "24px",
              width: "100%",
              maxWidth: "400px",
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <h3 style={{ fontSize: "16px", fontWeight: "600", color: "#ef4444", marginBottom: "10px" }}>
              Delete {showDelete.isDir ? "Folder" : "File"}?
            </h3>
            <p style={{ fontSize: "13px", color: "var(--text-secondary)", marginBottom: "20px", lineHeight: "1.5" }}>
              Are you sure you want to delete <strong>{showDelete.name}</strong>?
              {showDelete.isDir && " All files and subfolders inside will also be removed."}
            </p>
            <div style={{ display: "flex", justifyContent: "flex-end", gap: "8px" }}>
              <button type="button" className="btn btn--subtle" onClick={() => setShowDelete(null)}>
                Cancel
              </button>
              <button
                type="button"
                className="btn btn--primary"
                onClick={() => void handleDelete()}
                style={{ background: "#dc2626", color: "#fff" }}
              >
                Delete
              </button>
            </div>
          </div>
        </div>
      )}

      {/* MODAL: Fullscreen Document Preview */}
      {previewModal && (
        <div
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(0, 0, 0, 0.75)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 110,
            padding: "24px",
          }}
          onClick={() => setPreviewModal(null)}
        >
          <div
            style={{
              background: "var(--background)",
              border: "1px solid var(--border)",
              borderRadius: "var(--radius-xl)",
              width: "100%",
              maxWidth: "780px",
              maxHeight: "85vh",
              display: "flex",
              flexDirection: "column",
              overflow: "hidden",
              boxShadow: "0 20px 40px rgba(0,0,0,0.5)",
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                padding: "14px 20px",
                background: "var(--background-elevated)",
                borderBottom: "1px solid var(--border)",
              }}
            >
              <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
                <DocIcon size={18} style={{ color: "var(--accent-primary)" }} />
                <div>
                  <div style={{ fontWeight: "600", fontSize: "15px" }}>{previewModal.name}</div>
                  <div className="muted" style={{ fontSize: "11px" }}>
                    {formatBytes(previewModal.sizeBytes)} · Modified {new Date(previewModal.modifiedAt).toLocaleString()}
                  </div>
                </div>
              </div>
              <button
                type="button"
                className="icon-btn icon-btn--sm"
                onClick={() => setPreviewModal(null)}
              >
                ✕
              </button>
            </div>

            <div
              style={{
                flex: 1,
                padding: "24px",
                overflowY: "auto",
                fontFamily: "var(--font-mono)",
                fontSize: "13px",
                lineHeight: "1.7",
                whiteSpace: "pre-wrap",
                color: "var(--text)",
                background: "var(--surface)",
              }}
            >
              {getSamplePreview(previewModal.name, previewModal.isDir, project)}
            </div>

            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                padding: "12px 20px",
                background: "var(--background-elevated)",
                borderTop: "1px solid var(--border)",
                fontSize: "12px",
              }}
            >
              <span className="muted">Status: Indexed for private local citations</span>
              <button
                type="button"
                className="btn btn--primary btn--sm"
                onClick={() => setPreviewModal(null)}
              >
                Close Preview
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
