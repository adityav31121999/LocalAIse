import type { ProjectTemplate, RuntimeState } from "../types";

export function formatRelativeTime(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "unknown";
  const diff = Date.now() - then;
  const minutes = Math.round(diff / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} hr ago`;
  const days = Math.round(hours / 24);
  if (days === 1) return "yesterday";
  if (days < 30) return `${days} days ago`;
  const months = Math.round(days / 30);
  return months === 1 ? "last month" : `${months} months ago`;
}

export function formatClock(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  const time = date.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  const sameDay = new Date().toDateString() === date.toDateString();
  if (sameDay) return `Today, ${time}`;
  return `${date.toLocaleDateString(undefined, { month: "short", day: "numeric" })}, ${time}`;
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${Math.round(kb)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb.toFixed(1)} MB`;
  return `${(mb / 1024).toFixed(1)} GB`;
}

export function templateLabel(template: ProjectTemplate): string {
  switch (template) {
    case "budget":
      return "Budget";
    case "content-writing":
      return "Content writing";
    case "research":
      return "Research";
    case "general":
      return "General";
  }
}

export function templateBadgeClass(template: ProjectTemplate): string {
  switch (template) {
    case "budget":
      return "badge badge--budget";
    case "content-writing":
      return "badge badge--content";
    case "research":
      return "badge badge--research";
    case "general":
      return "badge badge--general";
  }
}

export function fileExtension(name: string): string {
  const dot = name.lastIndexOf(".");
  if (dot < 0) return "file";
  return name.slice(dot + 1).toLowerCase();
}

export function baseName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

/**
 * The label a saved chat shows in the picker, taken from its file name:
 * `chat-001` becomes `Chat 1`. Older timestamp-named files have no number, so
 * they fall back to their saved title. Anything else is shown as-is.
 */
export function chatLabel(id: string, fallback = ""): string {
  const match = /^chat-(\d+)$/i.exec(id);
  if (!match) return fallback || id;
  const number = Number.parseInt(match[1], 10);
  return Number.isFinite(number) ? `Chat ${number}` : fallback || id;
}

/** The numeric part of a `chat-001` id, for sorting; unknown ids go last. */
export function chatNumber(id: string): number {
  const match = /^chat-(\d+)$/i.exec(id);
  return match ? Number.parseInt(match[1], 10) : Number.MAX_SAFE_INTEGER;
}

export function runtimeLabel(state: RuntimeState): string {
  switch (state) {
    case "starting":
      return "Starting";
    case "ready":
      return "Ready";
    case "stopped":
      return "Stopped";
    case "error":
      return "Error";
  }
}

export function runtimeDotClass(state: RuntimeState): string {
  switch (state) {
    case "starting":
      return "dot dot--ai dot--pulse";
    case "ready":
      return "dot dot--success";
    case "stopped":
      return "dot dot--idle";
    case "error":
      return "dot dot--attention";
  }
}
