import { useEffect, useRef, useState, type CSSProperties } from "react";

export interface SlideNameProps {
  /** The full text. It is never wrapped or shortened in the DOM. */
  text: string;
  className?: string;
  style?: CSSProperties;
  /** Accessible label; defaults to the text. */
  label?: string;
}

/**
 * A single-line name that slides sideways on hover when it overflows its box.
 *
 * The sidebar is narrow, so long file names would otherwise be clipped with no
 * way to read them. Instead of reserving room for a permanent tooltip or
 * widening the panel, the name measures its own overflow and shifts the line
 * to reveal the hidden tail, then settles back. Names that fit never animate,
 * and the ellipsis returns as soon as the slide ends.
 */
export function SlideName({ text, className, style, label }: SlideNameProps) {
  const boxRef = useRef<HTMLSpanElement>(null);
  const [offset, setOffset] = useState(0);

  useEffect(() => {
    const box = boxRef.current;
    if (!box) return;

    const measure = () => {
      const next = Math.max(0, box.scrollWidth - box.clientWidth);
      // Ignore sub-pixel noise so the animation is not re-triggered by
      // scrollbar rounding when the panel is resized.
      setOffset((prev) => (Math.abs(prev - next) > 1 ? next : prev));
    };

    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(box);
    return () => observer.disconnect();
  }, [text]);

  return (
    <span
      ref={boxRef}
      className={`slide-name${offset > 1 ? " slide-name--overflow" : ""}${className ? ` ${className}` : ""}`}
      style={{ "--slide-offset": `${offset}px`, ...style } as CSSProperties}
      title={label ?? text}
    >
      {text}
    </span>
  );
}
