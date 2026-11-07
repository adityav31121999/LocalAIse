import { useEffect, useMemo, useRef, useState } from "react";
import { Composer } from "./Composer";
import { MessageList } from "./MessageList";
import { PlusIcon, SparkIcon, TrashIcon, MoreIcon, ChevronDownIcon } from "../ui/icons";
import { NetworkIndicator } from "../layout/NetworkIndicator";
import { ModeIndicator } from "../layout/ModeIndicator";
import { PanelErrorBoundary } from "../ui/PanelErrorBoundary";
import { useChatStore } from "../../state/chatStore";
import { useNavStore } from "../../state/navStore";
import { useProjectStore } from "../../state/projectStore";
import { useRuntimeStore } from "../../state/runtimeStore";
import { useSettingsStore } from "../../state/settingsStore";
import { chatLabel, chatNumber, formatCount } from "../../util/format";
import type { ChatMessage, ChatSummary, Project, ProjectTemplate } from "../../types";
import { DocumentViewer } from "../files/DocumentViewer";

const NO_MESSAGES: ChatMessage[] = [];
const NO_CHATS: ChatSummary[] = [];

const SUGGESTIONS: Record<ProjectTemplate, string[]> = {
  budget: [
    "Where did spending rise the most this quarter?",
    "Which invoices still need reconciling?",
  ],
  research: [
    "Summarise the sources I should cite first.",
    "What gap do these papers leave open?",
  ],
  "content-writing": [
    "Rewrite the homepage intro in a plainer voice.",
    "Which claims on the pricing page are unsupported?",
  ],
  general: ["What did I write most recently?", "Walk me through the files in this folder."],
};

