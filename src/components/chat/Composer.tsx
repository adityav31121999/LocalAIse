import { useEffect, useRef, useState } from "react";
import { ArrowUpIcon, PaperclipIcon, StopIcon } from "../ui/icons";
import { useProjectStore } from "../../state/projectStore";
import { useNavStore } from "../../state/navStore";
import { formatBytes } from "../../util/format";
import { invoke } from "@tauri-apps/api/core";
import type { Project } from "../../types";

/** Default system prompt for chat mode (mirrors the Rust `system_prompt` function). */
function defaultChatSystemPrompt(projectName: string): string {
  return (
    `You are localAIse, a private assistant that works only inside the folder “${projectName}”. ` +
    `The files in that folder never leave this machine. Answer in clear, plain prose, and say ` +
    `when you are not sure instead of guessing.`
  );
}

/** Default system prompt for agent mode (mirrors the Rust `agent_chat::instructions` function). */
function defaultAgentSystemPrompt(projectName: string, internetEnabled: boolean, hasRagContext: boolean): string {
  let text = defaultChatSystemPrompt(projectName);
  text +=
    "\n\nAGENT RULES: You are in agent mode on this private, local folder. " +
    "Before answering, you may act with the offered tools: read files, list folders " +
    "or search the project's indexed sources when the answer needs them, and use " +
    "the diary and calendar tools when the question touches the user's days. " +
    "write_file only when the user asked for something to be written. Cite files by " +
    "their relative path. Answer in plain prose; keep tool calls to one at a time.";

  if (hasRagContext) {
    text +=
      "\n\nRelevant context from the project's indexed sources has been " +
      "prepended to this conversation. Use that information first before calling " +
      "search_local. Only call search_local if you need additional information not " +
      "covered by the provided context.";
  }

  text += internetEnabled
    ? " The internet toggle is on, so web_search and web_fetch are available " +
      "when the folders cannot answer the question."
    : " The internet toggle is off, so the web tools are unavailable; say so " +
      "rather than guessing at the web.";

  return text;
}

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

/** Converts a File to base64 string. */
function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result as string;
      // Remove data URL prefix (e.g., "data:text/plain;base64,")
      const base64 = result.split(",")[1] ?? result;
      resolve(base64);
    };
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
}

interface ComposerProps {
  project: Project;
  streaming: boolean;
  onSend(text: string, sampling: { temperature: number; top_p: number; top_k: number }, attachedFiles?: string[]): void;
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
  const [uploadingAttachments, setUploadingAttachments] = useState(false);
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

  const canSend = (value.trim().length > 0 || attachedFiles.length > 0) && !streaming && !uploadingAttachments;

  const submit = async () => {
    if (!canSend) return;
    
    let attachedFilePaths: string[] = [];
    
    // Upload attached files to project first
    if (attachedFiles.length > 0) {
      setUploadingAttachments(true);
      try {
        const uploadPromises = attachedFiles.map(async (file) => {
          const base64 = await fileToBase64(file);
          const path = await invoke<string>("attach_file_to_project", {
            projectId: project.id,
            fileName: file.name,
            fileBytesBase64: base64,
          });
          return path;
        });
        attachedFilePaths = await Promise.all(uploadPromises);
      } catch (error) {
        console.error("Failed to attach files:", error);
        alert("Failed to attach files. Please try again.");
        setUploadingAttachments(false);
        return;
      }
      setUploadingAttachments(false);
    }
    
    const prompt = value.trim();
    onSend(prompt, { temperature, top_p: topP, top_k: topK }, attachedFilePaths);
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

  /** Returns the default system prompt for the current chat mode. */
  const getDefaultPrompt = () => {
    if (project.chatMode === "agent") {
      return defaultAgentSystemPrompt(project.name, project.internetEnabled, project.sourceCount > 0);
    }
    return defaultChatSystemPrompt(project.name);
  };

  return (
    <div className="composer">
      <div className="composer__inner">
        {promptOpen && (
          <div className="composer__prompt">
            <div className="composer__prompt-head">
              <span className="composer__prompt-title">System prompt · {project.name}</span>
              <span className="composer__prompt-state">
                {project.systemPrompt ? "custom" : `default (${project.chatMode})`}
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
              placeholder={project.systemPrompt ? "" : getDefaultPrompt()}
              aria-label={`System prompt for ${project.name}`}
            />
            <div className="composer__prompt-foot">
              <span className="composer__prompt-hint">
                Saved with this project · Ctrl+Enter to save · Shows {project.chatMode} mode default
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
