/** Calendar helpers for the diary. Dates are handled as local "YYYY-MM-DD" keys. */

const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

export function toDateKey(date: Date): string {
  const month = `${date.getMonth() + 1}`.padStart(2, "0");
  const day = `${date.getDate()}`.padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

export function todayKey(): string {
  return toDateKey(new Date());
}

export function fromDateKey(key: string): Date | null {
  const parts = key.split("-").map(Number);
  if (parts.length !== 3 || parts.some((part) => Number.isNaN(part))) return null;
  return new Date(parts[0], parts[1] - 1, parts[2]);
}

export function monthStart(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), 1);
}

export function addMonths(date: Date, delta: number): Date {
  return new Date(date.getFullYear(), date.getMonth() + delta, 1);
}

/** Six Monday-first weeks covering the given month. */
export function monthMatrix(month: Date): Date[] {
  const first = monthStart(month);
  const offset = (first.getDay() + 6) % 7;
  const start = new Date(first);
  start.setDate(first.getDate() - offset);

  return Array.from({ length: 42 }, (_, index) => {
    const day = new Date(start);
    day.setDate(start.getDate() + index);
    return day;
  });
}

export function weekdayLabels(): string[] {
  return WEEKDAYS;
}

export function monthLabel(month: Date): string {
  return month.toLocaleDateString(undefined, { month: "long", year: "numeric" });
}

export function formatDateKey(key: string, options: Intl.DateTimeFormatOptions): string {
  const date = fromDateKey(key);
  return date ? date.toLocaleDateString(undefined, options) : key;
}
