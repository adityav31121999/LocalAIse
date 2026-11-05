import { useEffect, useRef, useState } from "react";
import { ArrowUpIcon, PaperclipIcon, PauseIcon, StopIcon } from "../ui/icons";
import { useProjectStore } from "../../state/projectStore";
import { useNavStore } from "../../state/navStore";
import { invoke } from "../../api";
import { MentionAutocomplete } from "./MentionAutocomplete";
import type { Project } from "../../types";

/** Default system prompt for chat mode (mirrors the Rust `system_prompt` function). */
function defaultChatSystemPrompt(projectName: string): string {
  return (
    `You are localAIse, a private assistant that works only inside the ` +
    `project folder “${projectName}”. The files in that folder never leave this machine.\n\n` +
    `HOW FILES ARE NAMED HERE:\n` +
    `1. Every file is named by its path relative to the project root, such as ` +
    `"data/notes.md" or "outputs/report.md". These name the format; which files ` +
    `actually exist is a separate question.\n` +
    `2. The user mentions files with a leading @, such as @outputs/file.txt. The @ is ` +
    `only this app's chat marker: the real path is exactly what follows it, ` +
    `outputs/file.txt. This mapping is fixed — @something always means just ` +
    `"something", in every message, with no exceptions.\n` +
    `3. When you name a file — in an answer, a citation, or anything else — use the ` +
    `plain relative path. Never add @, quotes, backslashes, or absolute paths.\n\n` +
    `Answer in clear, plain prose, and say when you are not sure instead of guessing.`
  );
}

/** Default system prompt for agent mode (mirrors the Rust `agent_chat::instructions` function). */
function defaultAgentSystemPrompt(projectName: string, internetEnabled: boolean, hasRagContext: boolean): string {
  let text = defaultChatSystemPrompt(projectName);
  text +=
    "\n\nAGENT RULES — these remove all guesswork about files and tools:\n\n" +
    "HOW TO TURN A MENTION INTO A PATH: Chat messages name files with a leading @, " +
    "such as @outputs/file.txt. The @ is only this app's chat marker; the real path " +
    "is exactly what follows it, outputs/file.txt. This mapping is fixed — when a " +
    "message says @name, the request is about \"name\", every time, with no " +
    "exceptions. Reading it any other way (as a literal file name, as a special " +
    "directory, as something to ask about) is always wrong. Replace nothing but the " +
    "@, and keep the rest of the path character for character.\n\n" +
    "HOW TO CALL TOOLS: Tools accept only plain relative paths like " +
    "outputs/file.txt — never @, quotes, backslashes, or absolute paths. read_file " +
    "and list_files take such a path as given; write_file always saves inside " +
    "outputs/, so \"file.txt\" and \"outputs/file.txt\" both mean outputs/file.txt, " +
    "and you tell the user that location when you confirm. Keep the user's file, " +
    "folder, or text request exactly as given: to write \"Hello World!\" into " +
    "@outputs/file.txt you call write_file with path \"outputs/file.txt\" and " +
    "content \"Hello World!\", once. Call one tool at a time; write_file only when " +
    "the user asked for something to be written.\n\n" +
    "THINKING: Keep thinking to a few short sentences, then act. A request that " +
    "names a file needs no debate — turn @name into \"name\", call the tool with " +
    "that exact path, and move on.\n\n" +
    "IMPORTANT: For greetings, simple questions, and casual conversation, do NOT use " +
    "any tools. Answer directly without searching, reading files, or calling any " +
    "tools. Only use tools when the question genuinely requires accessing the " +
    "project's files, searching the web, or checking your diary/calendar.";

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

/** Converts a File to base64 string. An empty file resolves to "" (valid). */
function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result as string;
      // Remove data URL prefix (e.g., "data:text/plain;base64,"). A 0-byte
      // file still yields a valid data URL whose payload is the empty string.
      const comma = result.indexOf(",");
      resolve(comma >= 0 ? result.slice(comma + 1) : result);
    };
    reader.onerror = () => reject(reader.error ?? new Error(`Failed to read ${file.name}`));
    reader.readAsDataURL(file);
  });
}

/** A file the user picked with the paperclip, already copied into the project. */
interface AttachedFile {
  file: File;
  /** Path relative to the project root, e.g. `data/report.txt`. */
  path: string;
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
  const [attachedFiles, setAttachedFiles] = useState<AttachedFile[]>([]);
  const [uploadingAttachments, setUploadingAttachments] = useState(false);
  const [mentionedFilePaths, setMentionedFilePaths] = useState<string[]>([]);
  const attachInputRef = useRef<HTMLInputElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const promptRef = useRef<HTMLTextAreaElement>(null);
  const [cursorPosition, setCursorPosition] = useState<number | null>(null);
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

  const canSend =
    (value.trim().length > 0 || attachedFiles.length > 0 || mentionedFilePaths.length > 0) &&
    !streaming &&
    !uploadingAttachments;

