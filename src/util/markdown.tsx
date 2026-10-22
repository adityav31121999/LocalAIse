/** Minimal, dependency-free markdown renderer.
 *
 * Model output is markdown, but until now the chat panel rendered it as raw
 * text — "**bold**", ```fences``` and numbered lists all showed literally.
 * This module parses the subset of markdown that local models actually emit
 * and builds React elements directly. It never uses `dangerouslySetInnerHTML`,
 * so untrusted model output can never inject markup or script. Unknown syntax
 * falls back to plain text, and partial (streaming) input renders
 * progressively: an unclosed fence becomes a code block, unclosed emphasis
 * markers stay literal until the closing token arrives.
 */

import { createElement, type ReactNode } from "react";

/* ------------------------------------------------------------------ *
 * Inline parsing (bold, italic, code spans, strike, links)
 * ------------------------------------------------------------------ */

// Alternatives are ordered so code spans win over emphasis markers inside
// them, and strong runs can carry emphasized text.
const HEADING_TAGS = ["h1", "h2", "h3", "h4", "h5", "h6"] as const;
// Altered alternatives, in priority order. A fresh RegExp is made per call so
// recursive inline parsing cannot corrupt a shared `lastIndex`.
const INLINE_SOURCE =
  "(`[^`\\n]+`)|(\\*\\*[\\s\\S]+?\\*\\*)|(\\*[^*\\n]+\\*)|(~~[^~\\n]+~~)|((?<!\\w)_[^_\\n]+_(?!\\w))|(\\[[^\\]\\n]+\\]\\([^)\\n]+?\\))";

