import { create } from "zustand";
import type { AppView, WorkspaceTab } from "../types";

interface NavState {
  view: AppView;
  previousView: AppView;
  activeProjectTab: WorkspaceTab;
  setActiveProjectTab(tab: WorkspaceTab): void;
  showHome(): void;
  showDiary(): void;
  showSettings(): void;
  showThemeSettings(): void;
  closeThemeSettings(): void;
  showModelConfig(): void;
  closeModelConfig(): void;

  // Document viewer state
  openDocument: { projectId: string; filePath: string; fileName: string } | null;
  setOpenDocument(doc: { projectId: string; filePath: string; fileName: string } | null): void;
}

export const useNavStore = create<NavState>()((set, get) => ({
  view: "home",
  previousView: "home",
  activeProjectTab: "chat",
  setActiveProjectTab: (tab) => set({ activeProjectTab: tab }),
  showHome: () => set({ view: "home", previousView: "home" }),
  showDiary: () => set({ view: "diary", previousView: "home" }),
  showSettings: () => set({ view: "settings", previousView: "home" }),
  showDeepResearch: () => set({ view: "deep-research", previousView: "home" }),
  showThemeSettings: () => {
    const current = get().view;
    set({
      view: "theme-settings",
      previousView: current !== "theme-settings" ? current : get().previousView,
    });
  },
  closeThemeSettings: () => set({ view: get().previousView }),
  showModelConfig: () => {
    const current = get().view;
    set({
      view: "model-config",
      previousView: current !== "model-config" ? current : get().previousView,
    });
  },
  closeModelConfig: () => set({ view: get().previousView }),

  openDocument: null,
  setOpenDocument: (doc) => set({ openDocument: doc }),
}));
