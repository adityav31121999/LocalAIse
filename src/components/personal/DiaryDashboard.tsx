import { useEffect, useMemo, useState } from "react";
import { BookIcon, ChevronDownIcon, ChevronLeftIcon, ChevronRightIcon, FilePlusIcon, FolderIcon, PanelLeftIcon, PlusIcon, ShieldIcon } from "../ui/icons";
import { CalendarView } from "./CalendarView";
import { Reminders } from "./Reminders";
import { DiaryEditor } from "./DiaryEditor";
import { WorkspaceLayout } from "../layout/WorkspaceLayout";
import { useDiaryStore } from "../../state/diaryStore";
import { useNavStore } from "../../state/navStore";
import { useProjectStore, selectActiveProject } from "../../state/projectStore";
import { useSettingsStore } from "../../state/settingsStore";
import { dateKeyToDirName, formatDateKey, monthStart, todayKey } from "../../util/calendar";
import type { DiaryPage } from "../../types";

const LONG_DATE: Intl.DateTimeFormatOptions = {
  weekday: "long",
  day: "numeric",
  month: "long",
  year: "numeric",
};

function pageFileName(page: DiaryPage): string {
  if (page.fileName) return page.fileName;
  if (page.filePath) {
    const base = page.filePath.split("/").pop();
    if (base) return base;
  }
  if (page.date && page.nameLocked) return `${dateKeyToDirName(page.date)}.md`;
  const slug =
    page.title.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "") || "page";
  return `${slug}.md`;
}

function PageRow({
  page,
  selected,
  onSelect,
}: {
  page: DiaryPage;
  selected: boolean;
  onSelect(): void;
}) {
  return (
    <button
      type="button"
      className={`diary-row${selected ? " diary-row--active" : ""}`}
      aria-current={selected ? "true" : undefined}
      title={pageFileName(page)}
      onClick={onSelect}
    >
      <span className="diary-row__label truncate">
        {page.nameLocked ? <ShieldIcon size={11} /> : null}
        {pageFileName(page)}
      </span>
    </button>
  );
}

