import { create } from "zustand";
import { streamChat } from "../api/chat";
import * as projectsApi from "../api/projects";
import type { ChatMessage, ChatSummary } from "../types";

let activeController: AbortController | null = null;

function newId(): string {
  return Math.random().toString(36).slice(2, 10);
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
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
            createdAt: stored.createdAt,
            citations: stored.citations,
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
          onToken: (token) =>
            patchAnswer((message) => ({ ...message, content: message.content + token })),
          onReasoning: (delta) =>
            patchAnswer((message) => ({
              ...message,
              reasoning: (message.reasoning ?? "") + delta,
            })),
        },
        chatSessionId,
        sampling,
        attachedFiles,
      );
      patchAnswer((message) => ({
        ...message,
        content: result.content,
        reasoning: result.reasoning || message.reasoning,
        citations: result.citations,
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
      if (activeController === controller) activeController = null;
      set({ streamingProjectId: null });
    }
  },

  stop: () => {
    activeController?.abort();
    activeController = null;
    set({ streamingProjectId: null });
  },
}));
