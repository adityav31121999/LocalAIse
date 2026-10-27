import { useEffect, useMemo, useState } from "react";
import type { ComponentType } from "react";
import { analyzeDocument, type AnalysisRequest } from "../../api/analysis";
import { useProjectStore } from "../../state/projectStore";
import { Markdown } from "../../util/markdown";
import {
  CheckIcon,
  CopyIcon,
  DocIcon,
  DownloadIcon,
  HistoryIcon,
  LayersIcon,
  ShieldIcon,
  SparkIcon,
  TableIcon,
} from "../ui/icons";
import type { Project } from "../../types";

type AnalysisAction = AnalysisRequest["action"];

interface ActionPreset {
  id: AnalysisAction;
  label: string;
  hint: string;
  icon: ComponentType<{ size?: number }>;
}

const ACTION_PRESETS: ActionPreset[] = [
  {
    id: "summarize",
    label: "Summarize",
    hint: "Condense the whole document with map-reduce.",
    icon: SparkIcon,
  },
  {
    id: "extract_tables",
    label: "Extract tables",
    hint: "Pull out figures, tables and numeric claims.",
    icon: TableIcon,
  },
  {
    id: "flag_inconsistencies",
    label: "Flag inconsistencies",
    hint: "Find contradictions, gaps and shaky claims.",
    icon: ShieldIcon,
  },
  {
    id: "compare",
    label: "Compare two docs",
    hint: "Line up two sources and report the differences.",
    icon: LayersIcon,
  },
];

const ACTION_LABEL: Record<AnalysisAction, string> = {
  summarize: "Summary",
  extract_tables: "Tables & figures",
  flag_inconsistencies: "Inconsistencies",
  compare: "Comparison",
};

interface AnalysisEntry {
  id: string;
  action: AnalysisAction;
  fileName: string;
  content: string;
  savedPath?: string;
  createdAt: number;
}

function downloadText(fileName: string, text: string, mime: string) {
  const blob = new Blob([text], { type: mime });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = fileName;
  anchor.click();
  URL.revokeObjectURL(url);
}

function printMarkdown(title: string, text: string) {
  const win = window.open("", "_blank", "width=860,height=900");
  if (!win) return;
  const escaped = text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
  win.document.write(
    `<!doctype html><html><head><meta charset="utf-8"><title>${title}</title>` +
      `<style>body{font-family:ui-sans-serif,system-ui,sans-serif;line-height:1.6;max-width:760px;margin:40px auto;padding:0 24px;white-space:pre-wrap;}</style>` +
      `</head><body><h1>${title}</h1><pre>${escaped}</pre></body></html>`,
  );
  win.document.close();
  win.focus();
  win.print();
}

