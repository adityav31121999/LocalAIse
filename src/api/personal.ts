import { delay, invoke, isTauri } from "./index";
import { toDateKey } from "../util/calendar";
import type { DiaryPage, PersonalFolderRecord } from "../types";

/** Diary entries live in the personal space folder as one Markdown file per day. */

const STORAGE_KEY = "LocalAIse.diary";
export const PERSONAL_FOLDERS_STORAGE_KEY = "LocalAIse.database.personal_folders";

/**
 * Default hidden folder for personal space calendar pages.
 * Recorded in the database with isHidden: true and isDefault: true.
 */
export const DEFAULT_PERSONAL_CALENDAR_FOLDER: PersonalFolderRecord = {
  id: "folder-personal-calendar-default",
  name: ".calendar",
  path: ".calendar",
  isDir: true,
  isHidden: true,
  isDefault: true,
  type: "calendar_pages",
  description: "Default hidden database folder storing all Personal Space calendar & diary pages",
  createdAt: "2026-01-01T00:00:00.000Z",
  updatedAt: "2026-01-01T00:00:00.000Z",
};

export function getPersonalDatabaseFolders(): PersonalFolderRecord[] {
  try {
    const raw = localStorage.getItem(PERSONAL_FOLDERS_STORAGE_KEY);
    if (!raw) {
      const initial = [DEFAULT_PERSONAL_CALENDAR_FOLDER];
      localStorage.setItem(PERSONAL_FOLDERS_STORAGE_KEY, JSON.stringify(initial));
      return initial;
    }
    const folders = JSON.parse(raw) as PersonalFolderRecord[];
    if (!folders.some((f) => f.id === DEFAULT_PERSONAL_CALENDAR_FOLDER.id || f.name === ".calendar")) {
      folders.unshift(DEFAULT_PERSONAL_CALENDAR_FOLDER);
      localStorage.setItem(PERSONAL_FOLDERS_STORAGE_KEY, JSON.stringify(folders));
    }
    return folders;
  } catch {
    return [DEFAULT_PERSONAL_CALENDAR_FOLDER];
  }
}

export function getPersonalCalendarFolder(): PersonalFolderRecord {
  const folders = getPersonalDatabaseFolders();
  return (
    folders.find((f) => f.id === DEFAULT_PERSONAL_CALENDAR_FOLDER.id || f.name === ".calendar") ??
    DEFAULT_PERSONAL_CALENDAR_FOLDER
  );
}

function newId(): string {
  return Math.random().toString(36).slice(2, 10);
}

function keyDaysAgo(days: number): string {
  const date = new Date();
  date.setDate(date.getDate() - days);
  return toDateKey(date);
}

function seed(): DiaryPage[] {
  // Ensure the hidden folder is recorded on database
  getPersonalDatabaseFolders();

  const entries: Array<[number, string]> = [
    [0, "Quiet morning. Sketched the outline for the retrieval pass, then wrote down the three things\nthat still feel unresolved about chunk sizes."],
    [1, "Spent the afternoon on the budget review. Two invoices still need a category before the\nmonth closes — flagged them in the sources panel."],
    [3, "Long walk, no laptop. Came back convinced the sidebar should show sources and diary pages\nwith the same row rhythm."],
    [9, "First pass at the theme tokens. Dark graphite reads well; the warm paper surface is the\npiece that makes the diary feel different from the chat."],
    [17, "Started keeping this diary. The rule is simple: one page per day, plain Markdown, and nothing\nleaves this machine."],
  ];

  const now = new Date().toISOString();

  const dated: DiaryPage[] = entries.map(([days, body], index) => {
    const dKey = keyDaysAgo(days);
    return {
      id: `seed-${index}`,
      date: dKey,
      title: "",
      body,
      createdAt: now,
      updatedAt: now,
      folderId: DEFAULT_PERSONAL_CALENDAR_FOLDER.id,
      folderName: DEFAULT_PERSONAL_CALENDAR_FOLDER.name,
      filePath: `${DEFAULT_PERSONAL_CALENDAR_FOLDER.name}/${dKey}.md`,
      isHidden: true,
    };
  });

  const free: DiaryPage = {
    id: "seed-reading",
    date: null,
    title: "Reading list",
    body: "- Attention is all you need\n- On-device inference notes\n- Privacy budget primer\n",
    createdAt: now,
    updatedAt: now,
    folderId: DEFAULT_PERSONAL_CALENDAR_FOLDER.id,
    folderName: DEFAULT_PERSONAL_CALENDAR_FOLDER.name,
    filePath: `${DEFAULT_PERSONAL_CALENDAR_FOLDER.name}/reading-list.md`,
    isHidden: true,
  };

  return [free, ...dated];
}

