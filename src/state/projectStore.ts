import { create } from "zustand";
import * as projectsApi from "../api/projects";
import type { Project, ProjectTemplate, SourceFile } from "../types";

import { useNavStore } from "./navStore";

type LoadStatus = "idle" | "loading" | "ready" | "error";

interface ProjectState {
  projects: Project[];
  sources: SourceFile[];
  activeProjectId: string | null;
  status: LoadStatus;
  error: string | null;

  loadProjects(): Promise<void>;
  openProject(projectId: string): Promise<void>;
  closeProject(): void;
  setInternet(enabled: boolean): Promise<void>;
  setSystemPrompt(prompt: string | null): Promise<void>;
  setDiary(enabled: boolean): Promise<void>;
  createFolder(name: string): Promise<boolean>;
  addFolder(): Promise<void>;
  createProject(template: ProjectTemplate): Promise<void>;
  reindexSources(): Promise<void>;
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function replaceProject(projects: Project[], project: Project): Project[] {
  return projects.map((item) => (item.id === project.id ? project : item));
}

function newestFirst(projects: Project[], project: Project): Project[] {
  return [project, ...projects.filter((item) => item.id !== project.id)];
}

export const useProjectStore = create<ProjectState>()((set, get) => ({
  projects: [],
  sources: [],
  activeProjectId: null,
  status: "idle",
  error: null,

  loadProjects: async () => {
    set({ status: "loading", error: null });
    try {
      set({ projects: await projectsApi.listProjects(), status: "ready" });
    } catch (error) {
      set({ status: "error", error: message(error) });
    }
  },

  openProject: async (projectId) => {
    useNavStore.getState().setActiveProjectTab("chat");
    useNavStore.getState().showHome();
    set({ activeProjectId: projectId, sources: [], error: null });
    try {
      const [project, sources] = await Promise.all([
        projectsApi.openProject(projectId),
        projectsApi.listSources(projectId),
      ]);
      set((state) => ({ projects: replaceProject(state.projects, project), sources }));
    } catch (error) {
      set({ error: message(error) });
    }
  },

  closeProject: () => {
    useNavStore.getState().setActiveProjectTab("chat");
    set({ activeProjectId: null, sources: [] });
  },

  setInternet: async (enabled) => {
    const projectId = get().activeProjectId;
    if (!projectId) return;
    try {
      const project = await projectsApi.setProjectInternet(projectId, enabled);
      set((state) => ({ projects: replaceProject(state.projects, project) }));
    } catch (error) {
      set({ error: message(error) });
    }
  },

  setSystemPrompt: async (prompt) => {
    const projectId = get().activeProjectId;
    if (!projectId) return;
    try {
      const project = await projectsApi.setProjectSystemPrompt(projectId, prompt);
      set((state) => ({ projects: replaceProject(state.projects, project) }));
    } catch (error) {
      set({ error: message(error) });
    }
  },

  /** Turns this project's own diary/ folder on or off. Pages are never deleted. */
  setDiary: async (enabled) => {
    const projectId = get().activeProjectId;
    if (!projectId) return;
    try {
      const project = await projectsApi.setProjectDiary(projectId, enabled);
      set((state) => ({ projects: replaceProject(state.projects, project), error: null }));
    } catch (error) {
      set({ error: message(error) });
    }
  },

  /** Creates a folder in the project directory; resolves false on failure. */
  createFolder: async (name) => {
    const projectId = get().activeProjectId;
    if (!projectId) return false;
    try {
      const project = await projectsApi.createProjectFolder(projectId, name);
      set((state) => ({ projects: replaceProject(state.projects, project), error: null }));
      return true;
    } catch (error) {
      set({ error: message(error) });
      return false;
    }
  },

  addFolder: async () => {
    try {
      const project = await projectsApi.addProjectFolder();
      if (!project) return;
      set((state) => ({ projects: newestFirst(state.projects, project), error: null }));
      await get().openProject(project.id);
    } catch (error) {
      set({ error: message(error) });
    }
  },

  createProject: async (template) => {
    try {
      const project = await projectsApi.createProject(template);
      if (!project) return;
      set((state) => ({ projects: newestFirst(state.projects, project), error: null }));
      await get().openProject(project.id);
    } catch (error) {
      set({ error: message(error) });
    }
  },

  reindexSources: async () => {
    const projectId = get().activeProjectId;
    if (!projectId) return;
    try {
      const updatedSources = await projectsApi.reindexSources(projectId);
      set({ sources: updatedSources });
    } catch (error) {
      set({ error: message(error) });
    }
  },
}));

export function selectActiveProject(state: ProjectState): Project | null {
  return state.projects.find((project) => project.id === state.activeProjectId) ?? null;
}
