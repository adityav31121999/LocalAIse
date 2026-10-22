import { useEffect, useMemo, useState } from "react";
import { BookIcon, ChevronLeftIcon, PanelLeftIcon, PlusIcon } from "../ui/icons";
import { CalendarView } from "./CalendarView";
import { Reminders } from "./Reminders";
import { DiaryEditor } from "./DiaryEditor";
import { WorkspaceLayout } from "../layout/WorkspaceLayout";
import { useDiaryStore } from "../../state/diaryStore";
import { useNavStore } from "../../state/navStore";
import { useProjectStore, selectActiveProject } from "../../state/projectStore";
import { useSettingsStore } from "../../state/settingsStore";
import { formatDateKey, monthStart, todayKey } from "../../util/calendar";
import type { DiaryPage } from "../../types";

const LIST_DATE: Intl.DateTimeFormatOptions = { day: "numeric", month: "short" };
const LONG_DATE: Intl.DateTimeFormatOptions = {
  weekday: "long",
  day: "numeric",
  month: "long",
  year: "numeric",
};

function pageLabel(page: DiaryPage): string {
  if (page.date) return formatDateKey(page.date, LIST_DATE);
  return page.title.trim() || "Untitled page";
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
  const preview = page.body.trim().split("\n")[0];

  return (
    <button
      type="button"
      className={`diary-row${selected ? " diary-row--active" : ""}`}
      aria-current={selected ? "true" : undefined}
      onClick={onSelect}
    >
      <span className="diary-row__label truncate">{pageLabel(page)}</span>
      <span className={`diary-row__preview truncate${preview ? "" : " muted"}`}>
        {preview || "Empty"}
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
  const today = todayKey();

  useEffect(() => {
    void load(scopeProjectId);
  }, [load, scopeProjectId]);

  const activePage = pages.find((page) => page.id === activePageId) ?? null;
  const selectedDate = activePage?.date ?? pendingDate;

  const entryDates = useMemo(
    () => new Set(pages.filter((page) => page.date).map((page) => page.date as string)),
    [pages],
  );
  const dated = useMemo(
    () =>
      pages
        .filter((page) => page.date)
        .sort((a, b) => (b.date ?? "").localeCompare(a.date ?? "")),
    [pages],
  );
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
              <span className="truncate">Diary</span>
              <span className="badge">{pages.length} pages</span>
            </div>
            <div className="project-header__path truncate">
              {scopeProjectId
                ? `${project?.name} · diary/ folder in the project · one page per day`
                : "Personal space · one page per day, plus any page you add"}
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
          <div className="panel__head">
            <span className="panel__title">Calendar</span>
          </div>
          <div className="panel__cal">
            <CalendarView
              month={month}
              today={today}
              selectedDate={selectedDate}
              entryDates={entryDates}
              onMonthChange={setMonth}
              onSelectDate={selectDate}
            />
          </div>

          <Reminders selectedDate={selectedDate} />

          <div className="panel__head">
            <span className="panel__title">Pages</span>
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
              <p className="muted mt-2">
                No pages yet. Pick a date in the calendar, or add a page.
              </p>
            ) : (
              <>
                {dated.length > 0 ? <div className="panel__group">Entries</div> : null}
                {dated.map((page) => (
                  <PageRow
                    key={page.id}
                    page={page}
                    selected={page.id === activePageId}
                    onSelect={() => selectPage(page.id)}
                  />
                ))}

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

          <div className="panel__foot">
            {scopeProjectId
              ? "Stored in this project's diary/ folder as Markdown. Nothing here is uploaded."
              : "Stored in your personal space. Nothing here is uploaded."}
          </div>
        </div>
      }
    >
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
                  onClick={() => void createPage(pendingDate)}
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
    </WorkspaceLayout>
  );
}
