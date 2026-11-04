import { useCallback, useEffect, useMemo, useState } from "react";
import { DocIcon } from "../ui/icons";
import { listAllFiles } from "../../api/files";
import type { DirectoryEntry, Project } from "../../types";

type MentionTrigger = "@" | "#";

interface MentionState {
  trigger: MentionTrigger;
  query: string;
  start: number;
}

interface MentionAutocompleteProps {
  project: Project;
  value: string;
  cursor: number | null;
  onSelect: (filePath: string, displayText: string) => void;
}

/**
 * Detects `@` or `#` triggers in the composer textarea and shows a dropdown of
 * matching project files (all files in the project directory, minus the
 * app-owned dirs like chats/). As the user types after the trigger, the list
 * is filtered to names containing that text. When the user picks a file,
 * `onSelect` is called with the file path and the display text to insert.
 */
export function MentionAutocomplete({
  project,
  value,
  cursor,
  onSelect,
}: MentionAutocompleteProps) {
  const [mention, setMention] = useState<MentionState | null>(null);
  const [highlighted, setHighlighted] = useState(0);
  const [entries, setEntries] = useState<DirectoryEntry[]>([]);

  useEffect(() => {
    let mounted = true;
    listAllFiles(project.id)
      .then((list) => {
        if (!mounted) return;
        setEntries(list);
      })
      .catch((error) => {
        console.error("Failed to list project files for mentions:", error);
      });
    return () => {
      mounted = false;
    };
  }, [project.id]);

  /** Scan backwards from the cursor for a `@` or `#` trigger followed by query text. */
  const detectMention = useCallback(
    (val: string, pos: number | null): MentionState | null => {
      const cursor = pos ?? val.length;
      if (cursor === 0) return null;
      const before = val.slice(0, cursor);
      const match = before.match(/(?:^|\s)([@#])([^\s]*)$/);
      if (!match) return null;
      const [, trigger, query] = match;
      return {
        trigger: trigger as MentionTrigger,
        query,
        start: cursor - match[0].length,
      };
    },
    [],
  );

  const filtered = useMemo(() => {
    if (!mention) return [];
    const q = mention.query.toLowerCase();
    return entries.filter((e) => e.name.toLowerCase().includes(q));
  }, [mention, entries]);

  // Re-detect on every value/cursor change.
  useEffect(() => {
    const detected = detectMention(value, cursor);
    setMention(detected);
    if (detected) {
      setHighlighted(0);
    } else {
      setHighlighted(0);
    }
  }, [value, cursor, detectMention]);

  const handleSelect = useCallback(
    (file: DirectoryEntry) => {
      if (!mention) return;
      onSelect(file.name, `${mention.trigger}${file.name}`);
      setMention(null);
      setHighlighted(0);
    },
    [mention, onSelect],
  );

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      if (!mention || filtered.length === 0) return;

      if (event.key === "Escape") {
        setMention(null);
        setHighlighted(0);
        return;
      }

      if (event.key === "ArrowDown") {
        event.preventDefault();
        setHighlighted((h) => (h + 1) % filtered.length);
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        setHighlighted((h) => (h - 1 + filtered.length) % filtered.length);
      } else if (event.key === "Enter") {
        event.preventDefault();
        const file = filtered[highlighted];
        handleSelect(file);
      }
    },
    [mention, filtered, highlighted, handleSelect],
  );

  if (!mention || filtered.length === 0) return null;

  return (
    <div className="composer__mentions" onKeyDown={handleKeyDown}>
      <ul className="composer__mention-list">
        {filtered.map((file, index) => (
          <li
            key={file.path}
            className={`composer__mention-item${index === highlighted ? " composer__mention-item--active" : ""}`}
            onMouseMove={() => setHighlighted(index)}
            onClick={() => handleSelect(file)}
          >
            <DocIcon size={12} />
            <span className="composer__mention-name">{file.name}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
