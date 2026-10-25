import { useEffect, useRef, useState } from "react";
import { ArrowUpIcon, StopIcon } from "../ui/icons";
import { NetworkIndicator } from "../layout/NetworkIndicator";
import { ModeIndicator } from "../layout/ModeIndicator";
import { useProjectStore } from "../../state/projectStore";
import type { Project } from "../../types";

interface ComposerProps {
  project: Project;
  streaming: boolean;
  onSend(text: string): void;
  onStop(): void;
}

export function Composer({ project, streaming, onSend, onStop }: ComposerProps) {
  const [value, setValue] = useState("");
  const [promptOpen, setPromptOpen] = useState(false);
  const [promptDraft, setPromptDraft] = useState("");
  const [samplingOpen, setSamplingOpen] = useState(false);
  const [temperature, setTemperature] = useState(0.7);
  const [topP, setTopP] = useState(0.9);
  const [topK, setTopK] = useState(40);
  // Display strings for the sampling fields, so digits can be deleted and
  // retyped without the stored value snapping the text back.
  const [temperatureDisplay, setTemperatureDisplay] = useState("0.7");
  const [topPDisplay, setTopPDisplay] = useState("0.9");
  const [topKDisplay, setTopKDisplay] = useState("40");
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const promptRef = useRef<HTMLTextAreaElement>(null);
  const setSystemPrompt = useProjectStore((state) => state.setSystemPrompt);
  const setInternet = useProjectStore((state) => state.setInternet);
  const setChatMode = useProjectStore((state) => state.setChatMode);

  useEffect(() => {
    const element = textareaRef.current;
    if (!element) return;
    element.style.height = "auto";
    element.style.height = `${Math.min(element.scrollHeight, 180)}px`;
  }, [value]);

  useEffect(() => {
    if (promptOpen) promptRef.current?.focus();
  }, [promptOpen]);

  const canSend = value.trim().length > 0 && !streaming;

  const submit = () => {
    if (!canSend) return;
    onSend(value);
    setValue("");
  };

  const openPromptEditor = () => {
    setPromptDraft(project.systemPrompt ?? "");
    setPromptOpen(true);
  };

  const savePrompt = () => {
    void setSystemPrompt(promptDraft.trim() || null);
    setPromptOpen(false);
  };

  const resetPrompt = () => {
    void setSystemPrompt(null);
    setPromptDraft("");
    setPromptOpen(false);
  };

  return (
    <div className="composer">
      <div className="composer__inner">
        {promptOpen && (
          <div className="composer__prompt">
            <div className="composer__prompt-head">
              <span className="composer__prompt-title">System prompt · {project.name}</span>
              <span className="composer__prompt-state">
                {project.systemPrompt ? "custom" : "default"}
              </span>
            </div>
            <textarea
              ref={promptRef}
              rows={4}
              className="composer__prompt-textarea"
              value={promptDraft}
              onChange={(event) => setPromptDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
                  event.preventDefault();
                  savePrompt();
                }
              }}
              placeholder="Tell the model how to behave. Left empty, the built-in project prompt is used."
              aria-label={`System prompt for ${project.name}`}
            />
            <div className="composer__prompt-foot">
              <span className="composer__prompt-hint">
                Saved with this project · Ctrl+Enter to save
              </span>
              <button
                type="button"
                className="composer__prompt-reset"
                onClick={resetPrompt}
                disabled={!project.systemPrompt}
              >
                Use default
              </button>
              <button type="button" className="composer__prompt-save" onClick={savePrompt}>
                Save prompt
              </button>
            </div>
          </div>
        )}

        <div className="composer__box">
          <textarea
            ref={textareaRef}
            rows={1}
            value={value}
            onChange={(event) => setValue(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                submit();
              }
            }}
            placeholder={
              project.chatMode === "agent"
                ? `Agent mode — ask, or have it read, write and search ${project.name}…`
                : `Ask about the sources in ${project.name}…`
            }
            aria-label="Ask a question about this project"
          />

          {streaming ? (
            <button
              type="button"
              className="composer__send composer__send--stop"
              onClick={onStop}
              title="Stop generating"
            >
              <StopIcon size={15} />
              <span className="sr-only">Stop generating</span>
            </button>
          ) : (
            <button
              type="button"
              className="composer__send"
              onClick={submit}
              disabled={!canSend}
              title="Send (Enter)"
            >
              <ArrowUpIcon size={15} />
              <span className="sr-only">Send</span>
            </button>
          )}
        </div>

        <div className="composer__foot">
          <button
            type="button"
            className={`composer__prompt-btn${project.systemPrompt ? " composer__prompt-btn--active" : ""}`}
            aria-expanded={promptOpen}
            onClick={promptOpen ? () => setPromptOpen(false) : openPromptEditor}
            title={
              project.systemPrompt
                ? "Edit this project's system prompt"
                : "Set a system prompt for this project"
            }
          >
            System prompt{project.systemPrompt ? " · custom" : ""}
          </button>
          <span className="mono">{project.model}</span>
          <NetworkIndicator
            enabled={project.internetEnabled}
            onToggle={(enabled) => void setInternet(enabled)}
          />
          <ModeIndicator
            mode={project.chatMode}
            onToggle={(mode) => void setChatMode(mode)}
          />
          <button
            type="button"
            className={`composer__prompt-btn${samplingOpen ? " composer__prompt-btn--active" : ""}`}
            aria-expanded={samplingOpen}
            onClick={() => setSamplingOpen((open) => !open)}
            title="Adjust sampling parameters"
          >
            Sampling
          </button>
          {samplingOpen && (
            <div className="composer__prompt" style={{ marginTop: 8 }}>
              <div className="composer__prompt-head">
                <span className="composer__prompt-title">Sampling</span>
              </div>
              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: 8, padding: 8 }}>
                <label>
                  Temp
                  <input
                    type="number"
                    step="0.1"
                    min="0"
                    max="2"
                    value={temperatureDisplay}
                    onChange={(e) => {
                      setTemperatureDisplay(e.target.value);
                      const n = Number(e.target.value);
                      if (!Number.isNaN(n)) setTemperature(n);
                    }}
                    onBlur={() => setTemperatureDisplay(String(temperature))}
                    className="input"
                  />
                </label>
                <label>
                  Top-p
                  <input
                    type="number"
                    step="0.05"
                    min="0"
                    max="1"
                    value={topPDisplay}
                    onChange={(e) => {
                      setTopPDisplay(e.target.value);
                      const n = Number(e.target.value);
                      if (!Number.isNaN(n)) setTopP(n);
                    }}
                    onBlur={() => setTopPDisplay(String(topP))}
                    className="input"
                  />
                </label>
                <label>
                  Top-k
                  <input
                    type="number"
                    step="1"
                    min="0"
                    value={topKDisplay}
                    onChange={(e) => {
                      setTopKDisplay(e.target.value);
                      const n = Number(e.target.value);
                      if (!Number.isNaN(n)) setTopK(Math.max(0, n));
                    }}
                    onBlur={() => setTopKDisplay(String(topK))}
                    className="input"
                  />
                </label>
              </div>
            </div>
          )}
          <span className="composer__hint">Enter to send · Shift+Enter for a new line</span>
        </div>
      </div>
    </div>
  );
}
