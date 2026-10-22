/** Directory browsing and file operations for the project Files view. */

import { delay, invoke, isTauri } from "./index";
import {
  mockCopyEntry,
  mockCreateEntry,
  mockDeleteEntry,
  mockListDirectory,
  mockRenameEntry,
} from "./mock";
import type { DirectoryEntry } from "../types";

export async function listDirectory(
  projectId: string,
  path: string,
): Promise<DirectoryEntry[]> {
  if (isTauri) return invoke<DirectoryEntry[]>("list_project_directory", { projectId, path });
  await delay(110);
  return mockListDirectory(projectId, path);
}

export async function createEntry(
  projectId: string,
  parent: string,
  name: string,
  isDir: boolean,
): Promise<void> {
  if (isTauri) {
    return invoke<void>("create_project_entry", { projectId, parent, name, isDir });
  }
  await delay(90);
  mockCreateEntry(projectId, parent, name, isDir);
}

export async function renameEntry(
  projectId: string,
  path: string,
  newName: string,
): Promise<void> {
  if (isTauri) {
    return invoke<void>("rename_project_entry", { projectId, path, newName });
  }
  await delay(90);
  mockRenameEntry(projectId, path, newName);
}

/** Copies a file or folder next to itself, with a free "copy" name. */
export async function copyEntry(projectId: string, path: string): Promise<void> {
  if (isTauri) return invoke<void>("copy_project_entry", { projectId, path });
  await delay(90);
  mockCopyEntry(projectId, path);
}

export async function deleteEntry(projectId: string, path: string): Promise<void> {
  if (isTauri) return invoke<void>("delete_project_entry", { projectId, path });
  await delay(90);
  mockDeleteEntry(projectId, path);
}