export function DiaryDashboard() {
  const pages = useDiaryStore((state) => state.pages);
  const activePageId = useDiaryStore((state) => state.activePageId);
  const pendingDate = useDiaryStore((state) => state.pendingDate);
  const status = useDiaryStore((state) => state.status);
  const saveState = useDiaryStore((state) => state.saveState);
  const load = useDiaryStore((state) => state.load);
  const selectDate = useDiaryStore((state) => state.selectDate);
  const selectPage = useDiaryStore((state) => state.selectPage);
  const createPage = useDiaryStore((state) => state.createPage);
  const updatePage = useDiaryStore((state) => state.updatePage);
  const removePage = useDiaryStore((state) => state.removePage);

  const sidebarCollapsed = useSettingsStore((state) => state.sidebarCollapsed);
  const toggleSidebar = useSettingsStore((state) => state.toggleSidebar);
  const showHome = useNavStore((state) => state.showHome);
  const project = useProjectStore(selectActiveProject);

  // A project with its diary turned on owns the page; otherwise this view is
  // the personal space diary, exactly as before.
  const scopeProjectId = project?.diaryEnabled ? project.id : null;

  const [month, setMonth] = useState(() => monthStart(new Date()));
  const [collapsedDates, setCollapsedDates] = useState<Set<string>>(new Set());
  const today = todayKey();

  const toggleDateGroup = (date: string) =>
    setCollapsedDates((current) => {
      const next = new Set(current);
      if (next.has(date)) next.delete(date);
      else next.add(date);
      return next;
    });

  useEffect(() => {
    void load(scopeProjectId);
  }, [load, scopeProjectId]);

  const activePage = pages.find((page) => page.id === activePageId) ?? null;
  const selectedDate = activePage?.date ?? pendingDate;

  const entryDates = useMemo(
    () => new Set(pages.filter((page) => page.date).map((page) => page.date as string)),
    [pages],
  );
  const dateGroups = useMemo(() => {
    const map = new Map<string, DiaryPage[]>();
    for (const page of pages) {
      if (!page.date) continue;
      const list = map.get(page.date) ?? [];
      list.push(page);
      map.set(page.date, list);
    }
    return [...map.entries()]
      .sort(([a], [b]) => b.localeCompare(a))
      .map(([date, items]) => ({
        date,
        items: [...items].sort(
          (a, b) =>
            Number(Boolean(b.nameLocked)) - Number(Boolean(a.nameLocked)) ||
            a.createdAt.localeCompare(b.createdAt),
        ),
      }));
  }, [pages]);
  const extra = useMemo(
    () =>
      pages
        .filter((page) => !page.date)
        .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)),
    [pages],
  );

  const openToday = () => {
    setMonth(monthStart(new Date()));
    selectDate(today);
  };

  return (
    <WorkspaceLayout
      sidebarCollapsed={sidebarCollapsed}
      header={
        <header className="project-header">
          <button
            type="button"
            className="icon-btn icon-btn--sm"
            onClick={showHome}
            title="Work directories"
          >
            <ChevronLeftIcon size={16} />
            <span className="sr-only">Back to work directories</span>
          </button>

          <button
            type="button"
            className="icon-btn icon-btn--sm"
            onClick={toggleSidebar}
            aria-pressed={!sidebarCollapsed}
            title={sidebarCollapsed ? "Show diary panel" : "Hide diary panel"}
          >
            <PanelLeftIcon size={16} />
            <span className="sr-only">Toggle diary panel</span>
          </button>

          <div className="project-header__identity">
            <div className="project-header__name">
              <BookIcon size={14} />
              <span className="truncate">Personal Calendar & Diary</span>
              <span className="badge">{pages.length} pages</span>
              {!scopeProjectId && (
                <span
                  className="badge"
                  style={{
                    fontFamily: "var(--font-mono)",
                    fontSize: "10.5px",
                    background: "var(--surface-2)",
                    color: "var(--text-muted)",
                  }}
                  title="Recorded on database: default hidden folder storing all personal calendar pages"
                >
                  📁 .calendar (hidden)
                </span>
              )}
            </div>
            <div className="project-header__path truncate">
              {scopeProjectId
                ? `${project?.name} · diary/ folder in the project · one page per day`
                : "Personal space · All pages stored in default hidden database folder: .calendar"}
            </div>
          </div>

          <span className="project-header__spacer" />

          <button type="button" className="btn btn--secondary btn--sm" onClick={openToday}>
            Today
          </button>
          <button
            type="button"
            className="btn btn--primary btn--sm"
            onClick={() => void createPage(null)}
          >
            <PlusIcon size={14} />
            Add page
          </button>
        </header>
      }
      sidebar={
        <div className="panel">
          <Reminders selectedDate={selectedDate} />

          <div className="panel__foot">
            {scopeProjectId
              ? "Stored in this project's diary/ folder as Markdown. Nothing here is uploaded."
              : "Stored in your personal space. Nothing here is uploaded."}
          </div>
        </div>
      }
    >
      <div className="diary-split">
        <div className="diary-split__left">
          <aside className="diary-split__cal" aria-label="Calendar">
            <CalendarView
              month={month}
              today={today}
              selectedDate={selectedDate}
              entryDates={entryDates}
              onMonthChange={setMonth}
              onSelectDate={selectDate}
            />
          </aside>

          <div className="diary-split__pages" aria-label="Pages">
            <div className="panel__head">
              <span className="panel__title">Pages · {pages.length}</span>
              <button
                type="button"
                className="icon-btn icon-btn--sm"
                onClick={() => void createPage(null)}
                title="Add a page"
              >
                <PlusIcon size={14} />
                <span className="sr-only">Add a page</span>
              </button>
            </div>
            <div className="panel__scroll">
              {status === "loading" && pages.length === 0 ? (
                <p className="muted mt-2">
                  <span className="spinner" aria-hidden="true" /> Opening your diary…
                </p>
              ) : pages.length === 0 ? (
                <p className="muted mt-2">No pages yet. Pick a date, or add a page.</p>
              ) : (
                <>
                  {dateGroups.length > 0 ? <div className="panel__group">Entries</div> : null}
                  {dateGroups.map((group) => {
                    const collapsed = collapsedDates.has(group.date);
                    return (
                      <div key={group.date} className="tree-date-group">
                        <div className="tree-date">
                          <button
                            type="button"
                            className="tree-date__toggle"
                            aria-expanded={!collapsed}
                            onClick={() => toggleDateGroup(group.date)}
                            title={group.date}
                          >
                            {collapsed ? (
                              <ChevronRightIcon size={12} />
                            ) : (
                              <ChevronDownIcon size={12} />
                            )}
                            <FolderIcon size={14} />
                            <span className="truncate">{dateKeyToDirName(group.date)}</span>
                            <span className="tree-date__count">{group.items.length}</span>
                          </button>
                          <button
                            type="button"
                            className="icon-btn icon-btn--sm"
                            onClick={() => void createPage(group.date, false)}
                            title="Add a file for this date"
                          >
                            <FilePlusIcon size={14} />
                            <span className="sr-only">Add a file for this date</span>
                          </button>
                        </div>
                        {!collapsed &&
                          group.items.map((page) => (
                            <div key={page.id} className="tree-date__children">
                              <PageRow
                                page={page}
                                selected={page.id === activePageId}
                                onSelect={() => selectPage(page.id)}
                              />
                            </div>
                          ))}
                      </div>
                    );
                  })}
                  {extra.length > 0 ? <div className="panel__group">Extra pages</div> : null}
                  {extra.map((page) => (
                    <PageRow
                      key={page.id}
                      page={page}
                      selected={page.id === activePageId}
                      onSelect={() => selectPage(page.id)}
                    />
                  ))}
                </>
              )}
            </div>
          </div>
        </div>
        <div className="diary-split__doc">
          {activePage ? (
            <DiaryEditor
              page={activePage}
              saveState={saveState}
              onChange={(patch) => updatePage(activePage.id, patch)}
              onDelete={() => void removePage(activePage.id)}
            />
          ) : (
        <div className="diary">
          <div className="diary__sheet diary__sheet--empty">
            {pendingDate ? (
              <>
                <h2 className="diary__date">{formatDateKey(pendingDate, LONG_DATE)}</h2>
                <p className="muted">There is no entry for this day yet.</p>
                <button
                  type="button"
                  className="btn btn--primary"
                  onClick={() => void createPage(pendingDate, true)}
                >
                  <PlusIcon size={15} />
                  Start this entry
                </button>
              </>
            ) : (
              <>
                <BookIcon size={22} />
                <h2 className="diary__date">Your diary</h2>
                <p className="muted">
                  Pick a date in the calendar for a daily entry, or add a page for anything that is
                  not tied to a day — a reading list, a trip, a running project log.
                </p>
                <div className="diary__actions">
                  <button type="button" className="btn btn--primary" onClick={openToday}>
                    Open today
                  </button>
                  <button
                    type="button"
                    className="btn btn--secondary"
                    onClick={() => void createPage(null)}
                  >
                    <PlusIcon size={15} />
                    Add page
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
          )}
        </div>
      </div>
    </WorkspaceLayout>
  );
}
