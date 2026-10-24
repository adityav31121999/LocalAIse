import { useEffect, useMemo, useState } from "react";
import {
  CalendarIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  PlusIcon,
  SearchIcon,
} from "../ui/icons";
import { DiaryEditor } from "../personal/DiaryEditor";
import { useDiaryStore } from "../../state/diaryStore";
import {
  addMonths,
  formatDateKey,
  monthLabel,
  monthMatrix,
  monthStart,
  toDateKey,
  todayKey,
  weekdayLabels,
} from "../../util/calendar";
import type { DiaryPage, Project } from "../../types";

const LIST_DATE: Intl.DateTimeFormatOptions = { day: "numeric", month: "short", year: "numeric" };
const LONG_DATE: Intl.DateTimeFormatOptions = {
  weekday: "long",
  day: "numeric",
  month: "long",
  year: "numeric",
};

export function ProjectCalendarView({ project }: { project: Project }) {
  const pages = useDiaryStore((state) => state.pages);
  const activePageId = useDiaryStore((state) => state.activePageId);
  const pendingDate = useDiaryStore((state) => state.pendingDate);
  const saveState = useDiaryStore((state) => state.saveState);
  const load = useDiaryStore((state) => state.load);
  const selectDate = useDiaryStore((state) => state.selectDate);
  const selectPage = useDiaryStore((state) => state.selectPage);
  const createPage = useDiaryStore((state) => state.createPage);
  const updatePage = useDiaryStore((state) => state.updatePage);
  const removePage = useDiaryStore((state) => state.removePage);

  const [month, setMonth] = useState(() => monthStart(new Date()));
  const [query, setQuery] = useState("");
  const today = todayKey();

  useEffect(() => {
    void load(project.id);
  }, [project.id, load]);

  const activePage = pages.find((p) => p.id === activePageId) ?? null;
  const selectedDate = activePage?.date ?? pendingDate ?? today;

  // Set of all dates that have an entry in this project
  const entryDates = useMemo(
    () => new Set(pages.filter((p) => p.date).map((p) => p.date as string)),
    [pages],
  );

  const datedPages = useMemo(
    () =>
      pages
        .filter((p) => p.date)
        .sort((a, b) => (b.date ?? "").localeCompare(a.date ?? "")),
    [pages],
  );

  const filteredPages = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return datedPages;
    return datedPages.filter(
      (p) =>
        (p.date && p.date.includes(q)) ||
        p.title.toLowerCase().includes(q) ||
        p.body.toLowerCase().includes(q),
    );
  }, [datedPages, query]);

  const days = monthMatrix(month);

  const handleDayClick = (key: string) => {
    selectDate(key);
  };

  const handleJumpToPageDate = (page: DiaryPage) => {
    if (page.date) {
      const d = new Date(`${page.date}T00:00:00`);
      setMonth(monthStart(d));
      selectPage(page.id);
    }
  };

  const handleOpenToday = () => {
    const now = new Date();
    setMonth(monthStart(now));
    selectDate(today);
  };

  return (
    <div
      className="project-calendar-view"
      style={{
        display: "flex",
        height: "100%",
        width: "100%",
        background: "var(--background)",
        color: "var(--text)",
        overflow: "hidden",
      }}
    >
      {/* Left Column: Calendar & Dated Pages Timeline */}
      <div
        style={{
          width: "360px",
          minWidth: "320px",
          background: "var(--background-elevated)",
          borderRight: "1px solid var(--border)",
          display: "flex",
          flexDirection: "column",
          overflowY: "auto",
        }}
      >
        {/* Calendar Card Header */}
        <div style={{ padding: "16px 16px 12px", borderBottom: "1px solid var(--border)" }}>
          <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "12px" }}>
            <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
              <span style={{ color: "var(--accent-primary)", display: "inline-flex" }}>
                <CalendarIcon size={18} />
              </span>
              <span style={{ fontWeight: "600", fontSize: "14px" }}>Project Calendar</span>
            </div>
            <button
              type="button"
              className="btn btn--subtle btn--sm"
              onClick={handleOpenToday}
              title="Jump to today"
              style={{ padding: "2px 8px", fontSize: "11px" }}
            >
              Today
            </button>
          </div>

          {/* Month Navigator */}
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              background: "var(--surface)",
              borderRadius: "var(--radius-md)",
              border: "1px solid var(--border)",
              padding: "4px 8px",
            }}
          >
            <button
              type="button"
              className="icon-btn icon-btn--sm"
              onClick={() => setMonth(addMonths(month, -1))}
              title="Previous month"
            >
              <ChevronLeftIcon size={14} />
            </button>
            <span style={{ fontWeight: "600", fontSize: "13px" }}>{monthLabel(month)}</span>
            <button
              type="button"
              className="icon-btn icon-btn--sm"
              onClick={() => setMonth(addMonths(month, 1))}
              title="Next month"
            >
              <ChevronRightIcon size={14} />
            </button>
          </div>

          {/* Monthly Day Grid */}
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(7, 1fr)",
              gap: "2px",
              marginTop: "10px",
              textAlign: "center",
            }}
          >
            {weekdayLabels().map((label) => (
              <span
                key={label}
                style={{
                  fontSize: "10.5px",
                  fontWeight: "600",
                  color: "var(--text-muted)",
                  padding: "4px 0",
                }}
              >
                {label.slice(0, 1)}
              </span>
            ))}

            {days.map((day) => {
              const key = toDateKey(day);
              const isEntry = entryDates.has(key);
              const isOutside = day.getMonth() !== month.getMonth();
              const isToday = key === today;
              const isSelected = key === selectedDate;

              return (
                <button
                  key={key}
                  type="button"
                  onClick={() => handleDayClick(key)}
                  style={{
                    position: "relative",
                    aspectRatio: "1",
                    display: "flex",
                    flexDirection: "column",
                    alignItems: "center",
                    justifyContent: "center",
                    borderRadius: "var(--radius-sm)",
                    border: isSelected
                      ? "1.5px solid var(--accent-primary)"
                      : isToday
                        ? "1px dashed var(--accent-primary)"
                        : "1px solid transparent",
                    background: isSelected
                      ? "var(--accent-primary-soft)"
                      : isEntry
                        ? "var(--surface)"
                        : "transparent",
                    color: isOutside
                      ? "var(--text-faint)"
                      : isSelected
                        ? "var(--accent-primary)"
                        : "var(--text)",
                    fontSize: "12px",
                    fontWeight: isSelected || isToday ? "600" : "normal",
                    cursor: "pointer",
                    transition: "all 100ms",
                  }}
                  title={`${formatDateKey(key, { weekday: "long", day: "numeric", month: "long" })}${
                    isEntry ? " · Linked diary entry" : ""
                  }`}
                >
                  <span>{day.getDate()}</span>
                  {isEntry && (
                    <span
                      style={{
                        position: "absolute",
                        bottom: "3px",
                        width: "4px",
                        height: "4px",
                        borderRadius: "50%",
                        background: "var(--accent-primary)",
                      }}
                    />
                  )}
                </button>
              );
            })}
          </div>

          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              gap: "12px",
              marginTop: "10px",
              fontSize: "11px",
              color: "var(--text-muted)",
            }}
          >
            <div style={{ display: "flex", alignItems: "center", gap: "4px" }}>
              <span style={{ width: "6px", height: "6px", borderRadius: "50%", background: "var(--accent-primary)" }} />
              <span>Diary page linked</span>
            </div>
            <div style={{ display: "flex", alignItems: "center", gap: "4px" }}>
              <span
                style={{
                  width: "10px",
                  height: "10px",
                  borderRadius: "2px",
                  border: "1px dashed var(--accent-primary)",
                }}
              />
              <span>Today</span>
            </div>
          </div>
        </div>

        {/* Timeline of Linked Diary Pages */}
        <div style={{ flex: 1, display: "flex", flexDirection: "column", minHeight: 0 }}>
          <div
            style={{
              padding: "10px 16px 6px",
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
            }}
          >
            <span className="muted" style={{ fontSize: "11px", fontWeight: "600", textTransform: "uppercase" }}>
              Linked Diary Pages ({datedPages.length})
            </span>
          </div>

          {/* Quick Search */}
          <div style={{ padding: "0 16px 8px" }}>
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: "6px",
                padding: "2px 8px",
                background: "var(--surface)",
                border: "1px solid var(--border)",
                borderRadius: "var(--radius-sm)",
                height: "26px",
              }}
            >
              <SearchIcon size={12} className="muted" />
              <input
                type="text"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Search entries…"
                style={{
                  background: "transparent",
                  border: "none",
                  outline: "none",
                  fontSize: "11.5px",
                  color: "var(--text)",
                  width: "100%",
                }}
              />
            </div>
          </div>

          <div style={{ flex: 1, overflowY: "auto", padding: "0 8px 12px" }}>
            {filteredPages.length === 0 ? (
              <p className="muted" style={{ padding: "16px", fontSize: "12px", textAlign: "center" }}>
                {query ? "No matching entries" : "No diary entries yet"}
              </p>
            ) : (
              filteredPages.map((page) => {
                const isSelected = activePage?.id === page.id;
                const snippet = page.body.trim().split("\n")[0] || "No content";
                return (
                  <button
                    key={page.id}
                    type="button"
                    onClick={() => handleJumpToPageDate(page)}
                    style={{
                      display: "flex",
                      flexDirection: "column",
                      width: "100%",
                      padding: "8px 10px",
                      borderRadius: "var(--radius-md)",
                      background: isSelected ? "var(--accent-primary-soft)" : "transparent",
                      border: isSelected ? "1px solid var(--accent-primary)" : "1px solid transparent",
                      textAlign: "left",
                      cursor: "pointer",
                      marginBottom: "4px",
                      transition: "background 80ms",
                    }}
                  >
                    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                      <span
                        style={{
                          fontWeight: "600",
                          fontSize: "12px",
                          color: isSelected ? "var(--accent-primary)" : "var(--text)",
                        }}
                      >
                        {page.date ? formatDateKey(page.date, LIST_DATE) : page.title || "Untitled"}
                      </span>
                      <span className="mono" style={{ fontSize: "10px", color: "var(--text-faint)" }}>
                        {page.date}
                      </span>
                    </div>
                    <span
                      className="truncate"
                      style={{ fontSize: "11.5px", color: "var(--text-secondary)", marginTop: "2px" }}
                    >
                      {snippet}
                    </span>
                  </button>
                );
              })
            )}
          </div>
        </div>
      </div>

      {/* Right Column: Linked Diary Page Editor / Viewer */}
      <div style={{ flex: 1, display: "flex", flexDirection: "column", minWidth: 0, overflowY: "auto" }}>
        {activePage ? (
          <div style={{ flex: 1, display: "flex", flexDirection: "column" }}>
            {/* Header displaying linked date badge */}
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                padding: "10px 24px",
                background: "var(--surface)",
                borderBottom: "1px solid var(--border)",
              }}
            >
              <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                <span className="badge" style={{ color: "var(--accent-primary)", background: "var(--accent-primary-soft)" }}>
                  Linked Date: {activePage.date ? formatDateKey(activePage.date, LONG_DATE) : "Custom Page"}
                </span>
                <span className="mono" style={{ fontSize: "11px", color: "var(--text-faint)" }}>
                  diary/{activePage.date ? `${activePage.date}.md` : "untitled.md"}
                </span>
              </div>
            </div>

            <div style={{ flex: 1 }}>
              <DiaryEditor
                page={activePage}
                saveState={saveState}
                onChange={(patch) => updatePage(activePage.id, patch)}
                onDelete={() => void removePage(activePage.id)}
              />
            </div>
          </div>
        ) : (
          /* Empty Day State: Prompt to create diary entry for selected date */
          <div
            style={{
              flex: 1,
              display: "flex",
              flexDirection: "column",
              alignItems: "center",
              justifyContent: "center",
              padding: "48px 24px",
              textAlign: "center",
            }}
          >
            <div
              style={{
                width: "52px",
                height: "52px",
                borderRadius: "50%",
                background: "var(--surface-2)",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                marginBottom: "16px",
                color: "var(--accent-primary)",
              }}
            >
              <CalendarIcon size={26} />
            </div>

            <h2 style={{ fontSize: "20px", fontWeight: "600", marginBottom: "6px" }}>
              {selectedDate ? formatDateKey(selectedDate, LONG_DATE) : "Select a Date"}
            </h2>

            <p style={{ fontSize: "14px", color: "var(--text-secondary)", maxWidth: "44ch", marginBottom: "24px" }}>
              There is currently no diary entry recorded for this date in project <strong>{project.name}</strong>.
              Add daily notes, milestone progress, or meeting summaries linked directly to this day.
            </p>

            <button
              type="button"
              className="btn btn--primary"
              onClick={() => void createPage(selectedDate)}
              style={{ padding: "8px 20px", fontSize: "14px" }}
            >
              <PlusIcon size={16} />
              <span>Create Diary Entry for this Date</span>
            </button>
          </div>
        )}
      </div>
    </div>
  );
}