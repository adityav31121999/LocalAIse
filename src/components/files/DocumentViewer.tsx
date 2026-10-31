/** Document Viewer Panel — displays document content with AI summarization. */

import { useEffect, useState, useCallback, useMemo } from "react";
import {
  SparkIcon,
  XIcon,
  ChevronLeftIcon,
  DocIcon,
  QuoteIcon,
  TableIcon,
} from "../ui/icons";
import { readDocument, readFileBytes, summarizeDocument, saveSummaryAsMarkdown, type DocumentContent } from "../../api/document";
import { analyzeDocument } from "../../api/analysis";
import { MarkdownPreview } from "./MarkdownPreview";
import { useRuntimeConfig } from "../settings/runtime-config";
import { useNavStore } from "../../state/navStore";
import type { Project } from "../../types";
import { PDFViewer } from "./PDFViewer";
import { listen } from "../../api";

interface DocumentViewerProps {
  project: Project;
  filePath: string;
  fileName: string;
  onClose: () => void;
  onSummarize?: (summary: string) => void;
}

export function DocumentViewer({ project, filePath, fileName, onClose, onSummarize }: DocumentViewerProps) {
  const [content, setContent] = useState<DocumentContent | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [summary, setSummary] = useState<string | null>(null);
  const [summarizing, setSummarizing] = useState(false);
  const [saving, setSaving] = useState(false);
  const [recovering, setRecovering] = useState(false);
  const [summaryError, setSummaryError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<"content" | "summary">("content");
  const [summaryKind, setSummaryKind] = useState<"summary" | "tables" | "excerpt">("summary");
  const [currentPage, setCurrentPage] = useState<number | null>(null);
  const [pdfBytes, setPdfBytes] = useState<string | null>(null);
  /** Page total as rendered by pdf.js — more reliable than the extractor's
   * estimate, so the header prefers it once the viewer has loaded. */
  const [pdfPageTotal, setPdfPageTotal] = useState<number | null>(null);
  /** Track summarization phase: "extracting" | "summarizing" | "idle" */
  const [summarizePhase, setSummarizePhase] = useState<"extracting" | "summarizing" | "idle">("idle");

  const { draft: profile } = useRuntimeConfig();
  const insertIntoComposer = useNavStore((state) => state.insertIntoComposer);
  const setActiveProjectTab = useNavStore((state) => state.setActiveProjectTab);

  const ext = fileName.split(".").pop()?.toLowerCase() ?? "";
  const isPDF = ext === "pdf";
  const canSummarize = !loading && (isPDF ? Boolean(content || pdfBytes) : Boolean(content?.text?.trim()));

  const handlePdfPageChange = useCallback(
    (page: number, totalPages: number) => {
      setCurrentPage(page);
      setPdfPageTotal(totalPages);
    },
    [],
  );

  /** Derive the expected summary path for a document: data/foo.pdf -> summary/foo.md */
  const summaryPathFor = useCallback((filePath: string): string => {
    const sourceName = filePath.split("/").pop() ?? filePath;
    const dotIndex = sourceName.lastIndexOf(".");
    const stem = dotIndex > 0 ? sourceName.slice(0, dotIndex) : sourceName;
    return `summary/${stem}.md`;
  }, []);

  /** When a document is opened, check whether a summary already exists alongside it
   * (summary/<stem>.md) and auto-load it so the summary is accessible next to the source.
   * Only runs when no summarisation is currently in progress so it cannot race
   * with a freshly generated summary. */
  const loadExistingSummary = useCallback(async () => {
    if (summarizing) return;
    const path = summaryPathFor(filePath);
    try {
      const doc = await readDocument(project.id, path, true);
      if (doc?.text?.trim()) {
        setSummary(doc.text);
        setSummaryKind("summary");
        setActiveTab("summary");
      }
    } catch {
      // No existing summary — that's fine, the user can generate one.
    }
  }, [project.id, filePath, summaryPathFor, summarizing]);

  const loadDocument = useCallback(async () => {
    setLoading(true);
    setError(null);
    setPdfPageTotal(null);
    try {
      console.log("Loading document:", { projectId: project.id, filePath });
      
      let doc: DocumentContent | null = null;
      try {
        // Initially load without text extraction for PDFs (visual rendering only)
        doc = await readDocument(project.id, filePath, false);
        console.log("Document loaded:", doc);
        setContent(doc);
      } catch (err) {
        // read_document may fail if pdf-extract can't parse the PDF text,
        // but the file bytes may still be readable for visual rendering.
        console.warn("read_document failed, trying raw bytes only:", err);
        // Don't set error yet — try loading bytes for PDF visual rendering
      }

      // For PDFs, always try to load bytes for visual rendering
      if (isPDF) {
        try {
          let bytes: string | null = null;
          if (doc?.fileBytesBase64) {
            bytes = doc.fileBytesBase64;
          } else {
            bytes = await readFileBytes(project.id, filePath);
          }
          setPdfBytes(bytes);
        } catch (e) {
          console.warn("Could not load PDF bytes:", e);
          if (!doc) {
            // Both read_document and read_file_bytes failed
            const msg = e instanceof Error ? e.message : "Failed to load PDF";
            setError(msg);
          }
        }
      }

      if (!doc && !isPDF) {
        // Non-PDF and read_document failed — show error
        const msg = "Failed to load document";
        setError(msg);
      }

      // If a summary already exists alongside this document, load it so the
      // user can access it next to the source file.
      await loadExistingSummary();
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Failed to load document";
      console.error("Failed to load document:", err);
      setError(msg);
    } finally {
      setLoading(false);
    }
  }, [project.id, filePath, fileName]);

  useEffect(() => {
    loadDocument();
  }, [loadDocument]);

  const handleSummarize = async () => {
    // Guard: require document to be loaded and have text (or be a loadable PDF).
    // Note: for PDFs, text is intentionally not extracted during initial document load
    // to avoid UI blocking; the backend extracts PDF text on demand during summarizeDocument.
    if (!canSummarize) return;

    setSummarizing(true);
    setSummaryError(null);
    setSummary(null);
    setSummarizePhase("idle");

    // Listen for extraction_done event from backend (PDF only)
    let unlisten: (() => void) | null = null;
    if (isPDF) {
      setSummarizePhase("extracting");
      unlisten = await listen("summarize:extraction_done", () => {
        setSummarizePhase("summarizing");
      });
    } else {
      setSummarizePhase("summarizing");
    }

    try {
      const result = await summarizeDocument({
        projectId: project.id,
        filePath,
        profile,
      });
      setSummary(result.summary);
      setSummaryKind("summary");
      if (onSummarize) onSummarize(result.summary);
    } catch (firstErr) {
      // Direct (single-shot) summarization failed — most commonly because the
      // document is longer than the model's context window. Retry through the
      // map-reduce analysis pipeline, which chunks the document, summarises
      // each chunk, and synthesises the results, so long documents still get a
      // summary instead of a dead end.
      //
      // Use `filePath` (the full relative path, e.g. "data/report.pdf"), not
      // `fileName` (the bare name), so the backend sandbox-path check resolves
      // the file correctly for documents that are not in the project root.
      try {
        const result = await analyzeDocument({
          projectId: project.id,
          action: "summarize",
          fileName: filePath,
        });
        setSummary(result.content);
        setSummaryKind("summary");
        setSummaryError(null);
        if (onSummarize) onSummarize(result.content);
      } catch (mapReduceErr) {
        setSummaryError(mapReduceErr instanceof Error ? mapReduceErr.message : "Failed to summarize document");
      }
    } finally {
      setSummarizing(false);
      setSummarizePhase("idle");
      if (unlisten) unlisten();
      setActiveTab("summary");
    }
  };

  const handleSaveMarkdown = async () => {
    if (!summary) return;
    setSaving(true);
    try {
      await saveSummaryAsMarkdown(project.id, filePath, summary);
    } catch (err) {
      console.error("Manual save summary failed:", err);
      setSummaryError(err instanceof Error ? err.message : "Failed to save summary");
    } finally {
      setSaving(false);
    }
  };

  // Convert base64 to File object for PDFViewer.
  // Memoized: recreating the File object on every render would change the
  // identity the PDFViewer keys its load effect on, so any unrelated parent
  // render (e.g. the runtime status poll) used to make it reload and
  // flicker the whole document.
  // The decode is guarded: useMemo runs during render, so a throwing atob
  // (bad base64) here would unmount the entire React tree. A failure simply
  // yields null and is derived into a visible error below.
  const pdfFile = useMemo(
    () => {
      if (!pdfBytes) return null;
      try {
        return base64ToFile(pdfBytes, fileName);
      } catch (err) {
        console.error("Failed to decode PDF bytes:", err);
        return null;
      }
    },
    [pdfBytes, fileName],
  );
  const pdfDecodeFailed = Boolean(pdfBytes) && pdfFile === null;

  /** Queue a citation for this document in the chat composer. */
  const handleCiteInChat = () => {
    const location = isPDF && currentPage ? `, p. ${currentPage}` : "";
    const excerpt = (summary ?? content?.text ?? "").replace(/\s+/g, " ").trim();
    const lines = [`> **${fileName}**${location}`];
    if (excerpt) {
      lines.push(">", `> ${excerpt.slice(0, 240)}${excerpt.length > 240 ? "…" : ""}`, ">");
    }
    insertIntoComposer(`${lines.join("\n")}\n\n`);
    setActiveProjectTab("chat");
  };

  /** Pull key figures and tables out through the analysis pipeline. */
  const handleExtractTables = async () => {
    if (!canSummarize) return;
    setSummarizing(true);
    setSummaryError(null);
    try {
      const result = await analyzeDocument({
        projectId: project.id,
        action: "extract_tables",
        fileName: filePath,
        saveAs: `tables-${fileName.replace(/[^a-zA-Z0-9]/g, "_")}.md`,
      });
      setSummary(result.content);
      setSummaryKind("tables");
      setActiveTab("summary");
    } catch (err) {
      setSummaryError(err instanceof Error ? err.message : "Failed to extract tables");
      setActiveTab("summary");
    } finally {
      setSummarizing(false);
    }
  };

  /** Fallback when direct summarization fails: chunk the document via the
   *  map-reduce analysis command instead.
   *
   *  Uses `filePath` (full relative path) not `fileName` (bare name) so the
   *  backend sandbox check resolves correctly for sub-directory files.
   *  Does NOT pass `saveAs` here — the backend's `summarize_document` already
   *  writes to summary/<stem>.md, which `loadExistingSummary` picks up. */
  const handleMapReduce = async () => {
    setRecovering(true);
    try {
      const result = await analyzeDocument({
        projectId: project.id,
        action: "summarize",
        fileName: filePath,
      });
      setSummary(result.content);
      setSummaryKind("summary");
      setSummaryError(null);
      setActiveTab("summary");
    } catch (err) {
      setSummaryError(err instanceof Error ? err.message : "Map-reduce analysis failed");
    } finally {
      setRecovering(false);
    }
  };

  /** Last resort: show the extracted text so the document is still readable. */
  const handleUseExcerpt = () => {
    const text = content?.text?.trim();
    if (!text) return;
    const clipped = text.length > 6000 ? `${text.slice(0, 6000)}\n\n…` : text;
    setSummary(`## Document excerpt\n\n${clipped}`);
    setSummaryKind("excerpt");
    setSummaryError(null);
    setActiveTab("summary");
  };

  // Header info: real file size from the backend, page count from pdf.js when
  // available (falling back to the extractor's estimate). Shown as soon as
  // either is known, even if text extraction failed entirely.
  const displayedInfo = (() => {
    const parts: string[] = [];
    if (content?.sizeBytes !== undefined) {
      parts.push(`${content.sizeBytes.toLocaleString()} bytes`);
    } else if (pdfBytes) {
      // Extraction failed but the viewer has the bytes — derive the size
      // from base64 (3 bytes per 4 chars; ≈2 bytes of padding slop).
      const approxBytes = Math.floor((pdfBytes.length * 3) / 4);
      parts.push(`${approxBytes.toLocaleString()} bytes`);
    }
    const pages = isPDF ? (pdfPageTotal ?? content?.pageCount) : content?.pageCount;
    if (pages) parts.push(`${pages} pages`);
    return parts.length > 0 ? parts.join(" · ") : null;
  })();

  function base64ToFile(base64: string, filename: string): File {
    const byteCharacters = atob(base64);
    const byteNumbers = new Array(byteCharacters.length);
    for (let i = 0; i < byteCharacters.length; i++) {
      byteNumbers[i] = byteCharacters.charCodeAt(i);
    }
    return new File([new Uint8Array(byteNumbers)], filename, { type: "application/pdf" });
  }

  return (
    <div className="document-viewer" style={{ display: "flex", flexDirection: "column", height: "100%", background: "var(--background)", borderLeft: "1px solid var(--border)" }}>
      {/* Header */}
      <div style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "10px 14px",
        background: "var(--background-elevated)",
        borderBottom: "1px solid var(--border)",
      }}>
        <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
          <button
            type="button"
            className="icon-btn icon-btn--sm"
            onClick={onClose}
            title="Close viewer"
            aria-label="Close document viewer"
          >
            <ChevronLeftIcon size={18} />
          </button>
          <DocIcon size={20} style={{ color: isPDF ? "#ef4444" : "var(--accent-primary)" }} />
          <div>
            <div style={{ fontWeight: "500", fontSize: "13px", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", maxWidth: "280px" }}>
              {fileName}
            </div>
            <div style={{ fontSize: "11px", color: "var(--text-muted)" }}>
              {displayedInfo || "Loading…"}
            </div>
          </div>
        </div>

        <div className="doc-toolbar">
          <button
            type="button"
            className="btn btn--subtle btn--sm"
            onClick={handleCiteInChat}
            disabled={isPDF && !content}
            title={isPDF && !content ? "Load document first to cite" : "Insert a citation for this document into the chat composer"}
          >
            <QuoteIcon size={13} />
            <span>Cite in Chat</span>
          </button>

          <button
            type="button"
            className="btn btn--subtle btn--sm"
            onClick={() => void handleExtractTables()}
            disabled={!canSummarize}
            title={!canSummarize ? (isPDF ? "Load document first" : "Document text not available") : "Extract key figures and tables from this document"}
          >
            <TableIcon size={13} />
            <span>{loading ? "Loading…" : "Key figures & tables"}</span>
          </button>

          {!summarizing && !summary && (
            <button
              type="button"
              className="btn btn--primary btn--sm"
              onClick={handleSummarize}
              disabled={!canSummarize}
              title={
                !canSummarize
                  ? isPDF
                    ? "Load PDF first to summarize"
                    : "Document text not available for summarization"
                  : "Generate AI summary"
              }
            >
              <SparkIcon size={13} />
              <span>Summarize</span>
            </button>
          )}

          {summarizing && (
            <button type="button" className="btn btn--primary btn--sm" disabled>
              <div
                style={{
                  width: "13px",
                  height: "13px",
                  borderRadius: "50%",
                  border: "2px solid var(--border-strong)",
                  borderTopColor: "var(--accent-primary)",
                  animation: "spin 0.7s linear infinite",
                }}
              />
              <span>Summarizing…</span>
            </button>
          )}

          {summary && (
            <>
              <button
                type="button"
                className="btn btn--subtle btn--sm"
                onClick={() => {
                  handleSaveMarkdown();
                }}
                disabled={saving}
                title={saving ? "Saving…" : "Save summary as markdown file"}
              >
                {saving ? (
                  <div
                    style={{
                      width: "13px",
                      height: "13px",
                      borderRadius: "50%",
                      border: "2px solid var(--border-strong)",
                      borderTopColor: "var(--accent-primary)",
                      animation: "spin 0.7s linear infinite",
                    }}
                  />
                ) : (
                  <DocIcon size={13} />
                )}
                <span style={{ marginLeft: "4px", fontSize: "11px" }}>
                  {saving ? "Saving…" : "Save .md"}
                </span>
              </button>
            </>
          )}

          <button
            type="button"
            className="icon-btn icon-btn--sm"
            onClick={onClose}
            title="Close"
            aria-label="Close document viewer"
          >
            <XIcon size={16} />
          </button>
        </div>
      </div>

      {/* Tab Bar */}
      <div style={{
        display: "flex",
        background: "var(--surface)",
        borderBottom: "1px solid var(--border)",
        padding: "0 8px",
      }}>
        <button
          type="button"
          role="tab"
          aria-selected={activeTab === "content"}
          onClick={() => setActiveTab("content")}
          style={{
            padding: "8px 16px",
            fontSize: "12px",
            fontWeight: activeTab === "content" ? "600" : "500",
            color: activeTab === "content" ? "var(--accent-primary)" : "var(--text-secondary)",
            background: "transparent",
            border: "none",
            borderBottom: activeTab === "content" ? "2px solid var(--accent-primary)" : "2px solid transparent",
            cursor: "pointer",
            marginBottom: "-1px",
          }}
        >
          Document
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={activeTab === "summary"}
          onClick={() => setActiveTab("summary")}
          disabled={!summary && !summarizing}
          style={{
            padding: "8px 16px",
            fontSize: "12px",
            fontWeight: activeTab === "summary" ? "600" : "500",
            color: activeTab === "summary" ? "var(--accent-primary)" : summary ? "var(--text-secondary)" : "var(--text-faint)",
            background: "transparent",
            border: "none",
            borderBottom: activeTab === "summary" ? "2px solid var(--accent-primary)" : "2px solid transparent",
            cursor: summary || summarizing ? "pointer" : "not-allowed",
            marginBottom: "-1px",
            opacity: summary || summarizing ? 1 : 0.5,
          }}
        >
          <SparkIcon size={12} style={{ display: "inline-block", marginRight: "4px", verticalAlign: "middle" }} />
          {summaryKind === "tables" ? "Figures & tables" : summaryKind === "excerpt" ? "Excerpt" : "Summary"}
        </button>
      </div>

      {/* Content Area */}
      <div style={{ flex: 1, overflow: "auto", padding: "16px" }}>
        {loading ? (
          <div style={{
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center",
            height: "200px",
            color: "var(--text-muted)",
            gap: "12px",
          }}>
            <div
              style={{
                width: "32px",
                height: "32px",
                borderRadius: "50%",
                border: "2px solid var(--border-strong)",
                borderTopColor: "var(--accent-primary)",
                animation: "spin 0.7s linear infinite",
              }}
            />
            <span>Loading document…</span>
          </div>
        ) : error ? (
          <div style={{
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center",
            height: "200px",
            color: "#ef4444",
            textAlign: "center",
            padding: "24px",
          }}>
            <DocIcon size={32} style={{ color: "#ef4444", marginBottom: "12px" }} />
            <h3 style={{ fontSize: "14px", fontWeight: "600", marginBottom: "8px" }}>Failed to load document</h3>
            <p style={{ fontSize: "12px", color: "var(--text-muted)" }}>{error}</p>
          </div>
        ) : activeTab === "content" ? (
          content ? (
            isPDF && pdfDecodeFailed ? (
              // Bad base64 — show a controlled error instead of crashing.
              <div style={{
                display: "flex",
                flexDirection: "column",
                alignItems: "center",
                justifyContent: "center",
                height: "200px",
                color: "#ef4444",
                textAlign: "center",
                padding: "24px",
              }}>
                <DocIcon size={32} style={{ color: "#ef4444", marginBottom: "12px" }} />
                <h3 style={{ fontSize: "14px", fontWeight: "600", marginBottom: "8px" }}>Could not display PDF</h3>
                <p style={{ fontSize: "12px", color: "var(--text-muted)" }}>
                  The document viewer could not decode the PDF file data. Text extraction still worked;
                  see the console for details.
                </p>
              </div>
            ) : isPDF && pdfFile ? (
              // Visual PDF rendering for PDF files
              <PDFViewer file={pdfFile} onPageChange={handlePdfPageChange} />
            ) : (
              // Text content for non-PDF files
              <div style={{
                fontSize: "13px",
                lineHeight: 1.6,
                fontFamily: "var(--font-mono)",
                color: "var(--text)",
                whiteSpace: "pre-wrap",
                wordWrap: "break-word",
                maxHeight: "100%",
                overflow: "auto",
              }}>
                {content.text}
              </div>
            )
          ) : (
            <div style={{
              display: "flex",
              flexDirection: "column",
              alignItems: "center",
              justifyContent: "center",
              height: "200px",
              color: "var(--text-muted)",
              textAlign: "center",
              padding: "24px",
            }}>
              <DocIcon size={32} style={{ color: "var(--text-muted)", marginBottom: "12px" }} />
              <h3 style={{ fontSize: "14px", fontWeight: "600", marginBottom: "8px" }}>No content loaded</h3>
              <p style={{ fontSize: "12px", color: "var(--text-muted)" }}>Document content is empty. Check console for errors.</p>
            </div>
          )
        ) : summary ? (
          <div style={{
            lineHeight: 1.7,
            color: "var(--text)",
            maxHeight: "100%",
            overflow: "auto",
          }}>
            <MarkdownPreview markdown={summary} />
          </div>
        ) : summarizing ? (
          <div style={{
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center",
            height: "200px",
            color: "var(--text-muted)",
            gap: "12px",
          }}>
            <div
              style={{
                width: "32px",
                height: "32px",
                borderRadius: "50%",
                border: "2px solid var(--border-strong)",
                borderTopColor: "var(--accent-primary)",
                animation: "spin 0.7s linear infinite",
              }}
            />
            <span>{summarizePhase === "extracting" ? "Extracting PDF text…" : "Summarization in progress…"}</span>
            <p style={{ fontSize: "12px", color: "var(--text-faint)" }}>
              Using model context: {profile.contextTokens || "default"} tokens
            </p>
          </div>
        ) : summaryError ? null : (
          <div style={{
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center",
            height: "200px",
            color: "var(--text-muted)",
            textAlign: "center",
            gap: "16px",
            padding: "24px",
          }}>
            <SparkIcon size={48} style={{ opacity: 0.5 }} />
            <h3 style={{ fontSize: "15px", fontWeight: "600", color: "var(--text)" }}>
              No summary yet
            </h3>
            <p style={{ fontSize: "13px", maxWidth: "320px" }}>
              Click <strong>Summarize</strong> to generate an AI-powered summary of this document using your configured model settings.
            </p>
          </div>
        )}

        {summaryError && (
          <div className="doc-recovery" style={{ marginTop: 16 }}>
            <div className="doc-recovery__title">
              <SparkIcon size={14} />
              Couldn’t summarize this document
            </div>
            <p className="doc-recovery__cause">
              The model ran out of room or the request failed. You can summarize
              {" "}<code>{fileName}</code>{" "}in chunks through the map-reduce
              analysis pipeline instead, or read the extracted text directly.
            </p>
            <div className="doc-recovery__actions">
              <button
                type="button"
                className="btn btn--primary btn--sm"
                onClick={() => void handleMapReduce()}
                disabled={recovering}
              >
                <SparkIcon size={13} />
                {recovering ? "Running…" : "Run via Map-Reduce Analysis"}
              </button>
              <button
                type="button"
                className="btn btn--secondary btn--sm"
                onClick={handleUseExcerpt}
                disabled={!content?.text}
              >
                <DocIcon size={13} />
                Use document excerpt instead
              </button>
              <button
                type="button"
                className="btn btn--ghost btn--sm"
                onClick={() => setSummaryError(null)}
              >
                Dismiss
              </button>
            </div>
            <div className="doc-recovery__detail">Details: {summaryError}</div>
          </div>
        )}
      </div>
    </div>
  );
}