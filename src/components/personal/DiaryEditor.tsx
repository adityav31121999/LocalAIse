import { TrashIcon } from "../ui/icons";
import { formatDateKey } from "../../util/calendar";
import type { SaveState } from "../../state/diaryStore";
import type { DiaryPage } from "../../types";

const SAVE_LABEL: Record<SaveState, string> = {
  idle: "",
  saving: "Saving…",
  saved: "Saved",
  error: "Not saved",
};

interface DiaryEditorProps {
  page: DiaryPage;
  saveState: SaveState;
  onChange(patch: { title?: string; body?: string }): void;
  onDelete(): void;
}

export function DiaryEditor({ page, saveState, onChange, onDelete }: DiaryEditorProps) {
  const words = page.body.trim() ? page.body.trim().split(/\s+/).length : 0;

  return (
    <div className="diary">
      <div className="diary__sheet">
        <header className="diary__head">
          {page.date ? (
            <h2 className="diary__date">
              {formatDateKey(page.date, {
                weekday: "long",
                day: "numeric",
                month: "long",
                year: "numeric",
              })}
            </h2>
          ) : (
            <input
              className="diary__title"
              value={page.title}
              placeholder="Page title"
              aria-label="Page title"
              onChange={(event) => onChange({ title: event.target.value })}
            />
          )}
          <span className="diary__meta">{SAVE_LABEL[saveState]}</span>
        </header>

        <textarea
          className="diary__body"
          value={page.body}
          placeholder="Write freely. Markdown is fine, and nothing leaves this machine."
          aria-label={page.date ? "Diary entry" : "Page content"}
          onChange={(event) => onChange({ body: event.target.value })}
        />

        <footer className="diary__foot">
          <span>{words === 1 ? "1 word" : `${words} words`}</span>
          {page.date ? <span className="mono">{page.date}.md</span> : null}
          <span className="spacer" />
          <button type="button" className="btn btn--ghost btn--sm" onClick={onDelete}>
            <TrashIcon size={13} />
            Delete page
          </button>
        </footer>
      </div>
    </div>
  );
}
