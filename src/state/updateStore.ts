import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface UpdateInfo {
  version: string;
  body: string;
  date: string;
}

export interface DownloadProgress {
  progress: number; // 0-100
  downloaded: number;
  total: number;
}

interface UpdateState {
  // Update availability
  updateAvailable: UpdateInfo | null;
  // Download state
  downloading: boolean;
  progress: DownloadProgress | null;
  // Error state
  error: string | null;
  // Last check timestamp (epoch seconds)
  lastCheck: number | null;
  // Actions
  checkForUpdate: () => Promise<void>;
  startDownload: () => Promise<void>;
  skipVersion: () => Promise<void>;
  dismiss: () => void;
  setLastCheck: (ts: number) => void;
  clearError: () => void;
}

export const useUpdateStore = create<UpdateState>((set, get) => ({
  updateAvailable: null,
  downloading: false,
  progress: null,
  error: null,
  lastCheck: null,

  checkForUpdate: async () => {
    set({ error: null });
    try {
      const result = await invoke<UpdateInfo | null>("check_now");
      if (result) {
        set({ updateAvailable: result });
      } else {
        set({ error: "No update available — you're on the latest version." });
        // Refresh last check time
        const last = await invoke<number>("get_last_check");
        set({ lastCheck: last });
      }
    } catch (e) {
      set({ error: e instanceof Error ? e.message : "Update check failed" });
    }
  },

  startDownload: async () => {
    const { updateAvailable } = get();
    if (!updateAvailable) return;

    set({ downloading: true, progress: { progress: 0, downloaded: 0, total: 0 }, error: null });

    try {
      // Listen for progress events
      let _unlisten: (() => void) | null = null;
      await listen<DownloadProgress>("download-progress", (event) => {
        set({ progress: event.payload });
      }).then((fn) => { _unlisten = fn; });
      // _unlisten is intentionally unused - listener stays active until component unmounts
      void _unlisten;

      await invoke("install_update");

      // If we get here, install succeeded and app will restart
      set({ progress: { progress: 100, downloaded: 0, total: 0 } });
    } catch (e) {
      set({ error: e instanceof Error ? e.message : "Download or install failed", downloading: false });
    }
  },

  skipVersion: async () => {
    const { updateAvailable } = get();
    if (!updateAvailable) return;

    try {
      await invoke("skip_version", { version: updateAvailable.version });
      set({ updateAvailable: null });
    } catch (e) {
      set({ error: e instanceof Error ? e.message : "Failed to skip version" });
    }
  },

  dismiss: () => {
    set({ updateAvailable: null, error: null, progress: null, downloading: false });
  },

  setLastCheck: (ts: number) => {
    set({ lastCheck: ts });
  },

  clearError: () => {
    set({ error: null });
  },
}));

// Initialize: listen for update-available event from backend
let initialized = false;
export function initUpdateStore() {
  if (initialized) return;
  initialized = true;

  let _unlisten: (() => void) | null = null;
  listen<UpdateInfo>("update-available", (event) => {
    useUpdateStore.getState().setLastCheck(Date.now() / 1000);
    useUpdateStore.setState({ updateAvailable: event.payload });
  }).then((fn) => { _unlisten = fn; });
  // _unlisten is intentionally unused - listener stays active for app lifetime
  void _unlisten;
}