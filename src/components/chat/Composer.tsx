import { useEffect, useRef, useState } from "react";
import { ArrowUpIcon, StopIcon } from "../ui/icons";
import { NetworkIndicator } from "../layout/NetworkIndicator";
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
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const promptRef = useRef<HTMLTextAreaElement>(null);
  const setSystemPrompt = useProjectStore((state) => state.setSystemPrompt);
  const setInternet = useProjectStore((state) => state.setInternet);

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
            placeholder={`Ask about the sources in ${project.name}…`}
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
          <span className="composer__hint">Enter to send · Shift+Enter for a new line</span>
        </div>
      </div>
    </div>
  );
}
