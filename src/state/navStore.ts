import { create } from "zustand";
import type { AppView } from "../types";

interface NavState {
  view: AppView;
  showHome(): void;
  showDiary(): void;
  showSettings(): void;
}

export const useNavStore = create<NavState>()((set) => ({
  view: "home",
  showHome: () => set({ view: "home" }),
  showDiary: () => set({ view: "diary" }),
  showSettings: () => set({ view: "settings" }),
}));
