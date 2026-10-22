import { create } from "zustand";
import { delay, invoke, isTauri } from "../api/index";
import type { RuntimeStatus } from "../types";

const FALLBACK: RuntimeStatus = {
  model: "Qwen2.5 7B Instruct · Q4_K_M",
  state: "ready",
  port: 51234,
  profile: null,
  device: null,
  error: null,
};

interface RuntimeState {
  status: RuntimeStatus;
  load(): Promise<void>;
}

export const useRuntimeStore = create<RuntimeState>()((set) => ({
  status: {
    model: "starting…",
    state: "starting",
    port: null,
    profile: null,
    device: null,
    error: null,
  },

  load: async () => {
    if (isTauri) {
      set({ status: await invoke<RuntimeStatus>("runtime_status") });
      return;
    }
    await delay(320);
    set({ status: FALLBACK });
  },
}));
