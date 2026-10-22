import { useEffect, useMemo } from "react";
import { BookIcon, PlusIcon } from "../ui/icons";
import { DiaryEditor } from "./DiaryEditor";
import { useDiaryStore } from "../../state/diaryStore";
import { formatDateKey, todayKey } from "../../util/calendar";
import type { DiaryPage, Project } from "../../types";

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

/**
 * The project's own diary, rendered as a workspace tab. Pages are Markdown
 * files in `<project>/diary/`, one per date, so this view is the same data a
 * file explorer shows.
 */
export function ProjectDiary({ project }: { project: Project }) {
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

  useEffect(() => {
    void load(project.id);
  }, [project.id, load]);

  const sorted = useMemo(
    () =>
      [...pages].sort((a, b) => {
        if (a.date && b.date) return b.date.localeCompare(a.date);
        if (a.date) return -1;
        if (b.date) return 1;
        return b.updatedAt.localeCompare(a.updatedAt);
      }),
    [pages],
  );

  const activePage = pages.find((page) => page.id === activePageId) ?? null;

  return (
    <div className="project-diary">
      <aside className="project-diary__list">
        <div className="panel__head">
          <span className="panel__title">Diary</span>
          <span className="badge">{pages.length}</span>
        </div>

        <div className="panel__scroll">
          {status === "loading" && pages.length === 0 ? (
            <p className="muted mt-2">
              <span className="spinner" aria-hidden="true" /> Opening the diary…
            </p>
          ) : pages.length === 0 ? (
            <p className="muted mt-2">No pages yet. Start today's entry or add a page.</p>
          ) : (
            sorted.map((page) => {
              const preview = page.body.trim().split("\n")[0];
              return (
                <button
                  key={page.id}
                  type="button"
                  className={`diary-row${page.id === activePageId ? " diary-row--active" : ""}`}
                  aria-current={page.id === activePageId ? "true" : undefined}
                  onClick={() => selectPage(page.id)}
                >
                  <span className="diary-row__label truncate">{pageLabel(page)}</span>
                  <span className={`diary-row__preview truncate${preview ? "" : " muted"}`}>
                    {preview || "Empty"}
                  </span>
                </button>
              );
            })
          )}
        </div>

        <div className="panel__foot">
          Saved in <span className="mono">diary/</span> as Markdown · one page per day
        </div>
      </aside>

      <div className="project-diary__main">
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
                  <h2 className="diary__date">{project.name} diary</h2>
                  <p className="muted">
                    One Markdown page per day, stored in this project's <code>diary/</code> folder.
                    Nothing here is uploaded.
                  </p>
                  <div className="diary__actions">
                    <button type="button" className="btn btn--primary" onClick={() => selectDate(todayKey())}>
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
  );
}
