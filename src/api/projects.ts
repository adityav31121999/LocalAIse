import { delay, invoke, isTauri } from "./index";
import {
  listMockSources,
  MOCK_PROJECTS,
  mockChatDelete,
  mockChatList,
  mockChatRead,
} from "./mock";
import type {
  ChatMode,
  ChatSession,
  ChatSummary,
  Project,
  ProjectTemplate,
  SourceFile,
} from "../types";

export async function listProjects(): Promise<Project[]> {
  if (isTauri) return invoke<Project[]>("list_projects");
  await delay(180);
  return MOCK_PROJECTS.map((project) => ({ ...project }));
}

export async function openProject(projectId: string): Promise<Project> {
  if (isTauri) return invoke<Project>("open_project", { projectId });
  await delay(120);
  const project = MOCK_PROJECTS.find((item) => item.id === projectId);
  if (!project) throw new Error(`Unknown project: ${projectId}`);
  project.lastOpenedAt = new Date().toISOString();
  return { ...project };
}

export async function listSources(projectId: string): Promise<SourceFile[]> {
  if (isTauri) return invoke<SourceFile[]>("list_sources", { projectId });
  await delay(160);
  return listMockSources(projectId);
}

export async function reindexSources(projectId: string): Promise<SourceFile[]> {
  if (isTauri) return invoke<SourceFile[]>("reindex_sources", { projectId });
  await delay(300);
  return listMockSources(projectId).map((s) => ({ ...s, status: "indexed" }));
}

export async function setProjectInternet(projectId: string, enabled: boolean): Promise<Project> {
  if (isTauri) return invoke<Project>("set_project_internet", { projectId, enabled });
  await delay(90);
  const project = MOCK_PROJECTS.find((item) => item.id === projectId);
  if (!project) throw new Error(`Unknown project: ${projectId}`);
  project.internetEnabled = enabled;
  return { ...project };
}

/** Changes the project's display name; the folder on disk stays where it is. */
export async function renameProject(projectId: string, name: string): Promise<Project> {
  if (isTauri) return invoke<Project>("rename_project", { projectId, name });
  await delay(90);
  const trimmed = name.trim();
  if (!trimmed) throw new Error("Type a project name first.");
  const project = MOCK_PROJECTS.find((item) => item.id === projectId);
  if (!project) throw new Error(`Unknown project: ${projectId}`);
  project.name = trimmed;
  return { ...project };
}

/**
 * Unregisters a project. With `deleteFiles` the folder is removed from disk;
 * otherwise every file stays exactly where it was.
 */
export async function removeProject(projectId: string, deleteFiles: boolean): Promise<void> {
  if (isTauri) return invoke<void>("remove_project", { projectId, deleteFiles });
  await delay(80);
  const index = MOCK_PROJECTS.findIndex((item) => item.id === projectId);
  if (index >= 0) MOCK_PROJECTS.splice(index, 1);
}

/** Saves the project's chat system prompt. Null returns to the built-in default. */
export async function setProjectSystemPrompt(
  projectId: string,
  prompt: string | null,
): Promise<Project> {
  if (isTauri) return invoke<Project>("set_project_system_prompt", { projectId, prompt });
  await delay(90);
  const project = MOCK_PROJECTS.find((item) => item.id === projectId);
  if (!project) throw new Error(`Unknown project: ${projectId}`);
  project.systemPrompt = prompt;
  return { ...project };
}

/** Switches the chat between the plain assistant and the tool-using agent. */
export async function setProjectChatMode(projectId: string, mode: ChatMode): Promise<Project> {
  if (isTauri) return invoke<Project>("set_project_chat_mode", { projectId, mode });
  await delay(90);
  const project = MOCK_PROJECTS.find((item) => item.id === projectId);
  if (!project) throw new Error(`Unknown project: ${projectId}`);
  project.chatMode = mode;
  return { ...project };
}

/** Turns the project's own diary/ folder on or off. */
export async function setProjectDiary(projectId: string, enabled: boolean): Promise<Project> {
  if (isTauri) return invoke<Project>("set_project_diary", { projectId, enabled });
  await delay(90);
  const project = MOCK_PROJECTS.find((item) => item.id === projectId);
  if (!project) throw new Error(`Unknown project: ${projectId}`);
  project.diaryEnabled = enabled;
  return { ...project };
}

/** Creates a user folder inside the project directory. */
export async function createProjectFolder(projectId: string, name: string): Promise<Project> {
  if (isTauri) return invoke<Project>("create_project_folder", { projectId, name });
  await delay(120);
  const project = MOCK_PROJECTS.find((item) => item.id === projectId);
  if (!project) throw new Error(`Unknown project: ${projectId}`);
  const trimmed = name.trim();
  if (!trimmed) throw new Error("Type a folder name first.");
  if (!project.customFolders.includes(trimmed)) project.customFolders.push(trimmed);
  return { ...project };
}

/** Saved chat sessions in the project's chats/ folder, newest first. */
export async function listProjectChats(projectId: string): Promise<ChatSummary[]> {
  if (isTauri) return invoke<ChatSummary[]>("list_project_chats", { projectId });
  await delay(90);
  return mockChatList(projectId);
}

/** Reopens one saved conversation. */
export async function readProjectChat(projectId: string, chatId: string): Promise<ChatSession> {
  if (isTauri) return invoke<ChatSession>("read_project_chat", { projectId, chatId });
  await delay(90);
  return mockChatRead(projectId, chatId);
}

export async function deleteProjectChat(projectId: string, chatId: string): Promise<void> {
  if (isTauri) return invoke<void>("delete_project_chat", { projectId, chatId });
  await delay(80);
  mockChatDelete(projectId, chatId);
}

/** Adds a folder the user already has. Resolves to null when they cancel. */
export async function addProjectFolder(): Promise<Project | null> {
  if (isTauri) return invoke<Project | null>("add_project_folder");
  await delay(140);
  return null;
}

/** Creates a folder for the chosen template. Resolves to null when cancelled. */
export async function createProject(template: ProjectTemplate): Promise<Project | null> {
  if (isTauri) return invoke<Project | null>("create_project", { template });
  await delay(140);
  return null;
}
