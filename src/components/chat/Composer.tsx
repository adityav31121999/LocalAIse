import { useEffect, useRef, useState } from "react";
import { ArrowUpIcon, PaperclipIcon, StopIcon } from "../ui/icons";
import { useProjectStore } from "../../state/projectStore";
import { useNavStore } from "../../state/navStore";
import { formatBytes } from "../../util/format";
import type { Project } from "../../types";

/** pdf.js needs a worker URL to extract text off the main thread. Vite exposes
 *  `?url` imports as string URLs; declare the module so TypeScript accepts it. */
declare module "pdfjs-dist/build/pdf.worker.mjs?url";
import * as pdfjsLib from "pdfjs-dist";

/** Extensions whose contents can be read as plain text and sent to the model. */
const TEXT_EXTENSIONS = [
  ".md", ".txt", ".json", ".csv", ".ts", ".js", ".tsx", ".jsx",
  ".py", ".html", ".htm", ".xml", ".yaml", ".yml", ".log", ".ini",
  ".toml", ".cfg", ".rtf", ".pdf",
];

function isTextFile(name: string): boolean {
  const ext = name.slice(name.lastIndexOf(".")).toLowerCase();
  return TEXT_EXTENSIONS.includes(ext);
}

function readTextFile(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(typeof reader.result === "string" ? reader.result : String(reader.result ?? ""));
    reader.onerror = () => reject(reader.error);
    reader.readAsText(file);
  });
}

/** Extracts the text of every page of a PDF using pdf.js. */
async function readPdfFile(file: File): Promise<string> {
  const { default: pdfWorkerUrl } = await import(
    "pdfjs-dist/build/pdf.worker.mjs?url"
  );
  pdfjsLib.GlobalWorkerOptions.workerSrc = pdfWorkerUrl as string;
  const arrayBuffer = await file.arrayBuffer();
  const pdf = await pdfjsLib.getDocument({ data: arrayBuffer }).promise;
  const parts: string[] = [];
  for (let i = 1; i <= pdf.numPages; i++) {
    const page = await pdf.getPage(i);
    const content = await page.getTextContent();
    parts.push(
      content.items
        .map((item: unknown) => {
          const obj = item as { str?: string };
          return obj.str ?? "";
        })
        .join(" "),
    );
  }
  return parts.join("\n");
}

/** Reads an attached file's text. Text files are read directly; PDFs are parsed with pdf.js. */
async function readFileContent(file: File): Promise<string> {
  const name = file.name.toLowerCase();
  if (name.endsWith(".pdf")) {
    return readPdfFile(file);
  }
  return readTextFile(file);
}

interface ComposerProps {
  project: Project;
  streaming: boolean;
  onSend(text: string, sampling: { temperature: number; top_p: number; top_k: number }): void;
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
  const [attachedFiles, setAttachedFiles] = useState<File[]>([]);
  const attachInputRef = useRef<HTMLInputElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const promptRef = useRef<HTMLTextAreaElement>(null);
  const setSystemPrompt = useProjectStore((state) => state.setSystemPrompt);
  const composerInsert = useNavStore((state) => state.composerInsert);
  const consumeComposerInsert = useNavStore((state) => state.consumeComposerInsert);

  useEffect(() => {
    const element = textareaRef.current;
    if (!element) return;
    element.style.height = "auto";
    element.style.height = `${Math.min(element.scrollHeight, 180)}px`;
  }, [value]);

  // "Cite in Chat" from the document viewer queues a passage here. Append it
  // as its own block so a half-written question is never overwritten.
  useEffect(() => {
    if (!composerInsert) return;
    setValue((previous) => {
      const trimmed = previous.trimEnd();
      return trimmed ? `${trimmed}\n\n${composerInsert}` : composerInsert;
    });
    consumeComposerInsert();
    requestAnimationFrame(() => {
      const element = textareaRef.current;
      if (!element) return;
      element.focus();
      element.setSelectionRange(element.value.length, element.value.length);
    });
  }, [composerInsert, consumeComposerInsert]);

  useEffect(() => {
    if (promptOpen) promptRef.current?.focus();
  }, [promptOpen]);

  const canSend = (value.trim().length > 0 || attachedFiles.length > 0) && !streaming;

  const submit = async () => {
    if (!canSend) return;
    let prompt = value.trim();
    if (attachedFiles.length) {
      const parts = await Promise.all(attachedFiles.map(readFileContent));
      prompt =
        prompt +
        "\n\n---\nAttached files:\n" +
        parts
          .map((part, index) => `### ${attachedFiles[index].name}\n${part}`)
          .join("\n\n---\n\n");
    }
    onSend(prompt, { temperature, top_p: topP, top_k: topK });
    setValue("");
    setAttachedFiles([]);
    textareaRef.current?.focus();
  };

  const openAttachments = () => {
    attachInputRef.current?.click();
  };

  const handleAttachmentsChange = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(event.target.files ?? []);
    event.target.value = "";
    if (!files.length) return;
    const textFiles: File[] = [];
    const skipped: File[] = [];
    for (const file of files) {
      (isTextFile(file.name) ? textFiles : skipped).push(file);
    }
    if (skipped.length) {
      alert(
        "Only text files and PDFs can be attached to a chat message.\n\n" +
          "Skipped: " +
          skipped.map((file) => file.name).join(", ") +
          "\n\nAdd documents and other binaries to the Files view so they are indexed and searchable instead.",
      );
    }
    setAttachedFiles((previous) => [...previous, ...textFiles]);
  };

  const removeAttachment = (file: File) => {
    setAttachedFiles((previous) => previous.filter((item) => item !== file));
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
          <input
            ref={attachInputRef}
            type="file"
            multiple
            hidden
            onChange={handleAttachmentsChange}
            aria-label="Attach files"
          />
          <button
            type="button"
            className="composer__attach"
            onClick={openAttachments}
            disabled={streaming}
            title="Attach files"
          >
            <PaperclipIcon size={14} />
            <span className="sr-only">Attach files</span>
          </button>

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
        {attachedFiles.length > 0 && (
          <div className="composer__attachments">
            <span className="composer__attachments-label">Attached files:</span>
            {attachedFiles.map((file) => (
              <span key={`${file.name}-${file.size}`} className="composer__attach-chip">
                {file.name}
                <span className="composer__attach-size">({formatBytes(file.size)})</span>
                <button
                  type="button"
                  className="composer__attach-remove"
                  onClick={() => removeAttachment(file)}
                  aria-label={`Remove ${file.name}`}
                >
                  ×
                </button>
              </span>
            ))}
          </div>
        )}

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