// "https://…", "http://…", "mailto:…" or a bare domain like example.com/x.
const SAFE_URL = /^(https?:\/\/|mailto:)[^\s"'<>()]+$/i;
const BARE_DOMAIN = /^[a-z0-9-]+(\.[a-z0-9-]+)+(\/|\?|#|$)/i;

function safeLinkHref(raw: string): string | null {
  const url = raw.trim();
  if (url.startsWith("<") && url.endsWith(">")) return safeLinkHref(url.slice(1, -1));
  if (/[<>"'\\]/.test(url)) return null;
  if (url.startsWith("//")) return `https:${url}`;
  if (SAFE_URL.test(url)) return url;
  if (BARE_DOMAIN.test(url)) return `https://${url}`;
  return null;
}

/** One inline tokenizer pass. `depth` guards against runaway recursion. */
function parseInline(text: string, keyPrefix: string, depth = 0): ReactNode[] {
  if (!text || depth > 3) return [];
  const inlineRe = new RegExp(INLINE_SOURCE, "g");
  const nodes: ReactNode[] = [];
  let index = 0;

  for (let match = inlineRe.exec(text); match; match = inlineRe.exec(text)) {
    const [token, code, strong, emStar, del, emUnderscore, link] = match;
    if (match.index > index) nodes.push(text.slice(index, match.index));
    index = match.index + token.length;
    const key = `${keyPrefix}-${match.index}`;

    if (code !== undefined) {
      nodes.push(createElement("code", { key }, token.slice(1, -1)));
    } else if (strong !== undefined) {
      nodes.push(createElement("strong", { key }, parseNested(token.slice(2, -2), key, depth)));
    } else if (emStar !== undefined || emUnderscore !== undefined) {
      const body = emStar !== undefined ? token.slice(1, -1) : token.slice(1, -1);
      nodes.push(createElement("em", { key }, parseNested(body, key, depth)));
    } else if (del !== undefined) {
      nodes.push(createElement("del", { key }, parseNested(token.slice(2, -2), key, depth)));
    } else if (link !== undefined) {
      const labelEnd = token.indexOf("](");
      const label = token.slice(1, labelEnd);
      const href = safeLinkHref(token.slice(labelEnd + 2, -1));
      // Labels render as plain text — no nested emphasis inside link text.
      const children = href ? [label] : parseInline(label, key, depth);
      nodes.push(
        href
          ? createElement("a", { key, href, target: "_blank", rel: "noreferrer noopener" }, children)
          : createElement("span", { key, className: "md__dead-link" }, children),
      );
    }
  }

  if (index < text.length) nodes.push(text.slice(index));
  return nodes;
}

/** Contents of **bold** / *em* / ~~del~~ runs: nested emphasis, no links. */
function parseNested(text: string, keyPrefix: string, depth: number): ReactNode[] {
  if (depth > 3) return [text];
  const nodes: ReactNode[] = [];
  let index = 0;
  const pattern = /(\*[^*\n]+\*)|(~~[^~\n]+~~)/g;
  for (let match = pattern.exec(text); match; match = pattern.exec(text)) {
    const [token, em] = match;
    if (match.index > index) nodes.push(text.slice(index, match.index));
    index = match.index + token.length;
    const key = `${keyPrefix}-n${match.index}`;
    if (em) nodes.push(createElement("em", { key }, token.slice(1, -1)));
    else nodes.push(createElement("del", { key }, token.slice(2, -2)));
  }
  if (index < text.length) nodes.push(text.slice(index));
  return nodes;
}

/* ------------------------------------------------------------------ *
 * Block parsing
 * ------------------------------------------------------------------ */

const HR_RE = /^[ \t]*([-*+_])[ \t]*(?:\1[ \t]*){2,}$/;
const HEADING_RE = /^(#{1,6})[ \t]+(.*)$/;
const FENCE_RE = /^[ \t]*(`{3,}|~{3,})[ \t]*([\w-]*)/;
const LIST_RE = /^([ \t]*)([-*+]|(?:\d{1,9})[.)])[ \t]+(.*)$/;
const BLOCKQUOTE_RE = /^[ \t]*>[ \t]?/;
const TABLE_RULE_RE = /^[ \t]*\|?[ \t]*:?-*:?[ \t]*(\|[ \t]*:?-*:?[ \t]*)*\|?[ \t]*$/;

function normalizeIndent(spaces: string): number {
  // Tabs count as two spaces so mixed-indent files stay predictable.
  return Math.floor(spaces.replace(/\t/g, "  ").length / 2) * 2;
}

function splitCells(row: string): string[] {
  let text = row.trim();
  if (text.startsWith("|") && text.endsWith("|") && text.length > 1) text = text.slice(1, -1);
  return text.split("|");
}

/* ------------------------------------------------------------------ */

/** One paragraph as <p>. Lines hard-break (chat style) rather than fold. */
function renderParagraph(lines: string[], keyPrefix: string): ReactNode {
  const children: ReactNode[] = [];
  lines.forEach((line, i) => {
    if (i > 0) children.push(createElement("br", { key: `${keyPrefix}-br${i}` }));
    children.push(...parseInline(line, `${keyPrefix}-${i}`));
  });
  return createElement("p", { key: keyPrefix }, children);
}

function isBlockBoundary(line: string): boolean {
  return (
    !line.trim() ||
    HEADING_RE.test(line) ||
    HR_RE.test(line) ||
    FENCE_RE.test(line) ||
    LIST_RE.test(line) ||
    BLOCKQUOTE_RE.test(line)
  );
}

function parseTable(lines: string[], start: number, keyPrefix: string): [ReactNode, number] {
  const header = splitCells(lines[start] ?? "");
  const rows: string[][] = [];
  let cursor = start + 2;
  while (cursor < lines.length) {
    const line = lines[cursor] ?? "";
    if (isBlockBoundary(line) || !line.includes("|")) break;
    rows.push(splitCells(line));
    cursor += 1;
  }

  const head = createElement(
    "tr",
    { key: `${keyPrefix}-h` },
    header.map((cell, i) =>
      createElement("th", { key: i }, parseInline(cell.trim(), `${keyPrefix}-h${i}`)),
    ),
  );
  const body = rows.map((row, r) =>
    createElement(
      "tr",
      { key: `${keyPrefix}-r${r}` },
      header.map((_, i) =>
        createElement("td", { key: i }, parseInline((row[i] ?? "").trim(), `${keyPrefix}-r${r}-${i}`)),
      ),
    ),
  );
  const table = createElement(
    "table",
    { key: keyPrefix },
    createElement("thead", { key: "t" }, head),
    createElement("tbody", { key: "b" }, body),
  );
  return [table, cursor];
}

function parseListGroup(
  lines: string[],
  start: number,
  spec: { ordered: boolean; indent: number; start?: number },
  keyPrefix: string,
): [ReactNode, number] {
  const items: ReactNode[] = [];
  let cursor = start;

  while (cursor < lines.length) {
    const match = LIST_RE.exec(lines[cursor] ?? "");
    if (!match) break;
    const indent = normalizeIndent(match[1] ?? "");
    const ordered = /\d/.test(match[2] ?? "");

    if (indent < spec.indent) break; // dedent → this list level is finished
    if (ordered !== spec.ordered) break; // list kind changed → restart outside

    const itemText = [match[3] ?? ""];
    cursor += 1;

    // Lazy continuation: plain follow-up lines fold into the current item.
    while (cursor < lines.length && lines[cursor]!.trim() && !LIST_RE.test(lines[cursor] ?? "")) {
      itemText.push((lines[cursor] ?? "").trim());
      cursor += 1;
    }

    // Blank separator before a deeper sibling belongs to the last item.
    if (
      cursor < lines.length &&
      !lines[cursor]!.trim() &&
      cursor + 1 < lines.length &&
      LIST_RE.test(lines[cursor + 1] ?? "")
    ) {
      const upcoming = LIST_RE.exec(lines[cursor + 1] ?? "");
      if (upcoming && normalizeIndent(upcoming[1] ?? "") >= spec.indent + 2) cursor += 1;
    }

    let children: ReactNode[] = parseInline(itemText.join(" "), `${keyPrefix}-${items.length}`);
    const nested = LIST_RE.exec(lines[cursor] ?? "");
    if (nested && normalizeIndent(nested[1] ?? "") >= spec.indent + 2) {
      const [nestedList, used] = parseListGroup(
        lines,
        cursor,
        {
          ordered: /\d/.test(nested[2] ?? ""),
          indent: spec.indent + 2,
          start: /^\d/.test(nested[2] ?? "") ? Number.parseInt(nested[2]!, 10) : undefined,
        },
        `${keyPrefix}-c${cursor}`,
      );
      children = children.concat(nestedList);
      cursor = used; // absolute index where the nested group stopped
    }

    items.push(createElement("li", { key: `${keyPrefix}-${items.length}` }, children));
  }

  if (items.length === 0) return ["", cursor];
  const props: { key: string; start?: number } = { key: keyPrefix };
  if (spec.ordered && spec.start && spec.start > 1) props.start = spec.start;
  const list = createElement(spec.ordered ? "ol" : "ul", props, ...items);
  return [list, cursor];
}

function parseBlocks(text: string, keyPrefix: string): ReactNode[] {
  const lines = text.split("\n");
  const blocks: ReactNode[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i] ?? "";
    const key = `${keyPrefix}-b${i}`;

    // Blank lines only separate blocks.
    if (!line.trim()) {
      i += 1;
      continue;
    }

    // Fenced code block. An unclosed fence (mid-stream) consumes the rest.
    const fence = FENCE_RE.exec(line);
    if (fence) {
      const marker = fence[1] ?? "```";
      const info = (fence[2] ?? "").trim();
      const closer = new RegExp(`^[ \\t]*${marker[0]}{${marker.length},}[ \\t]*$`);
      i += 1;
      const code: string[] = [];
      while (i < lines.length && !closer.test(lines[i] ?? "")) {
        code.push(lines[i] ?? "");
        i += 1;
      }
      if (i < lines.length) i += 1; // closing fence
      blocks.push(
        createElement(
          "pre",
          { key },
          createElement("code", info ? { className: `lang-${info}` } : undefined, code.join("\n")),
        ),
      );
      continue;
    }

    // Heading
    const heading = HEADING_RE.exec(line);
    if (heading) {
      const level = Math.min(6, (heading[1] ?? "#").length);
      blocks.push(createElement(HEADING_TAGS[level - 1]!, { key }, parseInline((heading[2] ?? "").trim(), key)));
      i += 1;
      continue;
    }

    // Horizontal rule
    if (HR_RE.test(line) && !LIST_RE.test(line)) {
      blocks.push(createElement("hr", { key }));
      i += 1;
      continue;
    }

    // Table: header row followed by a dashed separator row.
    if (line.includes("|") && i + 1 < lines.length && TABLE_RULE_RE.test(lines[i + 1] ?? "")) {
      const [table, nextIndex] = parseTable(lines, i, key);
      blocks.push(table);
      i = nextIndex;
      continue;
    }

    // Blockquote
    if (BLOCKQUOTE_RE.test(line)) {
      const quoted: string[] = [];
      while (i < lines.length && /^[ \t]*>([ \t]|$)/.test(lines[i] ?? "")) {
        quoted.push((lines[i] ?? "").replace(BLOCKQUOTE_RE, ""));
        i += 1;
      }
      blocks.push(createElement("blockquote", { key }, parseBlocks(quoted.join("\n"), key)));
      continue;
    }

    // List group: consecutive list lines, stopping at a plain paragraph line.
    const listMatch = LIST_RE.exec(line);
    if (listMatch) {
      const [list, nextIndex] = parseListGroup(
        lines,
        i,
        {
          ordered: /\d/.test(listMatch[2] ?? ""),
          indent: normalizeIndent(listMatch[1] ?? ""),
          start: /^\d/.test(listMatch[2] ?? "") ? Number.parseInt(listMatch[2]!, 10) : undefined,
        },
        key,
      );
      blocks.push(list);
      i = nextIndex;
      continue;
    }

    // Paragraph: plain lines until blank or the next structural block.
    const paragraph: string[] = [];
    while (i < lines.length && !isBlockBoundary(lines[i] ?? "")) {
      paragraph.push(lines[i] ?? "");
      i += 1;
    }
    blocks.push(renderParagraph(paragraph, key));
  }

  return blocks;
}

export interface MarkdownProps {
  /** Markdown string, possibly a partially streamed message. */
  text: string;
  className?: string;
}

/** Renders a markdown string as nested React elements (XSS-safe by design). */
export function Markdown({ text, className }: MarkdownProps) {
  return createElement(
    "div",
    { className: className ? `md ${className}` : "md" },
    ...parseBlocks(text, "md"),
  );
}
