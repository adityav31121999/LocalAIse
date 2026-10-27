import { useState } from "react";
import { invoke, isTauri, delay } from "../../api";
import { Markdown } from "../../util/markdown";
import {
  CheckIcon,
  CopyIcon,
  DocIcon,
  DownloadIcon,
  GlobeIcon,
  ShieldIcon,
  SparkIcon,
} from "../ui/icons";
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

function downloadText(fileName: string, text: string, mime: string) {
  const blob = new Blob([text], { type: mime });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = fileName;
  anchor.click();
  URL.revokeObjectURL(url);
}

function slugify(value: string) {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 60) || "research";
}

export function ResearchPlanView({ project }: { project: Project }) {
  const [topic, setTopic] = useState("");
  const [plan, setPlan] = useState<ResearchPlan | null>(null);
  const [report, setReport] = useState<ResearchReportResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [executing, setExecuting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const doneCount = plan?.steps.filter((step) => step.completed).length ?? 0;

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
        setPlan({
          ...plan,
          status: "completed",
          steps: plan.steps.map((step) => ({ ...step, completed: true })),
        });
      } else {
        await delay(1200);
        setReport({
          topic: plan.topic,
          reportMarkdown: `# Research Report: ${plan.topic}\n\n## Executive Summary\nAnalysis generated from local sources.\n\n## Sources\n- project-notes.md`,
          savedToFile: "outputs/research-sample.md",
          sourcesUsed: ["project-notes.md"],
        });
        setPlan({
          ...plan,
          status: "completed",
          steps: plan.steps.map((step) => ({ ...step, completed: true })),
        });
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setExecuting(false);
    }
  };

  const toggleStep = (stepNumber: number) => {
    setPlan((previous) =>
      previous
        ? {
            ...previous,
            steps: previous.steps.map((step) =>
              step.stepNumber === stepNumber ? { ...step, completed: !step.completed } : step,
            ),
          }
        : previous,
    );
  };

  const renameStep = (stepNumber: number, title: string) => {
    setPlan((previous) =>
      previous
        ? {
            ...previous,
            steps: previous.steps.map((step) =>
              step.stepNumber === stepNumber ? { ...step, title } : step,
            ),
          }
        : previous,
    );
  };

  const flipSource = (stepNumber: number) => {
    setPlan((previous) =>
      previous
        ? {
            ...previous,
            steps: previous.steps.map((step) =>
              step.stepNumber === stepNumber
                ? { ...step, sourceType: step.sourceType === "local" ? "web" : "local" }
                : step,
            ),
          }
        : previous,
    );
  };

  const copyReport = async () => {
    if (!report) return;
    await navigator.clipboard.writeText(report.reportMarkdown);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1400);
  };

  return (
    <div className="workbench workbench--research">
      <div className="workbench__col">
        <div className="workbench__panel">
          <div className="workbench__title">
            <SparkIcon size={15} />
            Deep research
          </div>
          <p className="workbench__lede">
            Plan a multi-step investigation, approve it, then let the agent search,
            read and synthesize a cited report.
          </p>

          <div className="field-row">
            <label htmlFor="research-topic">Topic or question</label>
            <input
              id="research-topic"
              className="text-input"
              type="text"
              placeholder="What should the agent investigate?"
              value={topic}
              onChange={(event) => setTopic(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !loading && topic.trim()) {
                  void handleGeneratePlan();
                }
              }}
            />
          </div>

          <div className="scope-toggles">
            <span className="scope-toggle" aria-pressed={project.internetEnabled}>
              <GlobeIcon size={12} />
              Internet {project.internetEnabled ? "on" : "off"}
            </span>
            <span className="scope-toggle" aria-pressed={!project.internetEnabled}>
              <ShieldIcon size={12} />
              Local sources
            </span>
          </div>

          <div className="workbench__actions">
            <button
              type="button"
              className="btn btn--primary"
              onClick={() => void handleGeneratePlan()}
              disabled={loading || executing || !topic.trim()}
            >
              <SparkIcon size={14} />
              {loading ? "Planning…" : "Generate plan"}
            </button>
          </div>

          {error ? (
            <div className="doc-recovery">
              <div className="doc-recovery__title">Research failed</div>
              <p className="doc-recovery__cause">{error}</p>
            </div>
          ) : null}
        </div>

        {plan ? (
          <div className="workbench__panel">
            <div className="workbench__title">
              <CheckIcon size={15} />
              Plan · {doneCount}/{plan.steps.length} done
            </div>
            <p className="workbench__lede">
              Edit any step before approving. Untick a step to skip it.
            </p>
            <div className="plan-steps">
              {plan.steps.map((step) => (
                <div
                  key={step.stepNumber}
                  className={`plan-step${step.completed ? " plan-step--done" : ""}`}
                >
                  <input
                    type="checkbox"
                    className="plan-step__check"
                    checked={step.completed}
                    onChange={() => toggleStep(step.stepNumber)}
                    aria-label={`Mark step ${step.stepNumber} complete`}
                  />
                  <div className="plan-step__body">
                    <span className="plan-step__index">STEP {step.stepNumber}</span>
                    <input
                      className="plan-step__title"
                      value={step.title}
                      onChange={(event) => renameStep(step.stepNumber, event.target.value)}
                      aria-label={`Step ${step.stepNumber} title`}
                    />
                    <span className="plan-step__meta">
                      Query: <code>{step.query}</code>
                    </span>
                  </div>
                  <button
                    type="button"
                    className="scope-toggle"
                    aria-pressed={step.sourceType === "web"}
                    onClick={() => flipSource(step.stepNumber)}
                    title="Toggle between local index and web"
                  >
                    {step.sourceType === "web" ? <GlobeIcon size={11} /> : <ShieldIcon size={11} />}
                    {step.sourceType === "web" ? "Web" : "Local"}
                  </button>
                </div>
              ))}
            </div>

            <div className="workbench__actions">
              <button
                type="button"
                className="btn btn--primary"
                onClick={() => void handleApproveAndExecute()}
                disabled={executing}
              >
                <SparkIcon size={14} />
                {executing ? "Agent running…" : report ? "Run again" : "Approve & run agent"}
              </button>
              <button
                type="button"
                className="btn btn--ghost btn--sm"
                onClick={() => {
                  setPlan(null);
                  setReport(null);
                  setError(null);
                }}
                disabled={executing}
              >
                Clear
              </button>
            </div>
          </div>
        ) : null}
      </div>

      <div className="workbench__col">
        <div className="workbench__panel" style={{ flex: 1 }}>
          <div className="report-toolbar">
            <span className="workbench__title">
              {report ? "Final synthesis" : executing ? "Live synthesis" : "Synthesis"}
            </span>
            <span className="spacer" style={{ flex: 1 }} />
            <button
              type="button"
              className="btn btn--subtle btn--sm"
              onClick={() => void copyReport()}
              disabled={!report}
              title="Copy the report"
            >
              {copied ? <CheckIcon size={13} /> : <CopyIcon size={13} />}
              {copied ? "Copied" : "Copy"}
            </button>
            <button
              type="button"
              className="btn btn--subtle btn--sm"
              onClick={() =>
                report &&
                downloadText(`${slugify(report.topic)}.md`, report.reportMarkdown, "text/markdown")
              }
              disabled={!report}
              title="Download as Markdown"
            >
              <DownloadIcon size={13} />
              .md
            </button>
            <button
              type="button"
              className="btn btn--subtle btn--sm"
              onClick={() =>
                report &&
                downloadText(
                  `${slugify(report.topic)}.json`,
                  JSON.stringify(report, null, 2),
                  "application/json",
                )
              }
              disabled={!report}
              title="Download as JSON"
            >
              <DownloadIcon size={13} />
              .json
            </button>
          </div>

          {report?.savedToFile ? (
            <div className="pill pill--ok" style={{ alignSelf: "flex-start" }}>
              <CheckIcon size={11} />
              Saved to {report.savedToFile}
            </div>
          ) : null}

          <div className="workbench__output synthesis">
            {report ? (
              <>
                <Markdown text={report.reportMarkdown} />
                {report.sourcesUsed.length > 0 ? (
                  <div style={{ marginTop: "var(--space-4)" }}>
                    <div className="diary__aside-title" style={{ marginBottom: 6 }}>
                      Sources examined
                    </div>
                    <div className="cite-list">
                      {report.sourcesUsed.map((source) => (
                        <span key={source} className="pill">
                          <DocIcon size={11} />
                          {source}
                        </span>
                      ))}
                    </div>
                  </div>
                ) : null}
              </>
            ) : executing ? (
              <div className="synthesis__empty">
                <SparkIcon size={26} />
                <strong>Agent is working through the plan</strong>
                <span>
                  {plan?.steps.map((step) => step.title).join(" → ") || "Searching sources…"}
                </span>
              </div>
            ) : (
              <div className="synthesis__empty">
                <SparkIcon size={26} />
                <strong>The synthesized report lands here</strong>
                <span>
                  {plan
                    ? "Approve the plan on the left to start the agent."
                    : "Describe a topic, generate a plan, then approve it."}
                </span>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
