import type { ChatMode } from "../../types";

interface ModeIndicatorProps {
  mode: ChatMode;
  onToggle?: (mode: ChatMode) => void;
}

const MODES: Array<{
  value: ChatMode;
  label: string;
  hint: string;
}> = [
  {
    value: "chat",
    label: "Chat",
    hint: "Simple assistant — answers only, no tools",
  },
  {
    value: "agent",
    label: "Agent",
    hint: "Agent — can read, write, search and run tools on this folder",
  },
];

/**
 * The single switch between chat-bot and agent mode. The value comes from the
 * Rust project record, so the UI never claims a mode the core did not store.
 * Rendered as a static chip when there is no way to change it.
 */
export function ModeIndicator({ mode, onToggle }: ModeIndicatorProps) {
  if (!onToggle) {
    const chosen = MODES.find((entry) => entry.value === mode) ?? MODES[0];
    return (
      <span className="mode mode--static mode--selected-chat" title={chosen.hint}>
        {mode === "agent" ? "Agent" : "Chat"}
      </span>
    );
  }

  return (
    <span
      className={`mode${mode === "agent" ? " mode--agent" : ""}`}
      role="group"
      aria-label="Assistant mode"
    >
      {MODES.map((entry) => {
        const selected = entry.value === mode;
        return (
          <button
            key={entry.value}
            type="button"
            className={`mode__option${selected ? " mode__option--selected" : ""}`}
            aria-pressed={selected}
            title={entry.hint}
            disabled={selected}
            onClick={() => onToggle(entry.value)}
          >
            {entry.label}
          </button>
        );
      })}
    </span>
  );
}
