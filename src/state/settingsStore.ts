import { create } from "zustand";
import type { Density, ThemeMode, ThemeName } from "../types";

const STORAGE_KEY = "LocalAIse.settings";

/** Lazily load the Tauri IPC bridge so `@tauri-apps/api/core` isn't bundled
 * statically into the settings store's chunk. All callers below are async,
 * so the dynamic import fits naturally and lets Vite split `core` into its
 * own async chunk. */
async function tauriInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

/** Theme, color mode and density are independent settings. */
interface PersistedSettings {
  colorMode: ThemeMode;
  themeName: ThemeName;
  density: Density;
  sidebarCollapsed: boolean;
  sidebarWidth: number;
  railCollapsed: boolean;
  onboardingComplete: boolean;
}

const DEFAULTS: PersistedSettings = {
  colorMode: "system",
  themeName: "black-and-white",
  density: "comfortable",
  sidebarCollapsed: false,
  sidebarWidth: 220,
  railCollapsed: false,
  onboardingComplete: false,
};

export const THEME_ORDER: ThemeName[] = [
  "atelier",
  "paper",
  "cyber-lab",
  "amethyst",
  "arctic",
  "evergreen",
  "black-and-white",
];

export const MODE_ORDER: ThemeMode[] = ["system", "dark", "light"];
export const DENSITIES: Density[] = ["compact", "comfortable", "spacious"];

function isColorMode(value: unknown): value is ThemeMode {
  return value === "light" || value === "dark" || value === "system";
}

function isThemeName(value: unknown): value is ThemeName {
  return THEME_ORDER.includes(value as ThemeName);
}

function isDensity(value: unknown): value is Density {
  return DENSITIES.includes(value as Density);
}

function readPersisted(): PersistedSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULTS;
    const parsed = JSON.parse(raw) as Partial<PersistedSettings>;
    return {
      colorMode: isColorMode(parsed.colorMode) ? parsed.colorMode : DEFAULTS.colorMode,
      themeName: isThemeName(parsed.themeName) ? parsed.themeName : DEFAULTS.themeName,
      density: isDensity(parsed.density) ? parsed.density : DEFAULTS.density,
      sidebarCollapsed: Boolean(parsed.sidebarCollapsed),
      sidebarWidth: typeof parsed.sidebarWidth === "number" && parsed.sidebarWidth > 0
        ? parsed.sidebarWidth
        : DEFAULTS.sidebarWidth,
      railCollapsed: Boolean(parsed.railCollapsed),
      onboardingComplete: Boolean(parsed.onboardingComplete),
    };
  } catch {
    return DEFAULTS;
  }
}

function write(value: PersistedSettings): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
  } catch {
    // Storage can be unavailable; the in-memory value still applies.
  }
}

interface SettingsState extends PersistedSettings {
  setColorMode(mode: ThemeMode): void;
  cycleColorMode(): void;
  setThemeName(name: ThemeName): void;
  setDensity(density: Density): void;
  toggleSidebar(): void;
  setSidebarCollapsed(collapsed: boolean): void;
  setSidebarWidth(width: number): void;
  toggleRail(): void;
  setRailCollapsed(collapsed: boolean): void;
  completeOnboarding(): void;
  // Updater settings (synced with backend)
  getAutoUpdateCheck(): Promise<boolean>;
  setAutoUpdateCheck(enabled: boolean): Promise<void>;
  getLastCheck(): Promise<number>;
}

export const useSettingsStore = create<SettingsState>()((set, get) => {
  const persist = () => {
    const { colorMode, themeName, density, sidebarCollapsed, sidebarWidth, railCollapsed, onboardingComplete } = get();
    write({ colorMode, themeName, density, sidebarCollapsed, sidebarWidth, railCollapsed, onboardingComplete });
  };

  return {
    ...readPersisted(),

    setColorMode: (colorMode) => {
      set({ colorMode });
      persist();
    },

    cycleColorMode: () => {
      const next = MODE_ORDER[(MODE_ORDER.indexOf(get().colorMode) + 1) % MODE_ORDER.length];
      get().setColorMode(next);
    },

    setThemeName: (themeName) => {
      set({ themeName });
      persist();
    },

    setDensity: (density) => {
      set({ density });
      persist();
    },

    toggleSidebar: () => {
      set({ sidebarCollapsed: !get().sidebarCollapsed });
      persist();
    },

    // Used by the document viewer to get out of the way while a document is
    // open, then restore the user's preference when it closes. Not persisted
    // separately — it writes the same preference the toggle does.
    setSidebarCollapsed: (sidebarCollapsed) => {
      if (get().sidebarCollapsed === sidebarCollapsed) return;
      set({ sidebarCollapsed });
      persist();
    },

    setSidebarWidth: (width) => {
      set({ sidebarWidth: width });
      persist();
    },

    toggleRail: () => {
      set({ railCollapsed: !get().railCollapsed });
      persist();
    },

    setRailCollapsed: (railCollapsed) => {
      if (get().railCollapsed === railCollapsed) return;
      set({ railCollapsed });
      persist();
    },

    completeOnboarding: () => {
      set({ onboardingComplete: true });
      persist();
    },

    // Updater settings - delegated to backend
    getAutoUpdateCheck: async () => {
      return await tauriInvoke<boolean>("get_auto_update_check");
    },

    setAutoUpdateCheck: async (enabled: boolean) => {
      await tauriInvoke("set_auto_update_check", { enabled });
    },

    getLastCheck: async () => {
      return await tauriInvoke<number>("get_last_check");
    },
  };
});