export function ReportView({ project }: { project: Project }) {
  const sources = useProjectStore((state) => state.sources);
  const [selectedFile, setSelectedFile] = useState<string>(sources[0]?.name || "");
  const [compareFile, setCompareFile] = useState<string>(sources[1]?.name || "");
  const [action, setAction] = useState<AnalysisAction>("summarize");
  const [saveToOutputs, setSaveToOutputs] = useState(true);
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const [savedPath, setSavedPath] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [history, setHistory] = useState<AnalysisEntry[]>([]);
  const [activeEntryId, setActiveEntryId] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const activeEntry = useMemo(
    () => history.find((entry) => entry.id === activeEntryId) ?? null,
    [history, activeEntryId],
  );
  const shownAction = activeEntry?.action ?? action;
  const shownFile = activeEntry?.fileName ?? selectedFile;

  const compareOptions = useMemo(
    () => sources.filter((source) => source.name !== selectedFile),
    [sources, selectedFile],
  );

  // Sources can arrive after the tab mounts; adopt the first one so the run
  // button is usable without touching the dropdown.
  useEffect(() => {
    if (!selectedFile && sources[0]) setSelectedFile(sources[0].name);
  }, [sources, selectedFile]);

  useEffect(() => {
    if (!compareFile || compareFile === selectedFile) {
      const next = sources.find((source) => source.name !== selectedFile);
      if (next) setCompareFile(next.name);
    }
  }, [sources, selectedFile, compareFile]);

  const handleRunAnalysis = async () => {
    if (!selectedFile) return;
    setLoading(true);
    setError(null);
    setSavedPath(null);
    setActiveEntryId(null);

    const defaultSaveName = `${action}-${selectedFile.replace(/[^a-zA-Z0-9]/g, "_")}.md`;
    const compareWith =
      action === "compare" && compareFile && compareFile !== selectedFile ? compareFile : undefined;

    try {
      const res = await analyzeDocument({
        projectId: project.id,
        action,
        fileName: selectedFile,
        compareWith,
        saveAs: saveToOutputs ? defaultSaveName : undefined,
      });
      setResult(res.content);
      setSavedPath(res.savedToFile ?? null);
      const entry: AnalysisEntry = {
        id: `${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
        action,
        fileName: selectedFile,
        content: res.content,
        savedPath: res.savedToFile,
        createdAt: Date.now(),
      };
      setHistory((previous) => [entry, ...previous].slice(0, 12));
      setActiveEntryId(entry.id);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  const handleCopy = async () => {
    const text = activeEntry?.content ?? result;
    if (!text) return;
    await navigator.clipboard.writeText(text);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1400);
  };

  const handleDownload = () => {
    const text = activeEntry?.content ?? result;
    if (!text) return;
    const stem = `${shownAction}-${shownFile.replace(/[^a-zA-Z0-9]/g, "_")}`;
    downloadText(`${stem}.md`, text, "text/markdown");
  };

  const handleExportJson = () => {
    const text = activeEntry?.content ?? result;
    if (!text) return;
    const payload = {
      project: project.name,
      action: shownAction,
      document: shownFile,
      savedPath: activeEntry?.savedPath ?? savedPath,
      createdAt: new Date().toISOString(),
      content: text,
    };
    downloadText(
      `${shownAction}-${shownFile.replace(/[^a-zA-Z0-9]/g, "_")}.json`,
      JSON.stringify(payload, null, 2),
      "application/json",
    );
  };

  return (
    <div className="workbench">
      <div className="workbench__col">
        <div className="workbench__panel">
          <div className="workbench__title">
            <SparkIcon size={15} />
            Document analysis
          </div>
          <p className="workbench__lede">
            Run one focused pass over a source and get a cited answer back. Everything
            is generated on this machine.
          </p>

          <div className="preset-grid">
            {ACTION_PRESETS.map((preset) => {
              const Icon = preset.icon;
              return (
                <button
                  key={preset.id}
                  type="button"
                  className="preset"
                  aria-pressed={action === preset.id}
                  onClick={() => setAction(preset.id)}
                  disabled={loading}
                >
                  <span className="preset__icon">
                    <Icon size={15} />
                  </span>
                  <span className="preset__label">{preset.label}</span>
                  <span className="preset__hint">{preset.hint}</span>
                </button>
              );
            })}
          </div>

          <div className="field-row">
            <label htmlFor="analysis-file">Target document</label>
            <select
              id="analysis-file"
              className="select"
              value={selectedFile}
              onChange={(event) => setSelectedFile(event.target.value)}
              disabled={loading}
            >
              {sources.length === 0 ? <option value="">No indexed sources yet</option> : null}
              {sources.map((source) => (
                <option key={source.id} value={source.name}>
                  {source.name}
                </option>
              ))}
            </select>
          </div>

          {action === "compare" ? (
            <div className="field-row">
              <label htmlFor="analysis-compare">Compare with</label>
              <select
                id="analysis-compare"
                className="select"
                value={compareFile}
                onChange={(event) => setCompareFile(event.target.value)}
                disabled={loading}
              >
                {compareOptions.length === 0 ? (
                  <option value="">Add another source to compare</option>
                ) : null}
                {compareOptions.map((source) => (
                  <option key={source.id} value={source.name}>
                    {source.name}
                  </option>
                ))}
              </select>
            </div>
          ) : null}

          <label className="runrow" style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
            <input
              type="checkbox"
              checked={saveToOutputs}
              onChange={(event) => setSaveToOutputs(event.target.checked)}
            />
            Save results to <code className="mono">outputs/</code>
          </label>

          <div className="workbench__actions">
            <button
              type="button"
              className="btn btn--primary"
              onClick={() => void handleRunAnalysis()}
              disabled={loading || !selectedFile}
            >
              <SparkIcon size={14} />
              {loading ? "Analyzing…" : "Run analysis"}
            </button>
            <span className="muted" style={{ fontSize: "var(--text-xs)" }}>
              {ACTION_LABEL[action]}
            </span>
          </div>

          {error ? (
            <div className="doc-recovery">
              <div className="doc-recovery__title">Analysis failed</div>
              <p className="doc-recovery__cause">{error}</p>
              <div className="doc-recovery__detail">
                Check that the model is running and the document is indexed.
              </div>
            </div>
          ) : null}
        </div>

        <div className="workbench__panel">
          <div className="workbench__title">
            <HistoryIcon size={15} />
            Output history
          </div>
          {history.length === 0 ? (
            <p className="workbench__lede">
              Runs from this session will collect here with quick exports.
            </p>
          ) : (
            <div className="history-list">
              {history.map((entry) => (
                <div
                  key={entry.id}
                  className="history-item"
                  aria-current={entry.id === activeEntryId}
                >
                  <button
                    type="button"
                    className="history-item__main"
                    onClick={() => {
                      setActiveEntryId(entry.id);
                      setResult(entry.content);
                      setSavedPath(entry.savedPath ?? null);
                    }}
                  >
                    <span>{ACTION_LABEL[entry.action]} · {entry.fileName}</span>
                    <span className="history-item__meta">
                      {new Date(entry.createdAt).toLocaleTimeString([], {
                        hour: "2-digit",
                        minute: "2-digit",
                      })}
                      {entry.savedPath ? ` · ${entry.savedPath}` : ""}
                    </span>
                  </button>
                  <button
                    type="button"
                    className="icon-btn icon-btn--sm"
                    title="Copy this output"
                    onClick={() => void navigator.clipboard.writeText(entry.content)}
                  >
                    <CopyIcon size={13} />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      <div className="workbench__col">
        <div className="workbench__panel" style={{ flex: 1 }}>
          <div className="report-toolbar">
            <span className="workbench__title">
              {ACTION_LABEL[shownAction]} · {shownFile || "No document"}
            </span>
            <span className="spacer" style={{ flex: 1 }} />
            <button
              type="button"
              className="btn btn--subtle btn--sm"
              onClick={() => void handleCopy()}
              disabled={!result}
              title="Copy output"
            >
              {copied ? <CheckIcon size={13} /> : <CopyIcon size={13} />}
              {copied ? "Copied" : "Copy"}
            </button>
            <button
              type="button"
              className="btn btn--subtle btn--sm"
              onClick={handleDownload}
              disabled={!result}
              title="Download as Markdown"
            >
              <DownloadIcon size={13} />
              .md
            </button>
            <button
              type="button"
              className="btn btn--subtle btn--sm"
              onClick={handleExportJson}
              disabled={!result}
              title="Download as JSON"
            >
              <DownloadIcon size={13} />
              .json
            </button>
            <button
              type="button"
              className="btn btn--subtle btn--sm"
              onClick={() => result && printMarkdown(`${ACTION_LABEL[shownAction]} · ${shownFile}`, result)}
              disabled={!result}
              title="Open a print-friendly version (save as PDF)"
            >
              <DocIcon size={13} />
              Print
            </button>
          </div>

          {savedPath ? (
            <div className="pill pill--ok" style={{ alignSelf: "flex-start" }}>
              <CheckIcon size={11} />
              Saved to {savedPath}
            </div>
          ) : null}

          <div className="workbench__output report-body">
            {loading ? (
              <div className="output-empty">
                <SparkIcon size={26} />
                <strong>Analyzing {selectedFile}…</strong>
                <span>The model is reading the document in chunks.</span>
              </div>
            ) : result ? (
              <Markdown text={result} />
            ) : (
              <div className="output-empty">
                <SparkIcon size={26} />
                <strong>No analysis yet</strong>
                <span>Pick a preset on the left, choose a document, then run it.</span>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