export function ChatView({ project }: { project: Project }) {
  const messages = useChatStore((state) => state.byProject[project.id]) ?? NO_MESSAGES;
  const storedChats = useChatStore((state) => state.chats[project.id]) ?? NO_CHATS;
  const activeChatId = useChatStore((state) => state.activeChat[project.id]);
  const streaming = useChatStore((state) => state.streamingProjectId === project.id);
  const showThinking = useChatStore((state) => state.showThinking);
  const setShowThinking = useChatStore((state) => state.setShowThinking);
  const send = useChatStore((state) => state.send);
  const stop = useChatStore((state) => state.stop);
  const load = useChatStore((state) => state.load);
  const openChat = useChatStore((state) => state.openChat);
  const newChat = useChatStore((state) => state.newChat);
  const removeChat = useChatStore((state) => state.removeChat);
  const setInternet = useProjectStore((state) => state.setInternet);
  const setChatMode = useProjectStore((state) => state.setChatMode);

  // The model the runtime is actually running right now — what answers are
  // written with. Falls back to the project's selected model when nothing is
  // loaded yet.
  const runtime = useRuntimeStore((state) => state.status);
  const modelInUse =
    runtime.state === "ready" && runtime.model
      ? runtime.model
      : runtime.state === "starting"
        ? "Starting…"
        : project.model;

  // Cumulative tokens across the whole conversation — prompt, system prompt,
  // thinking and response summed over every turn the model ran.
  const totalConversationTokens = messages.reduce(
    (sum, message) => sum + (message.totalTokens ?? 0),
    0,
  );
  // The context window the user selected; 0 means the model's own default.
  const selectedContext = runtime.profile?.contextTokens ?? 0;

  const openDocument = useNavStore((state) => state.openDocument);
  const setOpenDocument = useNavStore((state) => state.setOpenDocument);

  // Numbered files read in file order, so Chat 1, Chat 2, Chat 3 stay in the
  // order the sessions were created rather than jumping to the last edited.
  const chats = useMemo(
    () => [...storedChats].sort((a, b) => chatNumber(a.id) - chatNumber(b.id)),
    [storedChats],
  );
  const activeChat = chats.find((chat) => chat.id === activeChatId);

  useEffect(() => {
    void load(project.id);
  }, [project.id, load]);

  // Reopening a project restores the conversation it was on. A chat started
  // with "New chat" is explicit, so it is never replaced automatically.
  useEffect(() => {
    if (activeChatId === undefined && storedChats.length > 0) {
      const mostRecent = [...storedChats].sort((a, b) =>
        b.updatedAt.localeCompare(a.updatedAt),
      )[0];
      void openChat(project.id, mostRecent.id);
    }
  }, [activeChatId, storedChats, project.id, openChat]);

  const isDocumentOpen = openDocument?.projectId === project.id;

  // When a document opens the explorer is in the way: remember whether the
  // user had it open, collapse it, then restore their preference on close.
  const sidebarCollapsed = useSettingsStore((state) => state.sidebarCollapsed);
  const setSidebarCollapsed = useSettingsStore((state) => state.setSidebarCollapsed);
  const hadSidebarOpen = useRef(false);
  const wasDocumentOpen = useRef(false);

  useEffect(() => {
    if (isDocumentOpen && !wasDocumentOpen.current) {
      // Only on the open transition, so manually reopening the explorer while
      // reading a document is not immediately undone.
      if (!sidebarCollapsed) {
        hadSidebarOpen.current = true;
        setSidebarCollapsed(true);
      }
    } else if (!isDocumentOpen && wasDocumentOpen.current) {
      if (hadSidebarOpen.current) {
        hadSidebarOpen.current = false;
        setSidebarCollapsed(false);
      }
    }
    wasDocumentOpen.current = isDocumentOpen;
  }, [isDocumentOpen, sidebarCollapsed, setSidebarCollapsed]);

  // Width of the document pane, as a percentage of the split area. Dragging
  // the splitter clamps it so neither side can be squeezed out of existence.
  const [docWidth, setDocWidth] = useState(46);
  const [dragging, setDragging] = useState(false);
  const paneRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!dragging) return;
    document.body.classList.add("chat-resizing");
    const handleMove = (event: PointerEvent) => {
      const pane = paneRef.current;
      if (!pane) return;
      const rect = pane.getBoundingClientRect();
      if (rect.width === 0) return;
      const next = ((rect.right - event.clientX) / rect.width) * 100;
      setDocWidth(Math.min(68, Math.max(28, next)));
    };
    const stop = () => setDragging(false);
    window.addEventListener("pointermove", handleMove);
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
    return () => {
      document.body.classList.remove("chat-resizing");
      window.removeEventListener("pointermove", handleMove);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
    };
  }, [dragging]);

  const handleCloseDocument = () => {
    setOpenDocument(null);
  };

  const renderChatContent = () => {
    if (messages.length === 0) {
      return (
        <div className="chat__scroll">
          <div className="chat-empty">
            <SparkIcon size={22} />
            <h2 className="chat-empty__title">Ask {project.name} a question</h2>
            <p className="chat-empty__text">
              Answers are written by a model running on this machine, grounded in the{" "}
              {project.sourceCount} sources inside this folder. Every claim links back to the file it
              came from, and each conversation is saved as JSON in <code>chats/</code>.
            </p>
            <div className="suggestions">
              {SUGGESTIONS[project.template].map((suggestion) => (
                <button
                  key={suggestion}
                  type="button"
                  className="suggestion"
                  onClick={() => void send(project.id, suggestion, activeChatId)}
                >
                  {suggestion}
                </button>
              ))}
            </div>
          </div>
        </div>
      );
    }

    return (
      <MessageList messages={messages} streaming={streaming} showThinking={showThinking} />
    );
  };

  const [chatMenuOpen, setChatMenuOpen] = useState(false);
  const chatMenuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!chatMenuOpen) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!chatMenuRef.current?.contains(event.target as Node)) setChatMenuOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setChatMenuOpen(false);
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [chatMenuOpen]);

  return (
    <div className="chat" style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <div className="chat__bar" style={{ flexShrink: 0 }}>
        <label className="chat__picker">
          <span className="sr-only">Saved chats</span>
          <select
            className="chat__select"
            value={activeChatId ?? ""}
            disabled={streaming}
            onChange={(event) => {
              const id = event.target.value;
              if (id) void openChat(project.id, id);
              else newChat(project.id);
            }}
          >
            <option value="">New chat (fresh cache)</option>
            {chats.map((chat) => (
              <option key={chat.id} value={chat.id}>
                {chatLabel(chat.id, chat.title)}
              </option>
            ))}
          </select>
        </label>

        <span className="muted chat__bar-meta">
          {activeChat?.title ? (
            <>
              <span className="chat__bar-title" title={activeChat.title}>
                {activeChat.title}
              </span>
              {" · "}
            </>
          ) : null}
          {messages.length > 0
            ? `${messages.length} messages`
            : `${chats.length} saved chat${chats.length === 1 ? "" : "s"} in chats/`}
          {" · "}
          {showThinking ? "thinking on" : "thinking off"}
        </span>

        <span className="project-header__spacer" />

        {totalConversationTokens > 0 && (
          <span className="chat__bar-tokens" title="Total tokens used in this conversation — prompt, system prompt, thinking and response">
            {formatCount(totalConversationTokens)} / {selectedContext ? formatCount(selectedContext) : "model default"}
          </span>
        )}
        <span className="mono chat__bar-model" title="Model used for this conversation">
          {modelInUse}
        </span>
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
          className="thinking-toggle"
          role="switch"
          aria-checked={showThinking}
          aria-label="Show thinking"
          title={showThinking ? "Hide the model's thinking" : "Show the model's thinking"}
          onClick={() => setShowThinking(!showThinking)}
        >
          <span className="thinking-toggle__track" aria-hidden="true">
            <span className="thinking-toggle__thumb" />
          </span>
          <span className="thinking-toggle__label">Thinking</span>
        </button>

        <div className="chat__bar-actions" ref={chatMenuRef}>
          {activeChatId ? (
            <button
              type="button"
              className="icon-btn icon-btn--sm"
              title="Delete this chat"
              disabled={streaming}
              onClick={() => void removeChat(project.id, activeChatId)}
            >
              <TrashIcon size={14} />
              <span className="sr-only">Delete this chat</span>
            </button>
          ) : null}

          <button
            type="button"
            className="btn btn--secondary btn--sm"
            disabled={streaming}
            onClick={() => newChat(project.id)}
            title="Start a new conversation with a completely fresh cache"
          >
            <PlusIcon size={14} />
            <span className="chat__new-chat-label">New chat</span>
          </button>

          <button
            type="button"
            className="icon-btn icon-btn--sm chat__overflow-btn"
            aria-expanded={chatMenuOpen}
            aria-haspopup="menu"
            aria-label="More chat actions"
            onClick={() => setChatMenuOpen((open) => !open)}
          >
            <MoreIcon size={15} />
          </button>

          {chatMenuOpen && (
            <div className="chat__overflow-menu card-menu" role="menu">
              {activeChatId && (
                <button
                  type="button"
                  role="menuitem"
                  className="card-menu__danger"
                  onClick={() => {
                    setChatMenuOpen(false);
                    void removeChat(project.id, activeChatId!);
                  }}
                >
                  <TrashIcon size={13} />
                  Delete this chat
                </button>
              )}
              <button
                type="button"
                role="menuitem"
                disabled={streaming}
                onClick={() => {
                  setChatMenuOpen(false);
                  newChat(project.id);
                }}
              >
                <PlusIcon size={13} />
                New chat (fresh cache)
              </button>
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setChatMenuOpen(false);
                  setShowThinking(!showThinking);
                }}
              >
                {showThinking ? (
                  <>
                    <ChevronDownIcon size={13} />
                    Hide thinking
                  </>
                ) : (
                  <>
                    <SparkIcon size={13} />
                    Show thinking
                  </>
                )}
              </button>
            </div>
          )}
        </div>
      </div>

      <div className="chat__pane" ref={paneRef} style={{ flex: 1, minHeight: 0 }}>
        {/* Main chat area */}
        <div
          className="chat__pane-main"
          style={{
            flex: isDocumentOpen ? "1 1 0%" : "1 1 100%",
            borderRight: isDocumentOpen ? undefined : "none",
          }}
        >
          {renderChatContent()}

          <Composer
            project={project}
            streaming={streaming}
            onSend={(text, sampling, attachedFiles) => void send(project.id, text, activeChatId, sampling, attachedFiles)}
            onStop={stop}
          />
        </div>

        {/* Document viewer panel */}
        {isDocumentOpen && openDocument && (
          <>
            <div
              className="chat__splitter"
              role="separator"
              aria-orientation="vertical"
              aria-label="Resize document panel"
              aria-valuenow={Math.round(docWidth)}
              aria-valuemin={28}
              aria-valuemax={68}
              data-dragging={dragging}
              onPointerDown={(event) => {
                event.preventDefault();
                setDragging(true);
              }}
              onDoubleClick={() => setDocWidth(46)}
              title="Drag to resize · double-click to reset"
            />
            <div
              className="chat__pane-doc"
              style={{ flex: `0 0 ${docWidth}%`, width: `${docWidth}%` }}
            >
              <PanelErrorBoundary label="document viewer">
                <DocumentViewer
                  project={project}
                  filePath={openDocument.filePath}
                  fileName={openDocument.fileName}
                  onClose={handleCloseDocument}
                />
              </PanelErrorBoundary>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
