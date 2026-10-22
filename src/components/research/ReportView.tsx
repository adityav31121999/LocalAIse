import { useState } from "react";
import { analyzeDocument, AnalysisRequest } from "../../api/analysis";
import { useProjectStore } from "../../state/projectStore";
import { Markdown } from "../../util/markdown";
import type { Project } from "../../types";

export function ReportView({ project }: { project: Project }) {
  const sources = useProjectStore((state) => state.sources);
  const [selectedFile, setSelectedFile] = useState<string>(sources[0]?.name || "");
  const [compareFile, setCompareFile] = useState<string>(sources[1]?.name || "");
  const [action, setAction] = useState<AnalysisRequest["action"]>("summarize");
  const [saveToOutputs, setSaveToOutputs] = useState(true);
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const [savedPath, setSavedPath] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleRunAnalysis = async () => {
    if (!selectedFile) return;
    setLoading(true);
    setError(null);
    setResult(null);
    setSavedPath(null);

    const defaultSaveName = `${action}-${selectedFile.replace(/[^a-zA-Z0-9]/g, "_")}.md`;

    try {
      const res = await analyzeDocument({
        projectId: project.id,
        action,
        fileName: selectedFile,
        compareWith: action === "compare" ? compareFile : undefined,
        saveAs: saveToOutputs ? defaultSaveName : undefined,
      });
      setResult(res.content);
      if (res.savedToFile) {
        setSavedPath(res.savedToFile);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="panel" style={{ padding: "20px", height: "100%", overflowY: "auto" }}>
      <div style={{ marginBottom: "20px" }}>
        <h2 style={{ fontSize: "18px", margin: "0 0 6px 0" }}>Document Analysis</h2>
        <p className="muted" style={{ margin: 0, fontSize: "13px" }}>
          Summarize, extract tables, flag contradictions, or compare documents using local inference.
        </p>
      </div>

      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "16px", marginBottom: "16px" }}>
        <div>
          <label style={{ display: "block", fontSize: "12px", marginBottom: "6px", fontWeight: 500 }}>
            Analysis Action
          </label>
          <select
            value={action}
            onChange={(e) => setAction(e.target.value as AnalysisRequest["action"])}
            style={{ width: "100%", padding: "8px", borderRadius: "6px", border: "1px solid var(--border)", background: "var(--surface)" }}
          >
            <option value="summarize">Summarize (Map-Reduce)</option>
            <option value="extract_tables">Extract Tables</option>
            <option value="flag_inconsistencies">Flag Inconsistencies</option>
            <option value="compare">Compare Documents</option>
          </select>
        </div>

        <div>
          <label style={{ display: "block", fontSize: "12px", marginBottom: "6px", fontWeight: 500 }}>
            Target Document
          </label>
          <select
            value={selectedFile}
            onChange={(e) => setSelectedFile(e.target.value)}
            style={{ width: "100%", padding: "8px", borderRadius: "6px", border: "1px solid var(--border)", background: "var(--surface)" }}
          >
            {sources.map((s) => (
              <option key={s.id} value={s.name}>
                {s.name}
              </option>
            ))}
          </select>
        </div>
      </div>

      {action === "compare" && (
        <div style={{ marginBottom: "16px" }}>
          <label style={{ display: "block", fontSize: "12px", marginBottom: "6px", fontWeight: 500 }}>
            Compare With Document
          </label>
          <select
            value={compareFile}
            onChange={(e) => setCompareFile(e.target.value)}
            style={{ width: "100%", padding: "8px", borderRadius: "6px", border: "1px solid var(--border)", background: "var(--surface)" }}
          >
            {sources
              .filter((s) => s.name !== selectedFile)
              .map((s) => (
                <option key={s.id} value={s.name}>
                  {s.name}
                </option>
              ))}
          </select>
        </div>
      )}

      <div style={{ display: "flex", alignItems: "center", gap: "12px", marginBottom: "20px" }}>
        <label style={{ display: "flex", alignItems: "center", gap: "6px", fontSize: "13px", cursor: "pointer" }}>
          <input
            type="checkbox"
            checked={saveToOutputs}
            onChange={(e) => setSaveToOutputs(e.target.checked)}
          />
          Automatically save results to <code>outputs/</code>
        </label>

        <button
          className="btn btn--primary"
          onClick={handleRunAnalysis}
          disabled={loading || !selectedFile}
          style={{ padding: "8px 16px", cursor: "pointer" }}
        >
          {loading ? "Analyzing Document…" : "Run Analysis"}
        </button>
      </div>

      {error && (
        <div style={{ padding: "12px", borderRadius: "6px", background: "rgba(255, 80, 80, 0.1)", color: "#ff6b6b", marginBottom: "16px" }}>
          {error}
        </div>
      )}

      {savedPath && (
        <div style={{ padding: "8px 12px", borderRadius: "6px", background: "rgba(102, 217, 181, 0.1)", color: "var(--accent)", marginBottom: "16px", fontSize: "13px" }}>
          Saved output report to: <strong>{savedPath}</strong>
        </div>
      )}

      {result && (
        <div style={{ background: "var(--surface)", border: "1px solid var(--border)", borderRadius: "8px", padding: "16px" }}>
          <h3 style={{ fontSize: "15px", marginTop: 0 }}>Analysis Output</h3>
          <div style={{ lineHeight: "1.6", fontSize: "14px" }}>
            <Markdown text={result} />
          </div>
        </div>
      )}
    </div>
  );
}
