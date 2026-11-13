import { useEffect, useState } from "react";
import { useProjectStore, selectActiveProject } from "../../state/projectStore";

// Properly typed ResearchEvent discriminated union
type ResearchEvent =
  | { type: "Planned"; sub_questions: string[] }
  | { type: "Searching"; query: string }
  | { type: "Fetched"; url: string; tier: string; chars: number }
  | { type: "FactExtracted"; sub_q: number; source_id: number; claim: string }
  | { type: "GapFound"; missing: string[] }
  | { type: "SectionWritten"; title: string; markdown: string; confidence?: number; citation_count?: number }
  | { type: "SectionConfidence"; title: string; confidence: number; citation_count: number }
  | { type: "Verified"; supported: number; unsupported: number; stage1_passes?: number; stage2_supported?: number; stage2_contradicted?: number; stage2_unsupported?: number }
  | { type: "Notice"; message: string }
  | { type: "Done"; report_id: number; report_path?: string }
  | { type: "Error"; message: string };

interface SubQuestion {
  question: string;
  search_queries?: string[];
  answer_criteria?: string;
  approved?: boolean;
}

interface SectionInfo {
  title: string;
  markdown: string;
  confidence?: number;
  citation_count?: number;
}

interface BudgetPreset {
  id: string;
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
  const [phase, setPhase] = useState<"input" | "plan" | "running" | "done">("input");
  const [subQuestions, setSubQuestions] = useState<SubQuestion[]>([]);
  const [sections, setSections] = useState<SectionInfo[]>([]);
  const [selectedBudget, setSelectedBudget] = useState("standard");
  const [reportPath, setReportPath] = useState<string | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    import("@tauri-apps/api/event")
      .then(({ listen }) => listen<ResearchEvent>("research:event", (e) => {
        const event = e.payload;
        setEvents((prev) => [...prev, event]);
        
        // Handle specific event types
        switch (event.type) {
          case "Planned":
            if (event.sub_questions) {
              const sqs: SubQuestion[] = event.sub_questions.map((q: string) => ({ question: q, approved: true }));
              setSubQuestions(sqs);
              setPhase("plan");
            }
            break;
          case "SectionWritten":
            if (event.title && event.markdown) {
              setSections((prev) => {
                const existing = prev.findIndex(s => s.title === event.title);
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
          case "Done":
            if (event.report_path) {
              setReportPath(event.report_path);
            }
            setPhase("done");
            setRunning(false);
            break;
          case "Error":
            setPhase("input");
            setRunning(false);
            break;
        }
      }))
      .then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  const handleRun = async () => {
    if (!question.trim() || !project) return;
    setEvents([]);
    setSections([]);
    setReportPath(null);
    setRunning(true);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("run_deep_research", { 
        question, 
        mode: "Online",
        project_id: project.id,
        provided_urls: [],
        use_browser: true,
        budget_preset: selectedBudget,
      });
      setPhase("running");
    } catch (e) {
      setRunning(false);
      setPhase("input");
    }
  };

  const handleApprovePlan = () => {
    // Send approved sub-questions to backend (would need new command)
    setPhase("running");
    // For now just continue with the already started research
  };

  const handleModifyQuestion = () => {
    setPhase("input");
    setSubQuestions([]);
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
      <h2 className="model-config__title">Deep Research</h2>

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
            <button type="button" className="btn btn--primary" disabled={!question.trim() || running} onClick={() => void handleRun()}>
              Start Research
            </button>
          </div>

          <fieldset style={{ border: "1px solid var(--color-border)", borderRadius: "var(--radius-md)", padding: "var(--space-3)" }}>
            <legend style={{ fontWeight: 600, marginBottom: "var(--space-2)" }}>Research Budget</legend>
            <div style={{ display: "flex", gap: "var(--space-2)", flexWrap: "wrap" }}>
              {BUDGET_PRESETS.map(preset => (
                <label key={preset.id} style={{ 
                  flex: 1, 
                  minWidth: "200px",
                  padding: "var(--space-3)",
                  border: `2px solid ${selectedBudget === preset.id ? "var(--color-primary)" : "var(--color-border)"}`,
                  borderRadius: "var(--radius-md)",
                  cursor: "pointer",
                  background: selectedBudget === preset.id ? "var(--color-primary-bg)" : "transparent",
                  transition: "all 0.2s"
                }}>
                  <input
                    type="radio"
                    name="budget"
                    value={preset.id}
                    checked={selectedBudget === preset.id}
                    onChange={() => setSelectedBudget(preset.id)}
                    style={{ marginRight: "var(--space-2)" }}
                  />
                  <strong>{preset.label}</strong>
                  <div className="field__hint" style={{ marginTop: "var(--space-1)" }}>{preset.description}</div>
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
              <div key={i} style={{ 
                padding: "var(--space-3)", 
                border: "1px solid var(--color-border)", 
                borderRadius: "var(--radius-md)",
                background: sq.approved ? "var(--color-success-bg)" : "var(--color-warning-bg)"
              }}>
                <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--space-2)" }}>
                  <input
                    type="checkbox"
                    checked={sq.approved}
                    onChange={(e) => {
                      setSubQuestions(prev => prev.map((s, idx) => idx === i ? { ...s, approved: e.target.checked } : s));
                    }}
                    style={{ marginTop: "2px" }}
                  />
                  <div style={{ flex: 1 }}>
                    <strong>Sub-question {i + 1}:</strong> {sq.question}
                    {sq.search_queries && sq.search_queries.length > 0 && (
                      <div className="field__hint" style={{ marginTop: "var(--space-1)" }}>
                        Search queries: {sq.search_queries.join(", ")}
                      </div>
                    )}
                    {sq.answer_criteria && (
                      <div className="field__hint" style={{ marginTop: "var(--space-1)" }}>
                        Criteria: {sq.answer_criteria}
                      </div>
                    )}
                  </div>
                </div>
              </div>
            ))}
          </div>

          <div style={{ display: "flex", gap: "var(--space-2)", justifyContent: "flex-end" }}>
            <button type="button" className="btn btn--secondary" onClick={handleModifyQuestion}>
              Modify Question
            </button>
            <button type="button" className="btn btn--primary" onClick={handleApprovePlan} disabled={!subQuestions.some(s => s.approved)}>
              Approve & Run
            </button>
          </div>
        </div>
      )}

