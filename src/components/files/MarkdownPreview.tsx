/** Renders markdown content as formatted HTML. Used for AI summaries
 * so users get a readable preview alongside the raw .md file.
 *
 * Rendering goes through the shared `Markdown` renderer so the preview and the
 * chat/research panels all interpret markdown the same way (and the app keeps a
 * single, XSS-safe renderer instead of two competing implementations).
 */
import { Markdown } from "../../util/markdown";

interface MarkdownPreviewProps {
  markdown: string;
}

/** Boilerplate headings/footers that LLMs tack onto generated summaries. */
const SUMMARY_BOILERPLATE = [
  /^#+\s*Executive Summary: Comprehensive Synthesis of Section Summaries\s*$/im,
  /^This summary provides a complete, actionable, and well-structured executive overview[\s\S]*?ready for use by technical leads, product managers, or research directors to guide implementation, resource allocation, and validation\.\s*$/im,
];

/** Strips LLM-added summary boilerplate so the preview shows only real content. */
function stripSummaryBoilerplate(markdown: string): string {
  let cleaned = markdown;
  for (const pattern of SUMMARY_BOILERPLATE) {
    cleaned = cleaned.replace(pattern, "").trim();
  }
  return cleaned;
}

export function MarkdownPreview({ markdown }: MarkdownPreviewProps) {
  if (!markdown?.trim()) {
    return null;
  }

  const cleaned = stripSummaryBoilerplate(markdown);
  if (!cleaned) {
    return null;
  }

  return <Markdown className="md--document" text={cleaned} />;
}
