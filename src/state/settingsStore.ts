import { create } from "zustand";
import type { Density, ThemeMode, ThemeName } from "../types";

const STORAGE_KEY = "LocalAIse.settings";

/** Theme, color mode and density are independent settings. */
interface PersistedSettings {
  colorMode: ThemeMode;
  themeName: ThemeName;
  density: Density;
  sidebarCollapsed: boolean;
  onboardingComplete: boolean;
}

const DEFAULTS: PersistedSettings = {
  colorMode: "system",
  themeName: "black-and-white",
  density: "comfortable",
  sidebarCollapsed: false,
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
  completeOnboarding(): void;
}

export const useSettingsStore = create<SettingsState>()((set, get) => {
  const persist = () => {
    const { colorMode, themeName, density, sidebarCollapsed, onboardingComplete } = get();
    write({ colorMode, themeName, density, sidebarCollapsed, onboardingComplete });
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

    completeOnboarding: () => {
      set({ onboardingComplete: true });
      persist();
    },
  };
});
