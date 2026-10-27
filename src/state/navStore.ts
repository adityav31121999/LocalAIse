import { create } from "zustand";
import type { AppView, WorkspaceTab } from "../types";

type SettingsTab = "appearance" | "model-runtime";

interface NavState {
  view: AppView;
  previousView: AppView;
  activeProjectTab: WorkspaceTab;
  settingsTab: SettingsTab;
  setActiveProjectTab(tab: WorkspaceTab): void;
  showHome(): void;
  showDiary(): void;
  showSettings(tab?: SettingsTab): void;
  showDeepResearch(): void;

  // Document viewer state
  openDocument: { projectId: string; filePath: string; fileName: string } | null;
  setOpenDocument(doc: { projectId: string; filePath: string; fileName: string } | null): void;

  // A passage queued for the chat composer (e.g. "Cite in Chat" from the
  // document viewer). The composer appends it and clears the slot.
  composerInsert: string | null;
  insertIntoComposer(text: string): void;
  consumeComposerInsert(): void;
}

export const useNavStore = create<NavState>()((set) => ({
  view: "home",
  previousView: "home",
  activeProjectTab: "chat",
  settingsTab: "appearance",
  setActiveProjectTab: (tab) => set({ activeProjectTab: tab }),
  showHome: () => set({ view: "home", previousView: "home" }),
  showDiary: () => set({ view: "diary", previousView: "home" }),
  showSettings: (tab = "appearance") => set({ view: "settings", previousView: "home", settingsTab: tab }),
  showDeepResearch: () => set({ view: "deep-research", previousView: "home" }),

  openDocument: null,
  setOpenDocument: (doc) => set({ openDocument: doc }),

  composerInsert: null,
  insertIntoComposer: (text) => set({ composerInsert: text }),
  consumeComposerInsert: () => set({ composerInsert: null }),
}));
