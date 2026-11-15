import { useMemo, useState } from "react";
import { marked } from "marked";
import DOMPurify from "dompurify";
import { DocIcon, PencilIcon, ShieldIcon, TrashIcon, WidthIcon } from "../ui/icons";
import { dateKeyToDirName, diaryEntryPath, formatDateKey } from "../../util/calendar";
import type { SaveState } from "../../state/diaryStore";
import type { DiaryPage } from "../../types";

const SAVE_LABEL: Record<SaveState, string> = {
  idle: "",
  saving: "Saving…",
  saved: "Saved",
  error: "Not saved",
};

const SAVE_TONE: Record<SaveState, string> = {
  idle: "neutral",
  saving: "pending",
  saved: "ok",
  error: "danger",
};

interface DiaryEditorProps {
  page: DiaryPage;
  saveState: SaveState;
  onChange(patch: { title?: string; body?: string }): void;
  onDelete(): void;
}

export function DiaryEditor({ page, saveState, onChange, onDelete }: DiaryEditorProps) {
  const [fullWidth, setFullWidth] = useState(false);
  const [preview, setPreview] = useState(false);
  const words = page.body.trim() ? page.body.trim().split(/\s+/).length : 0;
  const previewHtml = useMemo(
    () => DOMPurify.sanitize(marked.parse(page.body, { async: false }) as string),
    [page.body],
  );
  const characters = page.body.length;
  const readingMinutes = words > 0 ? Math.max(1, Math.round(words / 220)) : 0;
  const heading = page.date
    ? formatDateKey(page.date, {
        weekday: "long",
        day: "numeric",
        month: "long",
        year: "numeric",
      })
    : page.title || "Untitled page";
  const storedAs =
    page.filePath ??
    (page.date
      ? page.nameLocked
        ? diaryEntryPath(page.date)
        : `${dateKeyToDirName(page.date)}/…`
      : null);

  return (
    <div className="diary">
      <div className={`diary__stage${fullWidth ? " diary__stage--full" : ""}`}>
        <div className="diary__sheet">
          <header className="diary__head">
            {page.date ? (
              <div className="diary__titled">
                <h2 className="diary__date">{heading}</h2>
                {page.nameLocked ? (
                  <span
                    className="diary__lock"
                    title="The date-named daily entry: its file name cannot be changed"
                  >
                    <ShieldIcon size={12} />
                    {dateKeyToDirName(page.date)}.md
                  </span>
                ) : (
                  <input
                    className="diary__nameinput"
                    value={page.title}
                    placeholder="Add a name…"
                    aria-label="File name"
                    title="Rename this file"
                    onChange={(event) => onChange({ title: event.target.value })}
                  />
                )}
              </div>
            ) : (
              <input
                className="diary__title"
                value={page.title}
                placeholder="Page title"
                aria-label="Page title"
                onChange={(event) => onChange({ title: event.target.value })}
              />
            )}
            <div className="diary__head-actions">
              <span className={`pill pill--${SAVE_TONE[saveState]}`}>{SAVE_LABEL[saveState]}</span>
              <div className="diary__width-toggle" role="group" aria-label="View mode">
                <button
                  type="button"
                  aria-pressed={!preview}
                  onClick={() => setPreview(false)}
                  title="Edit the raw Markdown"
                >
                  <PencilIcon size={12} />
                  Write
                </button>
                <button
                  type="button"
                  aria-pressed={preview}
                  onClick={() => setPreview(true)}
                  title="Preview the rendered Markdown"
                >
                  <DocIcon size={12} />
                  Preview
                </button>
              </div>
              <div className="diary__width-toggle" role="group" aria-label="Editor width">
                <button
                  type="button"
                  aria-pressed={!fullWidth}
                  onClick={() => setFullWidth(false)}
                  title="Centered reading width"
                >
                  <WidthIcon size={12} />
                  Centered
                </button>
                <button
                  type="button"
                  aria-pressed={fullWidth}
                  onClick={() => setFullWidth(true)}
                  title="Use the full workspace width"
                >
                  <WidthIcon size={12} />
                  Full
                </button>
              </div>
            </div>
          </header>

          {preview ? (
            page.body.trim() ? (
              <div
                className="diary__preview"
                aria-label="Rendered markdown preview"
                // Sanitized with DOMPurify before injection.
                dangerouslySetInnerHTML={{ __html: previewHtml }}
              />
            ) : (
              <div className="diary__preview diary__preview--empty muted">
                Nothing to preview yet — switch to Write and add some Markdown.
              </div>
            )
          ) : (
            <textarea
              className="diary__body"
              value={page.body}
              placeholder="Write freely. Markdown is fine, and nothing leaves this machine."
              aria-label={page.date ? "Diary entry" : "Page content"}
              onChange={(event) => onChange({ body: event.target.value })}
            />
          )}

          <footer className="diary__foot">
            <span>{words === 1 ? "1 word" : `${words} words`}</span>
            {storedAs ? <span className="mono">{storedAs}</span> : null}
            <span className="spacer" />
            <button type="button" className="btn btn--ghost btn--sm" onClick={onDelete}>
              <TrashIcon size={13} />
              Delete page
            </button>
          </footer>
        </div>

        <aside className="diary__aside" aria-label="Page details">
          <div className="diary__aside-card">
            <div className="diary__aside-title">{page.date ? "Entry" : "Page"}</div>
            <div className="diary__aside-value">
              {page.title.trim() ? `${heading} · ${page.title.trim()}` : heading}
            </div>
          </div>
          <div className="diary__aside-card">
            <div className="diary__aside-title">At a glance</div>
            <div className="diary__aside-row">
              <span>Words</span>
              <strong>{words.toLocaleString()}</strong>
            </div>
            <div className="diary__aside-row">
              <span>Characters</span>
              <strong>{characters.toLocaleString()}</strong>
            </div>
            <div className="diary__aside-row">
              <span>Reading time</span>
              <strong>{readingMinutes === 0 ? "—" : `~${readingMinutes} min`}</strong>
            </div>
            <div className="diary__aside-row">
              <span>Saved</span>
              <strong>{SAVE_LABEL[saveState] || "—"}</strong>
            </div>
          </div>
          {storedAs ? (
            <div className="diary__aside-card">
              <div className="diary__aside-title">Stored as</div>
              <div className="diary__aside-row">
                <code className="mono">{storedAs}</code>
              </div>
            </div>
          ) : null}
        </aside>
      </div>
    </div>
  );
}
