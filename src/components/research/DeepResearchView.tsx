import { useEffect, useState } from "react";
import { useProjectStore, selectActiveProject } from "../../state/projectStore";
import {
  type CitationStyle,
  type DeepResearchPlan,
  type DeepResearchPlanStep,
  type ExportFormat,
  type ResearchEvent,
  type ResearchMode,
  planDeepResearch,
  runDeepResearch,
  stopDeepResearch,
  onResearchEvent,
  exportResearchReport,
  revealPath,
} from "../../api/research";
import { Markdown } from "../../util/markdown";

interface SubQuestion {
  question: string;
  searchQueries?: string[];
  answerCriteria?: string;
  source?: "local" | "internet";
  approved?: boolean;
}

interface SectionInfo {
  title: string;
  markdown: string;
  confidence?: number;
  citation_count?: number;
}

interface BudgetPreset {
  id: "quick" | "standard" | "deep";
  label: string;
  description: string;
  max_rounds: number;
  max_pages: number;
  max_cost_units: number;
  wall_clock_secs: number;
}

const BUDGET_PRESETS: BudgetPreset[] = [
  {
    id: "quick",
    label: "Quick",
    description: "~1 min, 2 rounds, 8 pages",
    max_rounds: 2,
    max_pages: 8,
    max_cost_units: 50,
    wall_clock_secs: 60,
  },
  {
    id: "standard",
    label: "Standard",
    description: "~3 min, 4 rounds, 20 pages",
    max_rounds: 4,
    max_pages: 20,
    max_cost_units: 200,
    wall_clock_secs: 180,
  },
  {
    id: "deep",
    label: "Deep",
    description: "~10 min, 6 rounds, 50 pages",
    max_rounds: 6,
    max_pages: 50,
    max_cost_units: 1000,
    wall_clock_secs: 600,
  },
];