function storageKey(projectId: string | null): string {
  return projectId ? `${STORAGE_KEY}:${projectId}` : STORAGE_KEY;
}

function read(projectId: string | null): DiaryPage[] {
  try {
    const raw = localStorage.getItem(storageKey(projectId));
    if (!raw) {
      const pages = projectId ? [] : seed();
      write(projectId, pages);
      return pages;
    }
    const parsed = JSON.parse(raw) as DiaryPage[];
    if (!projectId) {
      // Ensure hidden folder record exists in DB and all personal pages are bound to it
      getPersonalDatabaseFolders();
      return parsed.map((page) => ({
        ...page,
        folderId: page.folderId || DEFAULT_PERSONAL_CALENDAR_FOLDER.id,
        folderName: page.folderName || DEFAULT_PERSONAL_CALENDAR_FOLDER.name,
        filePath: page.filePath || `${DEFAULT_PERSONAL_CALENDAR_FOLDER.name}/${page.date || page.id}.md`,
        isHidden: true,
      }));
    }
    return parsed;
  } catch {
    return [];
  }
}

function write(projectId: string | null, pages: DiaryPage[]): void {
  try {
    localStorage.setItem(storageKey(projectId), JSON.stringify(pages));
  } catch {
    // Storage can be unavailable; the in-memory copy still works for this session.
  }
}

export async function listDiaryPages(projectId: string | null = null): Promise<DiaryPage[]> {
  if (isTauri) return invoke<DiaryPage[]>("list_diary_pages", { projectId });
  await delay(160);
  return read(projectId);
}

export async function createDiaryPage(
  input: {
    date: string | null;
    title: string;
  },
  projectId: string | null = null,
): Promise<DiaryPage> {
  if (isTauri) return invoke<DiaryPage>("create_diary_page", { projectId, ...input });
  await delay(80);
  const now = new Date().toISOString();
  const page: DiaryPage = {
    id: newId(),
    date: input.date,
    title: input.title,
    body: "",
    createdAt: now,
    updatedAt: now,
    folderId: projectId ? undefined : DEFAULT_PERSONAL_CALENDAR_FOLDER.id,
    folderName: projectId ? undefined : DEFAULT_PERSONAL_CALENDAR_FOLDER.name,
    filePath: projectId
      ? undefined
      : `${DEFAULT_PERSONAL_CALENDAR_FOLDER.name}/${input.date || "page"}.md`,
    isHidden: projectId ? undefined : true,
  };
  write(projectId, [page, ...read(projectId)]);
  return page;
}

export async function saveDiaryPage(
  page: DiaryPage,
  projectId: string | null = null,
): Promise<DiaryPage> {
  if (isTauri) return invoke<DiaryPage>("save_diary_page", { projectId, page });
  await delay(180);
  const saved: DiaryPage = { ...page, updatedAt: new Date().toISOString() };
  write(projectId, read(projectId).map((item) => (item.id === saved.id ? saved : item)));
  return saved;
}

export async function deleteDiaryPage(
  pageId: string,
  projectId: string | null = null,
): Promise<void> {
  if (isTauri) return invoke<void>("delete_diary_page", { projectId, pageId });
  await delay(80);
  write(projectId, read(projectId).filter((page) => page.id !== pageId));
}

export async function listCalendarEvents(): Promise<import("../types").CalendarEvent[]> {
  if (isTauri) return invoke<import("../types").CalendarEvent[]>("list_calendar_events");
  await delay(120);
  return [];
}

export async function createCalendarEvent(event: import("../types").CalendarEvent): Promise<import("../types").CalendarEvent> {
  if (isTauri) return invoke<import("../types").CalendarEvent>("create_calendar_event", { event });
  await delay(100);
  return event;
}

export async function updateCalendarEvent(event: import("../types").CalendarEvent): Promise<import("../types").CalendarEvent> {
  if (isTauri) return invoke<import("../types").CalendarEvent>("update_calendar_event", { event });
  await delay(100);
  return event;
}

export async function deleteCalendarEvent(eventId: string): Promise<void> {
  if (isTauri) return invoke<void>("delete_calendar_event", { eventId });
  await delay(80);
}

export async function exportCalendarIcs(): Promise<string> {
  if (isTauri) return invoke<string>("export_calendar_ics");
  await delay(80);
  return "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nEND:VCALENDAR\r\n";
}

export async function importCalendarIcs(icsContent: string): Promise<number> {
  if (isTauri) return invoke<number>("import_calendar_ics", { icsContent });
  await delay(150);
  return 1;
}

export async function searchPersonal(query: string): Promise<import("../types").PersonalSearchResult[]> {
  if (isTauri) return invoke<import("../types").PersonalSearchResult[]>("search_personal", { query });
  await delay(120);
  return [];
}
