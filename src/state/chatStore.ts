import { create } from "zustand";
import { stopChat, streamChat } from "../api/chat";
import * as projectsApi from "../api/projects";
import type { AgentStreamEvent, ChatMessage, ChatSummary } from "../types";

let activeController: AbortController | null = null;

function newId(): string {
  return Math.random().toString(36).slice(2, 10);
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * Appends a reasoning or answer delta to the step for the model call that
 * produced it. A new call therefore starts a fresh block instead of growing
 * the previous call's thinking.
 */
function appendAgentText(
  chat: ChatMessage,
  call: number,
  kind: "reasoning" | "content",
  delta: string,
): ChatMessage {
  const steps = [...(chat.steps ?? [])];
  let index = -1;
  for (let position = steps.length - 1; position >= 0; position -= 1) {
    const step = steps[position];
    if (step.kind === "tool") break;
    if (step.kind === kind && step.call === call) {
      index = position;
      break;
    }
  }
  if (index < 0) {
    steps.push({ id: newId(), kind, call, text: delta });
  } else {
    steps[index] = { ...steps[index], text: `${steps[index].text ?? ""}${delta}` };
  }
  return { ...chat, steps };
}

/** Adds a live tool row, then updates it in place when the call finishes. */
function applyAgentEvent(chat: ChatMessage, event: AgentStreamEvent): ChatMessage {
  const steps = [...(chat.steps ?? [])];
  if (event.type === "toolStart") {
    steps.push({
      id: event.id,
      kind: "tool",
      call: event.call,
      tool: {
        id: event.id,
        name: event.name,
        input: event.input,
        status: "running",
        summary: "",
      },
    });
  } else {
    const index = steps.findIndex((step) => step.kind === "tool" && step.tool?.id === event.id);
    if (index >= 0) {
      const step = steps[index];
      steps[index] = {
        ...step,
        tool: {
          ...step.tool!,
          status: event.ok ? "done" : "error",
          summary: event.summary,
        },
      };
    }
  }
  return { ...chat, steps };
}

interface ChatState {
  /** The conversation currently on screen, per project. */
  byProject: Record<string, ChatMessage[]>;
  /** The saved session each project is looking at; null means a new chat. */
  activeChat: Record<string, string | null>;
  /** Sessions saved in the project's chats/ folder. */
  chats: Record<string, ChatSummary[]>;
  streamingProjectId: string | null;
  /** Whether assistant reasoning traces are visible in the transcript. */
  showThinking: boolean;

  /** Refreshes the saved-chat list for a project. */
  load(projectId: string): Promise<void>;
  /** Reopens a saved conversation. */
  openChat(projectId: string, chatId: string): Promise<void>;
  /** Starts a blank conversation with a completely fresh cache. */
  newChat(projectId: string): void;
  removeChat(projectId: string, chatId: string): Promise<void>;
  send(projectId: string, content: string, sessionId: string | null, sampling?: { temperature: number; top_p: number; top_k: number }, attachedFiles?: string[]): Promise<void>;
  stop(): void;
  setShowThinking(show: boolean): void;
}

const SHOW_THINKING_KEY = "localaise.show-thinking";

function initialShowThinking(): boolean {
  try {
    return window.localStorage.getItem(SHOW_THINKING_KEY) !== "off";
  } catch {
    return true;
  }
}

function rememberShowThinking(show: boolean): void {
  try {
    window.localStorage.setItem(SHOW_THINKING_KEY, show ? "on" : "off");
  } catch {
    // A blocked storage API should not stop the toggle from working.
  }
}

export const useChatStore = create<ChatState>()((set, get) => ({
  byProject: {},
  activeChat: {},
  chats: {},
  streamingProjectId: null,
  showThinking: initialShowThinking(),

  setShowThinking: (show) => {
    rememberShowThinking(show);
    set({ showThinking: show });
  },

  load: async (projectId) => {
    try {
      const chats = await projectsApi.listProjectChats(projectId);
      set((state) => ({ chats: { ...state.chats, [projectId]: chats } }));
    } catch {
      // A project without a chats/ folder simply has nothing to list.
      set((state) => ({ chats: { ...state.chats, [projectId]: [] } }));
    }
  },

  openChat: async (projectId, chatId) => {
    if (get().streamingProjectId) return;
    try {
      const session = await projectsApi.readProjectChat(projectId, chatId);
      set((state) => ({
        byProject: {
          ...state.byProject,
          [projectId]: session.messages.map((stored) => ({
            id: stored.id,
            role: stored.role,
            content: stored.content,
            reasoning: stored.reasoning,
            steps: stored.steps,
            createdAt: stored.createdAt,
            citations: stored.citations,
            promptTokens: stored.promptTokens,
            completionTokens: stored.completionTokens,
            totalTokens: stored.totalTokens,
            contextTokens: stored.contextTokens,
            tokensPerSecond: stored.tokensPerSecond,
          })),
        },
        activeChat: { ...state.activeChat, [projectId]: session.id },
      }));
    } catch (error) {
      set((state) => ({
        byProject: {
          ...state.byProject,
          [projectId]: [
            ...(state.byProject[projectId] ?? []),
            {
              id: newId(),
              role: "assistant",
              content: message(error),
              createdAt: new Date().toISOString(),
            },
          ],
        },
      }));
    }
  },

  newChat: (projectId) => {
    if (get().streamingProjectId) return;
    set((state) => ({
      byProject: { ...state.byProject, [projectId]: [] },
      activeChat: { ...state.activeChat, [projectId]: null },
    }));
  },

  removeChat: async (projectId, chatId) => {
    try {
      await projectsApi.deleteProjectChat(projectId, chatId);
    } catch {
      // The file may already be gone; the list refresh below is the truth.
    }

    set((state) => {
      const active = state.activeChat[projectId];
      return {
        chats: {
          ...state.chats,
          [projectId]: (state.chats[projectId] ?? []).filter((chat) => chat.id !== chatId),
        },
        byProject:
          active === chatId
            ? { ...state.byProject, [projectId]: [] }
            : state.byProject,
        activeChat:
          active === chatId
            ? { ...state.activeChat, [projectId]: null }
            : state.activeChat,
      };
    });
  },

  send: async (projectId, content, sessionId: string | null, sampling?: { temperature: number; top_p: number; top_k: number }, attachedFiles?: string[]) => {
    const prompt = content.trim();
    if (!prompt && (!attachedFiles || attachedFiles.length === 0) || get().streamingProjectId) return;

    const chatSessionId = sessionId ?? get().activeChat[projectId] ?? null;
    const userMessage: ChatMessage = {
      id: newId(),
      role: "user",
      content: prompt,
      createdAt: new Date().toISOString(),
    };
    const answerId = newId();
    const answer: ChatMessage = {
      id: answerId,
      role: "assistant",
      content: "",
      reasoning: "",
      createdAt: new Date().toISOString(),
      pending: true,
    };

    set((state) => ({
      byProject: {
        ...state.byProject,
        [projectId]: [...(state.byProject[projectId] ?? []), userMessage, answer],
      },
      streamingProjectId: projectId,
    }));

    const controller = new AbortController();
    activeController = controller;

    const patchAnswer = (updater: (message: ChatMessage) => ChatMessage) => {
      set((state) => ({
        byProject: {
          ...state.byProject,
          [projectId]: (state.byProject[projectId] ?? []).map((message) =>
            message.id === answerId ? updater(message) : message,
          ),
        },
      }));
    };

    try {
      const result = await streamChat(
        projectId,
        prompt,
        {
          signal: controller.signal,
          onToken: (token, call) =>
            patchAnswer((message) => {
              const next = { ...message, content: message.content + token };
              return call === undefined ? next : appendAgentText(next, call, "content", token);
            }),
          onReasoning: (delta, call) =>
            patchAnswer((message) => {
              const next = {
                ...message,
                reasoning: (message.reasoning ?? "") + delta,
              };
              return call === undefined ? next : appendAgentText(next, call, "reasoning", delta);
            }),
          onAgentEvent: (event) =>
            patchAnswer((message) => applyAgentEvent(message, event)),
        },
        chatSessionId,
        sampling,
        attachedFiles,
      );
      patchAnswer((message) => ({
        ...message,
        content: result.content,
        reasoning: result.reasoning || message.reasoning,
        steps: result.steps?.length ? result.steps : message.steps,
        citations: result.citations,
        promptTokens: result.promptTokens,
        completionTokens: result.completionTokens,
        totalTokens: result.totalTokens,
        contextTokens: result.contextTokens,
        tokensPerSecond: result.tokensPerSecond,
        pending: false,
      }));
      set((state) => ({
        activeChat: { ...state.activeChat, [projectId]: result.sessionId || sessionId },
      }));
      // The turn is on disk now; refresh the session list in the background.
      void get().load(projectId);
    } catch (error) {
      const detail = message(error);
      patchAnswer((message) => ({
        ...message,
        content: message.content || `The local model did not respond: ${detail}`,
        pending: false,
      }));
    } finally {
      // A stopped turn has already handed streaming back; a newer turn may
      // own it now, so only the controller that is still current clears it.
      if (activeController === controller) {
        activeController = null;
        set({ streamingProjectId: null });
      }
    }
  },

  stop: () => {
    const projectId = get().streamingProjectId;
    // The Rust loop keeps running until it is told to stop; aborting the
    // listener alone would only hide its output.
    if (projectId) void stopChat(projectId);
    activeController?.abort();
    activeController = null;
    set((state) => ({
      streamingProjectId: null,
      byProject: projectId
        ? {
            ...state.byProject,
            [projectId]: (state.byProject[projectId] ?? []).map((chat) =>
              chat.pending ? { ...chat, pending: false } : chat,
            ),
          }
        : state.byProject,
    }));
  },
}));