export function DeepResearchView() {
  const project = useProjectStore(selectActiveProject);
  const [question, setQuestion] = useState("");
  const [events, setEvents] = useState<ResearchEvent[]>([]);
  const [running, setRunning] = useState(false);
  const [planning, setPlanning] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [phase, setPhase] = useState<"input" | "plan" | "running" | "done">("input");
  const [subQuestions, setSubQuestions] = useState<SubQuestion[]>([]);
  const [planRecord, setPlanRecord] = useState<DeepResearchPlan | null>(null);
  const [sections, setSections] = useState<SectionInfo[]>([]);
  const [selectedBudget, setSelectedBudget] = useState<"quick" | "standard" | "deep">("standard");
  const [mode, setMode] = useState<ResearchMode>("online");
  const [reportId, setReportId] = useState<number | null>(null);
  const [reportPath, setReportPath] = useState<string | null>(null);
  const [exportFormat, setExportFormat] = useState<ExportFormat>("markdown");
  const [citationStyle, setCitationStyle] = useState<CitationStyle>("numeric");
  const [exporting, setExporting] = useState(false);
  const [exportedTo, setExportedTo] = useState<string | null>(null);
  const [verified, setVerified] = useState<Extract<ResearchEvent, { type: "Verified" }> | null>(null);
  const [error, setError] = useState<string | null>(null);

  const internetAllowed = project?.internetEnabled ?? true;
  // A project with the internet toggle off can only ever run offline.
  const effectiveMode: ResearchMode = internetAllowed ? mode : "offline";

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    onResearchEvent((event) => {
      // Cap the log so very long (Deep) runs don't grow memory without bound.
      setEvents((prev) => (prev.length >= 300 ? [...prev.slice(-299), event] : [...prev, event]));

      switch (event.type) {
        case "Planned":
          if (event.sub_questions) {
            const sqs: SubQuestion[] = event.sub_questions.map((q: string) => ({
              question: q,
              approved: true,
            }));
            setSubQuestions(sqs);
            setPhase("plan");
          }
          break;        case "SectionWritten":
          if (event.title && event.markdown) {
            setSections((prev) => {
              const existing = prev.findIndex((s) => s.title === event.title);
              const newSection: SectionInfo = {
                title: event.title,
                markdown: event.markdown,
                confidence: event.confidence,
                citation_count: event.citation_count,
              };
              if (existing >= 0) {
                const updated = [...prev];
                updated[existing] = newSection;
                return updated;
              }
              return [...prev, newSection];
            });
          }
          break;
        case "Verified":
          setVerified(event);
          break;
        case "Done":
          if (event.report_path) {
            setReportPath(event.report_path);
          }
          setReportId(event.report_id);
          setError(null);
          setPhase("done");
          setRunning(false);
          setStopping(false);
          break;
        case "Error":
          setError(event.message);
          setPhase("input");
          setRunning(false);
          setStopping(false);
          break;
        case "Notice":
          if (event.message?.toLowerCase().includes("cancelled")) {
            setStopping(true);
          }
          break;
      }
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
  }, []);

  // Phase 1 → 2: ask the model for a plan, show it for review. Nothing is
  // gathered until the user approves.
  const handleStart = async () => {
    if (!question.trim() || !project || planning) return;
    setError(null);
    setPlanning(true);
    try {
      const plan = await planDeepResearch({
        question,
        mode: effectiveMode,
        projectId: project.id,
      });
      setPlanRecord(plan);
      setSubQuestions(
        plan.steps.map((step) => ({
          question: step.question,
          searchQueries: step.searchQueries,
          answerCriteria: step.answerCriteria,
          source: step.source,
          approved: true,
        }))
      );
      setPhase("plan");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setPlanning(false);
    }
  };

  // Phase 2 → 3: run the approved sub-questions. Unchecked ones are dropped
  // before the plan is sent back to the backend.
  const handleApprovePlan = async () => {
    if (!project || !planRecord) return;
    const approved = subQuestions
      .filter((sq) => sq.approved)
      .map((sq, i) => ({
        id: i + 1,
        question: sq.question,
        source: sq.source ?? "internet",
        searchQueries: sq.searchQueries ?? [sq.question],
        answerCriteria: sq.answerCriteria ?? "Any relevant information",
      })) as DeepResearchPlanStep[];

    if (approved.length === 0) return;

    setEvents([]);
    setSections([]);
    setReportPath(null);
    setReportId(null);
    setVerified(null);
    setExportedTo(null);
    setError(null);
    setRunning(true);
    setStopping(false);
    setPhase("running");
    try {
      await runDeepResearch({
        question,
        mode: effectiveMode,
        projectId: project.id,
        providedUrls: [],
        useBrowser: effectiveMode === "online",
        budgetPreset: selectedBudget,
        plan: { ...planRecord, steps: approved },
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setRunning(false);
      setStopping(false);
      setPhase("input");
    }
  };

  const handleStop = async () => {
    if (!project || !running || stopping) return;
    setStopping(true);
    try {
      await stopDeepResearch(project.id);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setStopping(false);
    }
  };

  const handleExport = async () => {
    if (!project || reportId === null || exporting) return;
    setExporting(true);
    setError(null);
    setExportedTo(null);
    try {
      const path = await exportResearchReport({
        projectId: project.id,
        reportId,
        format: exportFormat,
        citationStyle,
      });
      setExportedTo(path);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setExporting(false);
    }
  };

  const handleOpenReport = async (path: string) => {
    try {
      await revealPath(path);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleModifyQuestion = () => {
    setPhase("input");
    setSubQuestions([]);
    setPlanRecord(null);
    setError(null);
  };

  if (!project) {
    return (
      <div className="chat" style={{ padding: "var(--space-4)" }}>
        <h2 className="model-config__title">Deep Research</h2>
        <div className="field__hint" style={{ color: "var(--color-warning)" }}>
          Please open a project first to use deep research.
        </div>
      </div>
    );
  }

  return (
    <div className="chat" style={{ padding: "var(--space-4)", maxWidth: "900px" }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <h2 className="model-config__title">Deep Research</h2>
        {running && (
          <button
            type="button"
            className="btn btn--danger"
            disabled={stopping}
            onClick={() => void handleStop()}
          >
            {stopping ? "Stopping..." : "Stop Research"}
          </button>
        )}
      </div>

      {error && (
        <div
          className="field__hint"
          role="alert"
          style={{
            color: "var(--color-error)",
            border: "1px solid var(--color-error)",
            borderRadius: "var(--radius-md)",
            padding: "var(--space-2)",
          }}
        >
          {error}
        </div>
      )}

      {/* Phase 1: Question Input + Budget Selection */}
      {phase === "input" && (
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
          <div style={{ display: "flex", gap: "var(--space-2)", marginTop: "var(--space-3)" }}>
            <input
              className="input"
              style={{ flex: 1 }}
              value={question}
              onChange={(e) => setQuestion(e.target.value)}
              placeholder="Ask a research question…"
              aria-label="Research question"
            />
            <button
              type="button"
              className="btn btn--primary"
              disabled={!question.trim() || running || planning}
              onClick={() => void handleStart()}
            >
              {planning ? "Planning…" : "Start Research"}
            </button>
          </div>

          <fieldset
            style={{
              border: "1px solid var(--color-border)",
              borderRadius: "var(--radius-md)",
              padding: "var(--space-3)",
            }}
          >
            <legend style={{ fontWeight: 600, marginBottom: "var(--space-2)" }}>
              Research Mode
            </legend>
            <div style={{ display: "flex", gap: "var(--space-2)", alignItems: "center", flexWrap: "wrap" }}>
              {(["online", "offline"] as const).map((value) => {
                const disabled = value === "online" && !internetAllowed;
                return (
                  <label
                    key={value}
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: "var(--space-2)",
                      padding: "var(--space-2) var(--space-3)",
                      border: `2px solid ${
                        effectiveMode === value ? "var(--color-primary)" : "var(--color-border)"
                      }`,
                      borderRadius: "var(--radius-md)",
                      cursor: disabled ? "not-allowed" : "pointer",
                      opacity: disabled ? 0.5 : 1,
                      background:
                        effectiveMode === value ? "var(--color-primary-bg)" : "transparent",
                    }}
                    title={
                      disabled
                        ? "Internet access is disabled for this project"
                        : undefined
                    }
                  >
                    <input
                      type="radio"
                      name="mode"
                      value={value}
                      checked={effectiveMode === value}
                      disabled={disabled}
                      onChange={() => setMode(value)}
                    />
                    <span>
                      <strong>{value === "online" ? "Online" : "Offline"}</strong>
                      <span className="field__hint" style={{ display: "block" }}>
                        {value === "online"
                          ? "Search the web + project documents"
                          : "Project documents only"}
                      </span>
                    </span>
                  </label>
                );
              })}
            </div>
            {!internetAllowed && (
              <div className="field__hint" style={{ marginTop: "var(--space-2)", color: "var(--color-warning)" }}>
                Internet access is disabled for this project, so research runs offline against the project documents.
              </div>
            )}
          </fieldset>

          <fieldset
            style={{
              border: "1px solid var(--color-border)",
              borderRadius: "var(--radius-md)",
              padding: "var(--space-3)",
            }}
          >
            <legend style={{ fontWeight: 600, marginBottom: "var(--space-2)" }}>
              Research Budget
            </legend>
            <div style={{ display: "flex", gap: "var(--space-2)", flexWrap: "wrap" }}>
              {BUDGET_PRESETS.map((preset) => (
                <label
                  key={preset.id}
                  style={{
                    flex: 1,
                    minWidth: "200px",
                    padding: "var(--space-3)",
                    border: `2px solid ${
                      selectedBudget === preset.id ? "var(--color-primary)" : "var(--color-border)"
                    }`,
                    borderRadius: "var(--radius-md)",
                    cursor: "pointer",
                    background:
                      selectedBudget === preset.id ? "var(--color-primary-bg)" : "transparent",
                    transition: "all 0.2s",
                  }}
                >
                  <input
                    type="radio"
                    name="budget"
                    value={preset.id}
                    checked={selectedBudget === preset.id}
                    onChange={() => setSelectedBudget(preset.id)}
                    style={{ marginRight: "var(--space-2)" }}
                  />
                  <strong>{preset.label}</strong>
                  <div className="field__hint" style={{ marginTop: "var(--space-1)" }}>
                    {preset.description}
                  </div>
                </label>
              ))}
            </div>
          </fieldset>
        </div>
      )}

      {/* Phase 2: Plan Review & Approval */}
      {phase === "plan" && (
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
          <div className="field__hint" style={{ color: "var(--color-primary)" }}>
            Research plan generated. Review and approve sub-questions before proceeding.
          </div>

          <div style={{ display: "grid", gap: "var(--space-3)" }}>
            {subQuestions.map((sq, i) => (
              <div
                key={i}
                style={{
                  padding: "var(--space-3)",
                  border: "1px solid var(--color-border)",
                  borderRadius: "var(--radius-md)",
                  background: sq.approved ? "var(--color-success-bg)" : "var(--color-warning-bg)",
                }}
              >
                <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--space-2)" }}>
                  <input
                    type="checkbox"
                    checked={sq.approved}
                    onChange={(e) => {
                      setSubQuestions((prev) =>
                        prev.map((s, idx) =>
                          idx === i ? { ...s, approved: e.target.checked } : s
                        )
                      );
                    }}
                    style={{ marginTop: "2px" }}
                  />
                  <div style={{ flex: 1 }}>
                    <div style={{ display: "flex", gap: "var(--space-2)", alignItems: "baseline", flexWrap: "wrap" }}>
                      <strong>Sub-question {i + 1}:</strong> {sq.question}
                      {sq.source && (
                        <span
                          style={{
                            fontSize: "var(--text-xs)",
                            padding: "1px 6px",
                            borderRadius: "var(--radius-sm)",
                            background: "var(--color-surface)",
                            border: "1px solid var(--color-border)",
                          }}
                        >
                          {sq.source}
                        </span>
                      )}
                    </div>
                    {sq.searchQueries && sq.searchQueries.length > 0 && (
                      <div className="field__hint" style={{ marginTop: "var(--space-1)" }}>
                        Search queries: {sq.searchQueries.join(", ")}
                      </div>
                    )}
                    {sq.answerCriteria && (
                      <div className="field__hint" style={{ marginTop: "var(--space-1)" }}>
                        Criteria: {sq.answerCriteria}
                      </div>
                    )}
                  </div>
                </div>
              </div>
            ))}
          </div>

          <div style={{ display: "flex", gap: "var(--space-2)", justifyContent: "flex-end" }}>
            <button
              type="button"
              className="btn btn--secondary"
              onClick={handleModifyQuestion}
            >
              Modify Question
            </button>
            <button
              type="button"
              className="btn btn--primary"
              onClick={handleApprovePlan}
              disabled={!subQuestions.some((s) => s.approved)}
            >
              Approve & Run
            </button>
          </div>
        </div>
      )}

      {/* Phase 3: Running - Live Events + Sections */}
      {phase === "running" && (
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
          <div
            className="field__hint"
            style={{ color: stopping ? "var(--color-warning)" : "var(--color-primary)" }}
          >
            {stopping
              ? "Stopping research pipeline… generating partial report from gathered evidence."
              : `Research in progress… ${
                  events.filter((e) => e.type === "Fetched").length
                } sources fetched, ${
                  events.filter((e) => e.type === "FactExtracted").length
                } facts extracted`}
          </div>

          {/* Verification + gap status */}
          {(() => {
            const gapsFound = events
              .filter((e): e is Extract<ResearchEvent, { type: "GapFound" }> => e.type === "GapFound")
              .reduce((n, e) => n + e.missing.length, 0);
            if (!verified && gapsFound === 0) return null;
            return (
              <div style={{ display: "flex", gap: "var(--space-2)", flexWrap: "wrap" }}>
                {gapsFound > 0 && (
                  <span
                    style={{
                      padding: "2px 10px",
                      borderRadius: "var(--radius-sm)",
                      border: "1px solid var(--color-border)",
                      fontSize: "var(--text-sm)",
                    }}
                  >
                    🔎 {gapsFound} follow-up gap(s) identified
                  </span>
                )}
                {verified && (
                  <span
                    style={{
                      padding: "2px 10px",
                      borderRadius: "var(--radius-sm)",
                      border: "1px solid var(--color-border)",
                      fontSize: "var(--text-sm)",
                    }}
                    title={
                      verified.stage2_supported !== undefined
                        ? `Stage 1 pass: ${verified.stage1_passes ?? "?"} · Stage 2: ${
                            verified.stage2_supported ?? 0
                          } supported / ${verified.stage2_contradicted ?? 0} contradicted / ${
                            verified.stage2_unsupported ?? 0
                          } unsupported / ${verified.stage2_failed ?? 0} failed`
                        : undefined
                    }
                  >
                    ✅ {verified.supported} verified, {verified.unsupported} unsupported
                    {verified.stage2_failed ? ` (${verified.stage2_failed} stage-2 failed)` : ""}
                  </span>
                )}
              </div>
            );
          })()}

          {/* Sections with confidence */}
          {sections.length > 0 && (
            <details
              style={{ border: "1px solid var(--color-border)", borderRadius: "var(--radius-md)" }}
            >
              <summary
                style={{ padding: "var(--space-3)", fontWeight: 600, cursor: "pointer" }}
              >
                Sections ({sections.length}) - Click to expand
              </summary>
              <div
                style={{ padding: "var(--space-3)", display: "grid", gap: "var(--space-3)" }}
              >
                {sections.map((section, i) => (
                  <div
                    key={i}
                    style={{
                      padding: "var(--space-3)",
                      border: "1px solid var(--color-border)",
                      borderRadius: "var(--radius-md)",
                      background: "var(--color-surface)",
                    }}
                  >
                    <div
                      style={{
                        display: "flex",
                        justifyContent: "space-between",
                        alignItems: "center",
                        marginBottom: "var(--space-2)",
                      }}
                    >
                      <strong>{section.title}</strong>
                      <div
                        style={{
                          display: "flex",
                          gap: "var(--space-3)",
                          fontSize: "var(--text-sm)",
                        }}
                      >
                        {section.confidence !== undefined && (
                          <span
                            style={{
                              padding: "2px 8px",
                              borderRadius: "var(--radius-sm)",
                              background:
                                section.confidence > 0.7
                                  ? "var(--color-success-bg)"
                                  : section.confidence > 0.4
                                  ? "var(--color-warning-bg)"
                                  : "var(--color-error-bg)",
                              color:
                                section.confidence > 0.7
                                  ? "var(--color-success)"
                                  : section.confidence > 0.4
                                  ? "var(--color-warning)"
                                  : "var(--color-error)",
                            }}
                          >
                            Confidence: {Math.round(section.confidence * 100)}%
                          </span>
                        )}
                        {section.citation_count !== undefined && (
                          <span className="field__hint">
                            📎 {section.citation_count} citations
                          </span>
                        )}
                      </div>
                    </div>
                    <div
                      style={{
                        fontSize: "var(--text-sm)",
                        maxHeight: "220px",
                        overflow: "auto",
                      }}
                    >
                      <Markdown text={section.markdown} />
                    </div>
                  </div>
                ))}
              </div>
            </details>
          )}

          {/* Live event log */}
          <details>
            <summary
              style={{ padding: "var(--space-2)", fontWeight: 600, cursor: "pointer" }}
            >
              Event Log ({events.length})
            </summary>
            <div
              style={{
                maxHeight: "300px",
                overflow: "auto",
                padding: "var(--space-2)",
                fontFamily: "var(--font-mono)",
                fontSize: "var(--text-xs)",
              }}
            >
              {events.map((ev, i) => (
                <div
                  key={i}
                  style={{ padding: "2px 0", borderBottom: "1px solid var(--color-border)" }}
                >
                  <strong style={{ color: "var(--color-primary)" }}>{ev.type}</strong>{" "}
                  {Object.entries(ev)
                    .filter(([k]) => k !== "type")
                    .map(
                      ([k, v]) =>
                        `${k}=${
                          typeof v === "string" ? v.slice(0, 100) : JSON.stringify(v).slice(0, 100)
                        }`
                    )
                    .join(" · ")}
                </div>
              ))}
            </div>
          </details>
        </div>
      )}

      {/* Phase 4: Done - Report View */}
      {phase === "done" && (
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
          <div className="field__hint" style={{ color: "var(--color-success)" }}>
            Research complete! {sections.length} sections,{" "}
            {events.filter((e) => e.type === "Fetched").length} sources.
          </div>

          {verified && (
            <div className="field__hint">
              Verification: {verified.supported} supported, {verified.unsupported} unsupported
              {verified.stage2_failed ? `, ${verified.stage2_failed} stage-2 check failed (kept via stage 1)` : ""}
            </div>
          )}

          {reportPath && (
            <div
              style={{
                padding: "var(--space-3)",
                background: "var(--color-success-bg)",
                borderRadius: "var(--radius-md)",
                border: "1px solid var(--color-success)",
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                gap: "var(--space-3)",
                flexWrap: "wrap",
              }}
            >
              <div>
                <strong>Report saved to:</strong> {reportPath}
              </div>
              <button
                type="button"
                className="btn btn--secondary"
                onClick={() => void handleOpenReport(reportPath)}
              >
                Open
              </button>
            </div>
          )}

          {/* Export */}
          <div
            style={{
              padding: "var(--space-3)",
              border: "1px solid var(--color-border)",
              borderRadius: "var(--radius-md)",
              display: "flex",
              flexDirection: "column",
              gap: "var(--space-2)",
            }}
          >
            <strong>Export report</strong>
            <div style={{ display: "flex", gap: "var(--space-2)", flexWrap: "wrap", alignItems: "center" }}>
              <label className="field__hint" style={{ display: "flex", alignItems: "center", gap: "var(--space-1)" }}>
                Format
                <select
                  className="input"
                  value={exportFormat}
                  onChange={(e) => setExportFormat(e.target.value as ExportFormat)}
                  aria-label="Export format"
                >
                  <option value="markdown">Markdown</option>
                  <option value="json">JSON</option>
                  <option value="html">HTML</option>
                  <option value="notion">Notion blocks (JSON)</option>
                  <option value="text">Plain text</option>
                  <option value="csljson">CSL-JSON (Zotero)</option>
                </select>
              </label>
              <label className="field__hint" style={{ display: "flex", alignItems: "center", gap: "var(--space-1)" }}>
                Citations
                <select
                  className="input"
                  value={citationStyle}
                  onChange={(e) => setCitationStyle(e.target.value as CitationStyle)}
                  aria-label="Citation style"
                >
                  <option value="numeric">Numeric</option>
                  <option value="apa">APA 7</option>
                  <option value="ieee">IEEE</option>
                  <option value="chicago">Chicago</option>
                  <option value="mla">MLA 9</option>
                  <option value="harvard">Harvard</option>
                </select>
              </label>
              <button
                type="button"
                className="btn btn--primary"
                disabled={exporting || reportId === null}
                onClick={() => void handleExport()}
              >
                {exporting ? "Exporting…" : "Export"}
              </button>
            </div>
            {exportedTo && (
              <div className="field__hint" style={{ display: "flex", alignItems: "center", gap: "var(--space-2)" }}>
                <span>Exported to: {exportedTo}</span>
                <button
                  type="button"
                  className="btn btn--secondary"
                  onClick={() => void handleOpenReport(exportedTo)}
                >
                  Open
                </button>
              </div>
            )}
          </div>

          <details open>
            <summary
              style={{ padding: "var(--space-3)", fontWeight: 600, cursor: "pointer" }}
            >
              Report Sections ({sections.length})
            </summary>
            <div
              style={{ padding: "var(--space-3)", display: "grid", gap: "var(--space-4)" }}
            >
              {sections.map((section, i) => (
                <div
                  key={i}
                  style={{
                    padding: "var(--space-3)",
                    border: "1px solid var(--color-border)",
                    borderRadius: "var(--radius-md)",
                    background: "var(--color-surface)",
                  }}
                >
                  <div
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "center",
                      marginBottom: "var(--space-2)",
                    }}
                  >
                    <strong>{section.title}</strong>
                    <div
                      style={{
                        display: "flex",
                        gap: "var(--space-3)",
                        fontSize: "var(--text-sm)",
                      }}
                    >
                      {section.confidence !== undefined && (
                        <span
                          style={{
                            padding: "2px 8px",
                            borderRadius: "var(--radius-sm)",
                            background:
                              section.confidence > 0.7
                                ? "var(--color-success-bg)"
                                : section.confidence > 0.4
                                ? "var(--color-warning-bg)"
                                : "var(--color-error-bg)",
                            color:
                              section.confidence > 0.7
                                ? "var(--color-success)"
                                : section.confidence > 0.4
                                ? "var(--color-warning)"
                                : "var(--color-error)",
                          }}
                        >
                          Confidence: {Math.round(section.confidence * 100)}%
                        </span>
                      )}
                      {section.citation_count !== undefined && (
                        <span className="field__hint">
                          📎 {section.citation_count} citations
                        </span>
                      )}
                    </div>
                  </div>
                  <div
                    style={{
                      fontSize: "var(--text-sm)",
                      lineHeight: 1.6,
                    }}
                  >
                    <Markdown text={section.markdown} />
                  </div>
                </div>
              ))}
            </div>
          </details>

          <button
            type="button"
            className="btn btn--secondary"
            onClick={() => {
              setPhase("input");
              setQuestion("");
              setSections([]);
              setEvents([]);
              setSubQuestions([]);
              setPlanRecord(null);
              setError(null);
              setStopping(false);
              setReportPath(null);
              setReportId(null);
              setVerified(null);
              setExportedTo(null);
            }}
          >
            New Research
          </button>
        </div>
      )}
    </div>
  );
}