      {/* Phase 3: Running - Live Events + Sections */}
      {phase === "running" && (
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
          <div className="field__hint" style={{ color: "var(--color-primary)" }}>
            Research in progress… {events.filter(e => e.type === "Fetched").length} sources fetched, 
            {events.filter(e => e.type === "FactExtracted").length} facts extracted
          </div>

          {/* Sections with confidence */}
          {sections.length > 0 && (
            <details style={{ border: "1px solid var(--color-border)", borderRadius: "var(--radius-md)" }}>
              <summary style={{ padding: "var(--space-3)", fontWeight: 600, cursor: "pointer" }}>
                Sections ({sections.length}) - Click to expand
              </summary>
              <div style={{ padding: "var(--space-3)", display: "grid", gap: "var(--space-3)" }}>
                {sections.map((section, i) => (
                  <div key={i} style={{ 
                    padding: "var(--space-3)", 
                    border: "1px solid var(--color-border)", 
                    borderRadius: "var(--radius-md)",
                    background: "var(--color-surface)"
                  }}>
                    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "var(--space-2)" }}>
                      <strong>{section.title}</strong>
                      <div style={{ display: "flex", gap: "var(--space-3)", fontSize: "var(--text-sm)" }}>
                        {section.confidence !== undefined && (
                          <span style={{ 
                            padding: "2px 8px", 
                            borderRadius: "var(--radius-sm)",
                            background: section.confidence > 0.7 ? "var(--color-success-bg)" : section.confidence > 0.4 ? "var(--color-warning-bg)" : "var(--color-error-bg)",
                            color: section.confidence > 0.7 ? "var(--color-success)" : section.confidence > 0.4 ? "var(--color-warning)" : "var(--color-error)"
                          }}>
                            Confidence: {Math.round(section.confidence * 100)}%
                          </span>
                        )}
                        {section.citation_count !== undefined && (
                          <span className="field__hint">📎 {section.citation_count} citations</span>
                        )}
                      </div>
                    </div>
                    <div 
                      style={{ 
                        fontSize: "var(--text-sm)", 
                        whiteSpace: "pre-wrap", 
                        maxHeight: "200px", 
                        overflow: "auto",
                        fontFamily: "var(--font-mono)"
                      }}
                    >
                      {section.markdown.slice(0, 500)}{section.markdown.length > 500 ? "..." : ""}
                    </div>
                  </div>
                ))}
              </div>
            </details>
          )}

