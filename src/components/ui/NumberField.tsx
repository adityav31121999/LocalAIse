/** A controlled number input the user can freely clear.
 *
 * The classic controlled-input problem: coercing the value on every
 * keystroke (`Number(x) || 0`) snaps an emptied field straight back to "0",
 * so digits can never be deleted — and a leading digit re-renders as "02…".
 * This component keeps the raw text locally while the user is typing and
 * only commits parsed numbers, so an empty box stays empty until they type,
 * and a blur snaps the display to whatever was actually stored.
 */

import { useEffect, useRef, useState, type CSSProperties } from "react";

export interface NumberFieldProps {
  /** The stored value; the display resyncs whenever this changes from outside. */
  value: number;
  /** Smallest accepted number; also what an emptied field commits. */
  min?: number;
  /** Largest accepted number, when the setting has a ceiling. */
  max?: number;
  step?: number;
  className?: string;
  style?: CSSProperties;
  /** Accessible name; also used for the title. */
  label: string;
  onChange(value: number): void;
}

export function NumberField({
  value,
  min = 0,
  max,
  step,
  className,
  style,
  label,
  onChange,
}: NumberFieldProps) {
  const [text, setText] = useState<string>(String(value));
  const blank = useRef(false);

  // Outside changes (reset, scope switch, applying) win over the local text,
  // but an intentionally blank field is left alone while it has focus.
  useEffect(() => {
    if (blank.current && text.trim() === "") return;
    if (Number(text) !== value) setText(String(value));
    // The display follows the stored value only; `text` is deliberately not
    // a dependency, or every keystroke would fight the user.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value]);

  const clamp = (n: number): number => {
    if (Number.isNaN(n)) return min;
    return Math.max(min, max != null ? Math.min(max, n) : n);
  };

  return (
    <input
      className={className}
      style={style}
      type="number"
      inputMode="numeric"
      min={min}
      max={max}
      step={step}
      value={text}
      title={label}
      aria-label={label}
      onChange={(event) => {
        const raw = event.target.value;
        setText(raw);
        if (raw.trim() === "") {
          blank.current = true;
          onChange(min);
          return;
        }
        blank.current = false;
        const n = Number(raw);
        if (!Number.isNaN(n)) onChange(clamp(n));
      }}
      onBlur={() => {
        blank.current = false;
        setText(String(value));
      }}
    />
  );
}