  const submit = async () => {
    if (!canSend) return;

    // Paths from the paperclip and from `@`/`#` mentions are both relative to
    // the project root. Deduplicate in case a file was picked and mentioned.
    const attachedFilePaths = Array.from(
      new Set([...attachedFiles.map((item) => item.path), ...mentionedFilePaths]),
    );

    const prompt = value.trim() || (attachedFilePaths.length > 0 ? `Review the attached files.` : "");
    onSend(prompt, { temperature, top_p: topP, top_k: topK }, attachedFilePaths);
    setValue("");
    setAttachedFiles([]);
    setMentionedFilePaths([]);
    textareaRef.current?.focus();
    setCursorPosition(null);
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
    if (!textFiles.length) return;

    // Copy each file into the project right away so the composer can show the
    // real relative path (data/…) exactly like an `@` mention. Uploading on
    // send used to lose that path when the message reached the model.
    setUploadingAttachments(true);
    const uploaded: AttachedFile[] = [];
    try {
      for (const file of textFiles) {
        // An empty string is valid for a 0-byte file; only a rejected
        // FileReader counts as a failure.
        const base64 = await fileToBase64(file);
        const path = await invoke<string>("attach_file_to_project", {
          projectId: project.id,
          fileName: file.name,
          fileBytesBase64: base64,
        });
        if (!path) {
          throw new Error(`attach_file_to_project returned empty path for: ${file.name}`);
        }
        uploaded.push({ file, path });
      }
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      console.error("Failed to attach files:", detail);
      alert(`Failed to attach files: ${detail}`);
    } finally {
      if (uploaded.length) {
        setAttachedFiles((previous) => [...previous, ...uploaded]);
      }
      setUploadingAttachments(false);
    }
  };

  const removeAttachment = (path: string) => {
    setAttachedFiles((previous) => previous.filter((item) => item.path !== path));
  };

  const removeMention = (filePath: string) => {
    setMentionedFilePaths((previous) => previous.filter((p) => p !== filePath));
  };

  const handleMentionSelect = (filePath: string, displayText: string) => {
    const textarea = textareaRef.current;
    if (!textarea) return;

    const start = cursorPosition ?? 0;
    const before = value.slice(0, start);
    const after = value.slice(start);

    // Trim any trailing trigger/query text already typed before the cursor.
    const cleanedBefore = before.replace(/[@#][^\s]*$/, "");

    const prefix = cleanedBefore && !cleanedBefore.endsWith(" ") && !cleanedBefore.endsWith("\n") ? " " : "";
    const next = `${cleanedBefore}${prefix}${displayText}${after}`;

    setValue(next);
    setMentionedFilePaths((previous) => {
      if (previous.includes(filePath)) return previous;
      return [...previous, filePath];
    });

    const newCursor = cleanedBefore.length + prefix.length + displayText.length;
    textarea.value = next;
    textarea.setSelectionRange(newCursor, newCursor);
    textarea.dispatchEvent(new Event("input", { bubbles: true }));
    textarea.focus();
    setCursorPosition(newCursor);
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
            onChange={(event) => {
              const target = event.target as HTMLTextAreaElement;
              setValue(target.value);
              setCursorPosition(target.selectionStart ?? null);
            }}
            onSelect={(event) => {
              const target = event.target as HTMLTextAreaElement;
              setCursorPosition(target.selectionStart ?? null);
            }}
            onClick={(event) => {
              const target = event.target as HTMLTextAreaElement;
              setCursorPosition(target.selectionStart ?? null);
            }}
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
            disabled={streaming || uploadingAttachments}
            title={uploadingAttachments ? "Uploading…" : "Attach files"}
          >
            <PaperclipIcon size={14} />
            <span className="sr-only">Attach files</span>
          </button>

          <MentionAutocomplete
            project={project}
            value={value}
            cursor={cursorPosition}
            onSelect={handleMentionSelect}
          />

          {streaming ? (
            <button
              type="button"
              className={`composer__send composer__send--stop${
                project.chatMode === "agent" ? " composer__send--pause" : ""
              }`}
              onClick={onStop}
              title={
                project.chatMode === "agent"
                  ? "Pause the agent after the current step"
                  : "Stop generating"
              }
            >
              {project.chatMode === "agent" ? <PauseIcon size={14} /> : <StopIcon size={15} />}
              {project.chatMode === "agent" ? (
                <span>Pause</span>
              ) : (
                <span className="sr-only">Stop generating</span>
              )}
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
            {attachedFiles.map(({ file, path }) => (
              <span key={path} className="composer__attach-chip" title={file.name}>
                @{path}
                <button
                  type="button"
                  className="composer__attach-remove"
                  onClick={() => removeAttachment(path)}
                  aria-label={`Remove ${file.name}`}
                >
                  ×
                </button>
              </span>
            ))}
          </div>
        )}
        {mentionedFilePaths.length > 0 && (
          <div className="composer__attachments">
            <span className="composer__attachments-label">Mentioned files:</span>
            {mentionedFilePaths.map((filePath) => (
              <span key={filePath} className="composer__attach-chip">
                @{filePath}
                <button
                  type="button"
                  className="composer__attach-remove"
                  onClick={() => removeMention(filePath)}
                  aria-label={`Remove mention ${filePath}`}
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
            <div className="composer__sampling">
              <label className="composer__sampling-item">
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
                  className="composer__sampling-input"
                />
              </label>
              <label className="composer__sampling-item">
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
                  className="composer__sampling-input"
                />
              </label>
              <label className="composer__sampling-item">
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
                  className="composer__sampling-input"
                />
              </label>
            </div>
          )}
          <span className="composer__hint">Enter to send · Shift+Enter for a new line</span>
        </div>
      </div>
    </div>
  );
}
