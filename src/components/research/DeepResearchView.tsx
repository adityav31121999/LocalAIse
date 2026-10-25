import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

interface ResearchEvent {
  type: string;
  [key: string]: unknown;
}

export function DeepResearchView() {
  const [question, setQuestion] = useState("");
  const [events, setEvents] = useState<ResearchEvent[]>([]);
  const [running, setRunning] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<ResearchEvent>("research:event", (e) => {
      setEvents((prev) => [...prev, e.payload]);
    }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  const run = async () => {
    if (!question.trim()) return;
    setEvents([]);
    setRunning(true);
    try {
      await invoke("run_deep_research", { question });
    } finally {
      setRunning(false);
    }
  };

  return (
    <div className="chat" style={{ padding: "var(--space-4)" }}>
      <h2 className="model-config__title">Deep research</h2>
      <div style={{ display: "flex", gap: "var(--space-2)", marginTop: "var(--space-3)" }}>
        <input
          className="input"
          style={{ flex: 1 }}
          value={question}
          onChange={(e) => setQuestion(e.target.value)}
          placeholder="Ask a research question…"
          aria-label="Research question"
        />
        <button type="button" className="btn btn--primary" disabled={running} onClick={() => void run()}>
          {running ? "Running…" : "Run"}
        </button>
      </div>
      <div style={{ marginTop: "var(--space-4)", display: "grid", gap: "var(--space-2)" }}>
        {events.map((ev, i) => (
          <div key={i} className="field__hint" style={{ fontFamily: "var(--font-mono)", fontSize: "var(--text-xs)" }}>
            <strong>{ev.type}</strong>{" "}
            {Object.entries(ev)
              .filter(([k]) => k !== "type")
              .map(([k, v]) => `${k}=${typeof v === "string" ? v.slice(0, 80) : JSON.stringify(v)}`)
              .join(" · ")}
          </div>
        ))}
      </div>
    </div>
  );
}