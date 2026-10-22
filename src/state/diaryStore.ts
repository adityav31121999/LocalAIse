import { create } from "zustand";
import * as diaryApi from "../api/personal";
import { todayKey } from "../util/calendar";
import type { DiaryPage } from "../types";

export type SaveState = "idle" | "saving" | "saved" | "error";

const SAVE_DELAY = 600;
const saveTimers = new Map<string, number>();

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

interface DiaryState {
  pages: DiaryPage[];
  activePageId: string | null;
  /** A date picked in the calendar that has no entry yet. */
  pendingDate: string | null;
  /** The project whose diary/ folder is open; null means the personal space. */
  scopeProjectId: string | null;
  status: "idle" | "loading" | "ready" | "error";
  error: string | null;
  saveState: SaveState;

  load(projectId?: string | null): Promise<void>;
  selectDate(date: string): void;
  selectPage(pageId: string): void;
  createPage(date: string | null): Promise<void>;
  updatePage(pageId: string, patch: Partial<Pick<DiaryPage, "title" | "body">>): void;
  removePage(pageId: string): Promise<void>;
}

export const useDiaryStore = create<DiaryState>()((set, get) => ({
  pages: [],
  activePageId: null,
  pendingDate: null,
  scopeProjectId: null,
  status: "idle",
  error: null,
  saveState: "idle",

  load: async (projectId = null) => {
    // Switching between the personal space and a project starts cleanly.
    set({
      status: "loading",
      error: null,
      scopeProjectId: projectId,
      pages: [],
      activePageId: null,
      pendingDate: null,
      saveState: "idle",
    });
    try {
      const pages = await diaryApi.listDiaryPages(projectId);
      const mostRecent = [...pages].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))[0];
      const initial = pages.find((page) => page.date === todayKey()) ?? mostRecent ?? null;
      set({
        pages,
        status: "ready",
        activePageId: initial?.id ?? null,
      });
    } catch (error) {
      set({ status: "error", error: message(error) });
    }
  },

  selectDate: (date) => {
    const existing = get().pages.find((page) => page.date === date);
    if (existing) set({ activePageId: existing.id, pendingDate: null });
    else set({ activePageId: null, pendingDate: date });
  },

  selectPage: (pageId) => set({ activePageId: pageId, pendingDate: null }),

  createPage: async (date) => {
    try {
      const page = await diaryApi.createDiaryPage(
        { date, title: date ? "" : "Untitled page" },
        get().scopeProjectId,
      );
      set((state) => ({
        pages: [page, ...state.pages],
        activePageId: page.id,
        pendingDate: null,
        saveState: "idle",
      }));
    } catch (error) {
      set({ error: message(error) });
    }
  },

  updatePage: (pageId, patch) => {
    set((state) => ({
      pages: state.pages.map((page) =>
        page.id === pageId ? { ...page, ...patch, updatedAt: new Date().toISOString() } : page,
      ),
      saveState: "saving",
    }));

    const pending = saveTimers.get(pageId);
    if (pending) window.clearTimeout(pending);

    saveTimers.set(
      pageId,
      window.setTimeout(() => {
        saveTimers.delete(pageId);
        const page = get().pages.find((item) => item.id === pageId);
        if (!page) return;
        const projectId = get().scopeProjectId;
        void diaryApi
          .saveDiaryPage(page, projectId)
          .then((saved) =>
            set((state) => ({
              pages: state.pages.map((item) => (item.id === saved.id ? saved : item)),
              saveState: saved.id === state.activePageId ? "saved" : state.saveState,
            })),
          )
          .catch((error) => set({ saveState: "error", error: message(error) }));
      }, SAVE_DELAY),
    );
  },

  removePage: async (pageId) => {
    const pages = get().pages;
    const index = pages.findIndex((page) => page.id === pageId);
    const remaining = pages.filter((page) => page.id !== pageId);
    const fallback = remaining[Math.min(index, remaining.length - 1)] ?? null;

    set((state) => ({
      pages: remaining,
      activePageId: state.activePageId === pageId ? (fallback?.id ?? null) : state.activePageId,
      pendingDate: null,
    }));

    try {
      await diaryApi.deleteDiaryPage(pageId, get().scopeProjectId);
    } catch (error) {
      set({ error: message(error) });
    }
  },
}));
