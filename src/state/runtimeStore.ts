import { create } from "zustand";
import { delay, invoke, isTauri } from "../api/index";
import { loadModel, unloadModel } from "../api/models";
import type { RuntimeStatus } from "../types";

const FALLBACK: RuntimeStatus = {
  model: "Qwen2.5 7B Instruct · Q4_K_M",
  modelPath: null,
  state: "ready",
  port: 51234,
  profile: null,
  device: null,
  error: null,
};

interface RuntimeState {
  status: RuntimeStatus;
  load(): Promise<void>;
  /** Selects a model and starts the runtime server for it. */
  loadModel(modelPath: string): Promise<void>;
  /** Stops the running runtime server and clears the selected model. */
  unloadModel(): Promise<void>;
}

export const useRuntimeStore = create<RuntimeState>()((set) => ({
  status: {
    model: "starting…",
    modelPath: null,
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

  loadModel: async (modelPath: string) => {
    set({ status: await loadModel(modelPath) });
  },

  unloadModel: async () => {
    set({ status: await unloadModel() });
  },
}));