          {/* Live event log */}
          <details>
            <summary style={{ padding: "var(--space-2)", fontWeight: 600, cursor: "pointer" }}>
              Event Log ({events.length})
            </summary>
            <div style={{ maxHeight: "300px", overflow: "auto", padding: "var(--space-2)", fontFamily: "var(--font-mono)", fontSize: "var(--text-xs)" }}>
              {events.map((ev, i) => (
                <div key={i} style={{ padding: "2px 0", borderBottom: "1px solid var(--color-border)" }}>
                  <strong style={{ color: "var(--color-primary)" }}>{ev.type}</strong>{" "}
                  {Object.entries(ev)
                    .filter(([k]) => k !== "type")
                    .map(([k, v]) => `${k}=${typeof v === "string" ? v.slice(0, 100) : JSON.stringify(v).slice(0, 100)}`)
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
            Research complete! {sections.length} sections, {events.filter(e => e.type === "Fetched").length} sources.
          </div>

          {reportPath && (
            <div style={{ padding: "var(--space-3)", background: "var(--color-success-bg)", borderRadius: "var(--radius-md)", border: "1px solid var(--color-success)" }}>
              <strong>Report saved to:</strong> {reportPath}
            </div>
          )}

          <details open>
            <summary style={{ padding: "var(--space-3)", fontWeight: 600, cursor: "pointer" }}>
              Report Sections ({sections.length})
            </summary>
            <div style={{ padding: "var(--space-3)", display: "grid", gap: "var(--space-4)" }}>
              {sections.map((section, i) => (
                <div key={i} style={{ 
                  padding: "var(--space-3)", 
                  border: "1px solid var(--color-border)", 
                  borderRadius: "var(--radius-md)",
                  background: "var(--color-surface)"
                }}>
                  <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "var(--space-2)" }}>
                    <strong>{section.title}</strong>
                    <div style={{ display: "flex", gap: "var(--space-3)", fontSize: "var(--text-sm)" }}>
                      {section.confidence !== undefined && (
                        <span style={{ 
                          padding: "2px 8px", 
                          borderRadius: "var(--radius-sm)",
                          background: section.confidence > 0.7 ? "var(--color-success-bg)" : section.confidence > 0.4 ? "var(--color-warning-bg)" : "var(--color-error-bg)",
                          color: section.confidence > 0.7 ? "var(--color-success)" : section.confidence > 0.4 ? "var(--color-warning)" : "var(--color-error)"
                        }}>
                          Confidence: {Math.round(section.confidence * 100)}%
                        </span>
                      )}
                      {section.citation_count !== undefined && (
                        <span className="field__hint">📎 {section.citation_count} citations</span>
                      )}
                    </div>
                  </div>
                  <div 
                    style={{ 
                      fontSize: "var(--text-sm)", 
                      whiteSpace: "pre-wrap", 
                      fontFamily: "var(--font-mono)",
                      lineHeight: 1.6
                    }}
                    dangerouslySetInnerHTML={{ __html: section.markdown }}
                  />
                </div>
              ))}
            </div>
          </details>

          <button type="button" className="btn btn--secondary" onClick={() => { setPhase("input"); setQuestion(""); setSections([]); setEvents([]); }}>
            New Research
          </button>
        </div>
      )}
    </div>
  );
}