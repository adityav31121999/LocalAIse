import { useState } from "react";
import { invoke, isTauri, delay } from "../../api";
import type { Project } from "../../types";

export interface ResearchStep {
  stepNumber: number;
  title: string;
  query: string;
  sourceType: "local" | "web";
  completed: boolean;
  findings?: string;
}

export interface ResearchPlan {
  topic: string;
  maxSteps: number;
  steps: ResearchStep[];
  status: "planned" | "approved" | "running" | "completed";
}

export interface ResearchReportResult {
  topic: string;
  reportMarkdown: string;
  savedToFile?: string;
  sourcesUsed: string[];
}

export function ResearchPlanView({ project }: { project: Project }) {
  const [topic, setTopic] = useState("");
  const [plan, setPlan] = useState<ResearchPlan | null>(null);
  const [report, setReport] = useState<ResearchReportResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [executing, setExecuting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleGeneratePlan = async () => {
    if (!topic.trim()) return;
    setLoading(true);
    setError(null);
    setPlan(null);
    setReport(null);

    try {
      if (isTauri) {
        const generated = await invoke<ResearchPlan>("create_research_plan", {
          projectId: project.id,
          topic: topic.trim(),
        });
        setPlan(generated);
      } else {
        await delay(400);
        setPlan({
          topic: topic.trim(),
          maxSteps: 3,
          status: "planned",
          steps: [
            { stepNumber: 1, title: "Search local project files", query: topic, sourceType: "local", completed: false },
            { stepNumber: 2, title: "Synthesize findings into cited report", query: "synthesize", sourceType: "local", completed: false },
          ],
        });
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  const handleApproveAndExecute = async () => {
    if (!plan) return;
    setExecuting(true);
    setError(null);

    try {
      if (isTauri) {
        const res = await invoke<ResearchReportResult>("execute_research_plan", {
          projectId: project.id,
          plan,
        });
        setReport(res);
      } else {
        await delay(1200);
        setReport({
          topic: plan.topic,
          reportMarkdown: `# Research Report: ${plan.topic}\n\n## Executive Summary\nAnalysis generated from local sources.\n\n## Sources\n- project-notes.md`,
          savedToFile: "outputs/research-sample.md",
          sourcesUsed: ["project-notes.md"],
        });
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setExecuting(false);
    }
  };

  return (
    <div className="panel" style={{ padding: "20px", height: "100%", overflowY: "auto" }}>
      <div style={{ marginBottom: "20px" }}>
        <h2 style={{ fontSize: "18px", margin: "0 0 6px 0" }}>Deep Research Agent</h2>
        <p className="muted" style={{ margin: 0, fontSize: "13px" }}>
          Autonomous multi-step investigation: plan generation, user approval gate, search & read, and synthesized report with citations.
        </p>
      </div>

      {/* Topic input */}
      <div style={{ display: "flex", gap: "10px", marginBottom: "20px" }}>
        <input
          type="text"
          placeholder="What topic or question should the agent investigate?"
          value={topic}
          onChange={(e) => setTopic(e.target.value)}
          style={{ flex: 1, padding: "8px 12px", borderRadius: "6px", border: "1px solid var(--border)", background: "var(--surface)", fontSize: "14px" }}
        />
        <button
          className="btn btn--primary"
          onClick={handleGeneratePlan}
          disabled={loading || executing || !topic.trim()}
          style={{ padding: "8px 16px" }}
        >
          {loading ? "Generating Plan…" : "Generate Plan"}
        </button>
      </div>

      {error && (
        <div style={{ padding: "12px", borderRadius: "6px", background: "rgba(255, 80, 80, 0.1)", color: "#ff6b6b", marginBottom: "16px" }}>
          {error}
        </div>
      )}

      {/* Proposed Plan with Approval Checkpoint */}
      {plan && !report && (
        <div style={{ background: "var(--surface)", border: "1px solid var(--border)", borderRadius: "8px", padding: "16px", marginBottom: "20px" }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "12px" }}>
            <h3 style={{ fontSize: "15px", margin: 0 }}>Proposed Research Plan</h3>
            <span className="badge">
              {project.internetEnabled ? "🌐 Internet On" : "🔒 Local Only"}
            </span>
          </div>

          <div style={{ display: "flex", flexDirection: "column", gap: "10px", marginBottom: "16px" }}>
            {plan.steps.map((step) => (
              <div
                key={step.stepNumber}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "10px",
                  padding: "10px",
                  background: "var(--surface-raised)",
                  borderRadius: "6px",
                  fontSize: "13px",
                }}
              >
                <span style={{ fontWeight: 700, width: "24px" }}>#{step.stepNumber}</span>
                <div style={{ flex: 1 }}>
                  <div><strong>{step.title}</strong></div>
                  <span className="muted" style={{ fontSize: "12px" }}>
                    Source: {step.sourceType === "web" ? "External Web" : "Local Indexed Folders"} · Query: <code>{step.query}</code>
                  </span>
                </div>
              </div>
            ))}
          </div>

          <div style={{ display: "flex", gap: "10px" }}>
            <button
              className="btn btn--primary"
              onClick={handleApproveAndExecute}
              disabled={executing}
              style={{ padding: "8px 18px" }}
            >
              {executing ? "Agent Executing Steps…" : "Approve Plan & Run Agent"}
            </button>
            <button
              className="btn btn--subtle"
              onClick={() => setPlan(null)}
              disabled={executing}
              style={{ padding: "8px 14px" }}
            >
              Cancel
            </button>
          </div>
        </div>
      )}

      {/* Completed Report */}
      {report && (
        <div style={{ background: "var(--surface)", border: "1px solid var(--border)", borderRadius: "8px", padding: "16px" }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "12px" }}>
            <h3 style={{ fontSize: "16px", margin: 0 }}>Final Research Synthesis</h3>
            {report.savedToFile && (
              <span className="badge" style={{ background: "rgba(102, 217, 181, 0.15)", color: "var(--accent)" }}>
                Saved: {report.savedToFile}
              </span>
            )}
          </div>

          <div style={{ fontSize: "12px", marginBottom: "12px", color: "var(--text-secondary)" }}>
            Sources examined ({report.sourcesUsed.length}): {report.sourcesUsed.join(", ")}
          </div>

          <div style={{ whiteSpace: "pre-wrap", lineHeight: "1.6", fontSize: "14px" }}>
            {report.reportMarkdown}
          </div>
        </div>
      )}
    </div>
  );
}
