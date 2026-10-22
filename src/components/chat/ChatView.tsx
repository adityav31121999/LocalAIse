import { useEffect } from "react";
import { Composer } from "./Composer";
import { MessageList } from "./MessageList";
import { PlusIcon, SparkIcon, TrashIcon } from "../ui/icons";
import { useChatStore } from "../../state/chatStore";
import type { ChatMessage, ChatSummary, Project, ProjectTemplate } from "../../types";

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
  const chats = useChatStore((state) => state.chats[project.id]) ?? NO_CHATS;
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

  useEffect(() => {
    void load(project.id);
  }, [project.id, load]);

  // Reopening a project restores the conversation it was on. A chat started
  // with "New chat" is explicit, so it is never replaced automatically.
  useEffect(() => {
    if (activeChatId === undefined && chats.length > 0) {
      void openChat(project.id, chats[0].id);
    }
  }, [activeChatId, chats, project.id, openChat]);

  return (
    <div className="chat">
      <div className="chat__bar">
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
                {chat.title || "Untitled chat"}
              </option>
            ))}
          </select>
        </label>

        <span className="muted chat__bar-meta">
          {messages.length > 0
            ? `${messages.length} messages`
            : `${chats.length} saved chat${chats.length === 1 ? "" : "s"} in chats/`}
          {" · "}
          {showThinking ? "thinking on" : "thinking off"}
        </span>

        <span className="project-header__spacer" />

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
          Thinking
        </button>

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
          New chat
        </button>
      </div>

      {messages.length === 0 ? (
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
                  onClick={() => void send(project.id, suggestion)}
                >
                  {suggestion}
                </button>
              ))}
            </div>
          </div>
        </div>
      ) : (
        <MessageList messages={messages} streaming={streaming} showThinking={showThinking} />
      )}

      <Composer
        project={project}
        streaming={streaming}
        onSend={(text) => void send(project.id, text)}
        onStop={stop}
      />
    </div>
  );
}
