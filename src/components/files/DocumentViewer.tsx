/** Document Viewer Panel — displays document content with AI summarization. */

import { useEffect, useState, useCallback } from "react";
import {
  SparkIcon,
  XIcon,
  CopyIcon,
  ChevronLeftIcon,
  DocIcon,
} from "../ui/icons";
import { readDocument, readFileBytes, summarizeDocument, saveSummaryAsMarkdown, type DocumentContent } from "../../api/document";
import { useRuntimeConfig } from "../settings/runtime-config";
import type { Project } from "../../types";
import { PDFViewer } from "./PDFViewer";

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
  const [savedFile, setSavedFile] = useState<{ fileName: string; savedPath: string } | null>(null);
  const [summaryError, setSummaryError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<"content" | "summary">("content");
  const [pdfBytes, setPdfBytes] = useState<string | null>(null);

  const { draft: profile } = useRuntimeConfig();

  const loadDocument = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      console.log("Loading document:", { projectId: project.id, filePath });
      
      let doc: DocumentContent | null = null;
      try {
        doc = await readDocument(project.id, filePath);
        console.log("Document loaded:", doc);
        setContent(doc);
      } catch (err) {
        // read_document may fail if pdf-extract can't parse the PDF text,
        // but the file bytes may still be readable for visual rendering.
        console.warn("read_document failed, trying raw bytes only:", err);
        // Don't set error yet — try loading bytes for PDF visual rendering
      }

      // For PDFs, always try to load bytes for visual rendering
      const isPDF = fileName.toLowerCase().endsWith(".pdf");
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
    if (!content) return;

    setSummarizing(true);
    setSummaryError(null);
    setSummary(null);

    try {
      const result = await summarizeDocument({
        projectId: project.id,
        filePath,
        profile,
      });
      setSummary(result.summary);
      if (onSummarize) onSummarize(result.summary);
    } catch (err) {
      setSummaryError(err instanceof Error ? err.message : "Failed to summarize document");
    } finally {
      setSummarizing(false);
      setActiveTab("summary");
    }
  };

  const handleCopyContent = async () => {
    if (!content) return;
    await navigator.clipboard.writeText(content.text);
  };

  const handleCopySummary = async () => {
    if (!summary) return;
    await navigator.clipboard.writeText(summary);
  };

  const handleSaveMarkdown = async () => {
    if (!summary) return;
    setSaving(true);
    try {
      const result = await saveSummaryAsMarkdown(project.id, filePath, summary);
      setSavedFile(result);
    } catch (err) {
      setSummaryError(err instanceof Error ? err.message : "Failed to save summary");
    } finally {
      setSaving(false);
    }
  };

  const ext = fileName.split(".").pop()?.toLowerCase() ?? "";
  const isPDF = ext === "pdf";
  
  // Convert base64 to File object for PDFViewer
  const pdfFile = pdfBytes ? base64ToFile(pdfBytes, fileName) : null;

  function base64ToFile(base64: string, filename: string): File {
    const byteCharacters = atob(base64);
    const byteNumbers = new Array(byteCharacters.length);
    for (let i = 0; i < byteCharacters.length; i++) {
      byteNumbers[i] = byteCharacters.charCodeAt(i);
    }
    const byteArray = new Uint8Array(byteNumbers);
    return new File([byteArray], filename, { type: "application/pdf" });
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
              {content ? `${content.sizeBytes.toLocaleString()} bytes${content.pageCount ? ` · ${content.pageCount} pages` : ""}` : "Loading…"}
            </div>
          </div>
        </div>

        <div style={{ display: "flex", alignItems: "center", gap: "6px" }}>
          {!summarizing && !summary && (
            <button
              type="button"
              className="btn btn--primary btn--sm"
              onClick={handleSummarize}
              disabled={loading || !content}
              title="Generate AI summary"
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
                onClick={handleCopySummary}
                title="Copy summary"
              >
                <CopyIcon size={13} />
              </button>
              {!saving && !savedFile ? (
                <button
                  type="button"
                  className="btn btn--subtle btn--sm"
                  onClick={handleSaveMarkdown}
                  title="Save summary as markdown file"
                >
                  <DocIcon size={13} />
                  <span style={{ marginLeft: "4px", fontSize: "11px" }}>Save .md</span>
                </button>
              ) : saving ? (
                <button type="button" className="btn btn--subtle btn--sm" disabled>
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
                </button>
              ) : (
                <button
                  type="button"
                  className="btn btn--subtle btn--sm"
                  onClick={() => {
                    alert(`Saved as: ${savedFile?.savedPath}`);
                  }}
                  title="Saved"
                >
                  <DocIcon size={13} style={{ color: "var(--accent-success)" }} />
                </button>
              )}
              <button
                type="button"
                className="btn btn--subtle btn--sm"
                onClick={() => setActiveTab("content")}
                title="View document content"
              >
                <DocIcon size={13} />
              </button>
            </>
          )}

          {!summarizing && content && (
            <button
              type="button"
              className="btn btn--subtle btn--sm"
              onClick={handleCopyContent}
              title="Copy document text"
            >
              <CopyIcon size={13} />
            </button>
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
          Summary
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
            isPDF && pdfFile ? (
              // Visual PDF rendering for PDF files
              <PDFViewer file={pdfFile} />
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
            fontSize: "13px",
            lineHeight: 1.7,
            color: "var(--text)",
            whiteSpace: "pre-wrap",
            wordWrap: "break-word",
            maxHeight: "100%",
            overflow: "auto",
          }}>
            {summary}
            {savedFile && (
              <div style={{ marginTop: "16px", padding: "12px", background: "rgba(34, 197, 94, 0.1)", border: "1px solid rgba(34, 197, 94, 0.3)", borderRadius: "var(--radius-md)", color: "var(--accent-success)", fontSize: "12px", display: "flex", alignItems: "center", gap: "8px" }}>
                <DocIcon size={14} style={{ color: "var(--accent-success)" }} />
                <span>Saved as markdown: <strong>{savedFile.fileName}</strong></span>
              </div>
            )}
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
            <span>Generating summary with AI…</span>
            <p style={{ fontSize: "12px", color: "var(--text-faint)" }}>
              Using model context: {profile.contextTokens || "default"} tokens
            </p>
          </div>
        ) : (
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
          <div style={{
            marginTop: "16px",
            padding: "12px",
            background: "rgba(220, 38, 38, 0.1)",
            border: "1px solid rgba(220, 38, 38, 0.3)",
            borderRadius: "var(--radius-md)",
            color: "#f87171",
            fontSize: "12px",
          }}>
            Error: {summaryError}
          </div>
        )}
      </div>
    </div>
  );
}