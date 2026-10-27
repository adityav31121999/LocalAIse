/** PDF Viewer Component — renders PDF pages visually using pdf.js. */

import { useCallback, useEffect, useRef, useState, memo } from "react";
import * as pdfjsLib from "pdfjs-dist";

/** pdf.js needs a worker URL to parse off the main thread. */
declare module "pdfjs-dist/build/pdf.worker.mjs?url";

/** pdf.js rejects a cancelled page.render() with this named error. */
function isRenderCancelled(err: unknown): boolean {
  return (
    typeof err === "object" &&
    err !== null &&
    (err as { name?: string }).name === "RenderingCancelledException"
  );
}

interface PDFViewerProps {
  /** File to render (PDF). */
  file: File;
  /** Initial page to show (1-indexed). */
  initialPage?: number;
  /** Callback when page changes. */
  onPageChange?: (page: number, totalPages: number) => void;
  /** Scale factor for rendering. 1.0 = 72 DPI. */
  scale?: number;
  /** Maximum canvas dimensions to prevent memory issues. */
  maxCanvasSize?: number;
}

interface PageError {
  page: number;
  message: string;
}

export const PDFViewer = memo(function PDFViewer({ file, initialPage = 1, onPageChange, scale = 1.5, maxCanvasSize = 4096 }: PDFViewerProps) {
  const [pdf, setPdf] = useState<pdfjsLib.PDFDocumentProxy | null>(null);
  const [currentPage, setCurrentPage] = useState(initialPage);
  const [totalPages, setTotalPages] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [pageErrors, setPageErrors] = useState<PageError[]>([]);
  const [workerReady, setWorkerReady] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const pageCanvasRefs = useRef<Map<number, HTMLCanvasElement>>(new Map());
  const renderTasksRef = useRef<Map<number, pdfjsLib.RenderTask>>(new Map());
  /** True after the first successful load; later reloads of the same view
   * keep the already-rendered pages visible instead of flashing a spinner. */
  const loadedOnceRef = useRef(false);

  // Prop mirrors, so the callbacks below can read them without being keyed
  // on them. The load effect must only depend on the actual document input.
  const initialPageRef = useRef(initialPage);
  initialPageRef.current = initialPage;
  const currentPageRef = useRef(currentPage);
  currentPageRef.current = currentPage;
  const onPageChangeRef = useRef(onPageChange);
  onPageChangeRef.current = onPageChange;

  // Initialize pdf.js worker
  useEffect(() => {
    const initWorker = async () => {
      try {
        const { default: pdfWorkerUrl } = await import("pdfjs-dist/build/pdf.worker.mjs?url");
        pdfjsLib.GlobalWorkerOptions.workerSrc = pdfWorkerUrl as string;
        setWorkerReady(true);
      } catch (err) {
        console.error("Failed to initialize pdf.js worker:", err);
        setError("Failed to initialize PDF renderer");
      }
    };
    initWorker();
  }, []);

  // Load PDF document. Deliberately keyed on [file, workerReady] only:
  // page navigation must not re-parse the document, and an unstable
  // callback prop must not make this re-run via an effect dependency.
  const loadPdf = useCallback(async () => {
    if (!workerReady) return;
    if (!loadedOnceRef.current) setLoading(true);
    setError(null);
    setPageErrors([]);
    try {
      const arrayBuffer = await file.arrayBuffer();
      // Self-contained rendering: pdf.js needs its bundled standard-font data
      // (for documents using non-embedded base-14 fonts like Times/Helvetica —
      // common in LaTeX PDFs) and cmaps (for CJK text). Both are vendored in
      // public/pdfjs/ so dev and the packaged app fetch them from the frontend
      // origin itself, never from a CDN.
      const assetBase = `${import.meta.env.BASE_URL}pdfjs/`;
      const doc = await pdfjsLib.getDocument({
        data: arrayBuffer,
        standardFontDataUrl: `${assetBase}standard_fonts/`,
        cMapUrl: `${assetBase}cmaps/`,
        cMapPacked: true,
      }).promise;
      setPdf(doc);
      setTotalPages(doc.numPages);
      // Start (or re-start) at the requested page, clamped to the document.
      const target = Math.max(1, Math.min(initialPageRef.current, doc.numPages));
      setCurrentPage(target);
      onPageChangeRef.current?.(target, doc.numPages);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load PDF");
    } finally {
      loadedOnceRef.current = true;
      setLoading(false);
    }
  }, [file, workerReady]);

  useEffect(() => {
    loadPdf();
  }, [loadPdf]);

  // Render a single page to canvas
  const renderPage = useCallback(async (pageNum: number, canvas: HTMLCanvasElement) => {
    if (!pdf) return;
    try {
      const page = await pdf.getPage(pageNum);
      const viewport = page.getViewport({ scale });

      // Clamp to max size
      let outputScale = 1;
      if (viewport.width > maxCanvasSize || viewport.height > maxCanvasSize) {
        outputScale = Math.min(maxCanvasSize / viewport.width, maxCanvasSize / viewport.height);
      }
      const outputViewport = page.getViewport({ scale: scale * outputScale });

      // Browsers limit canvas dimensions; clamp to a safe max
      const MAX_SAFE_DIM = 16384;
      if (outputViewport.width > MAX_SAFE_DIM || outputViewport.height > MAX_SAFE_DIM) {
        const safeScale = Math.min(MAX_SAFE_DIM / outputViewport.width, MAX_SAFE_DIM / outputViewport.height);
        outputViewport.scale *= safeScale;
        outputViewport.width *= safeScale;
        outputViewport.height *= safeScale;
        outputViewport.transform = page.getViewport({ scale: scale * outputScale * safeScale }).transform;
      }

      canvas.width = Math.floor(outputViewport.width);
      canvas.height = Math.floor(outputViewport.height);

      const context = canvas.getContext("2d");
      if (!context) throw new Error("Canvas context unavailable");

      // Fill with white background first (PDF pages may have transparent backgrounds)
      context.fillStyle = "white";
      context.fillRect(0, 0, canvas.width, canvas.height);

      // High-quality rendering
      context.imageSmoothingEnabled = true;
      context.imageSmoothingQuality = "high";

      const task = page.render({ canvas, canvasContext: context, viewport: outputViewport });
      renderTasksRef.current.set(pageNum, task);
      await task.promise;
      if (renderTasksRef.current.get(pageNum) === task) {
        renderTasksRef.current.delete(pageNum);
      }

      // Remove this page from error list if it was previously failing
      setPageErrors((prev) => prev.filter((e) => e.page !== pageNum));
    } catch (err) {
      // A superseded or unmounted render is expected — never report it as
      // a page failure.
      if (isRenderCancelled(err)) return;
      const msg = err instanceof Error ? err.message : "Unknown render error";
      console.error(`Failed to render page ${pageNum}:`, err);
      setPageErrors((prev) => {
        const existing = prev.find((e) => e.page === pageNum);
        if (existing) return prev;
        return [...prev, { page: pageNum, message: msg }];
      });
    }
  }, [pdf, scale, maxCanvasSize]);

  // Render all pages when the PDF loads or its scale changes
  useEffect(() => {
    if (!pdf || !containerRef.current) return;

    const container = containerRef.current;
    container.innerHTML = ""; // Clear previous renders
    pageCanvasRefs.current.clear();

    // Races the previous pass: any render still in flight belongs to a
    // canvas that is about to be dropped.
    renderTasksRef.current.forEach((task) => task.cancel());
    renderTasksRef.current.clear();

    const pageNumbers = Array.from({ length: totalPages }, (_, i) => i + 1);

    pageNumbers.forEach(async (pageNum) => {
      const canvas = document.createElement("canvas");
      canvas.className = "pdf-page-canvas";
      canvas.style.width = "100%";
      canvas.style.height = "auto";
      canvas.style.display = "block";
      canvas.style.marginBottom = "16px";
      canvas.style.boxShadow = "0 2px 8px rgba(0,0,0,0.1)";
      canvas.style.background = "white";
      container.appendChild(canvas);
      pageCanvasRefs.current.set(pageNum, canvas);
      await renderPage(pageNum, canvas);
    });

    // Drop this pass's renders when the view changes or the component unmounts,
    // so a slow page render cannot write into a canvas nobody displays.
    return () => {
      renderTasksRef.current.forEach((task) => task.cancel());
      renderTasksRef.current.clear();
    };
  }, [pdf, totalPages, renderPage]);

  // Handle page navigation
  const goToPage = (page: number) => {
    if (page < 1 || page > totalPages) return;
    setCurrentPage(page);
    onPageChange?.(page, totalPages);
    
    // Scroll to the page
    const canvas = pageCanvasRefs.current.get(page);
    canvas?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  const goToPrevPage = () => goToPage(currentPage - 1);
  const goToNextPage = () => goToPage(currentPage + 1);

  const retryPage = (page: number) => {
    const canvas = pageCanvasRefs.current.get(page);
    if (canvas) {
      renderPage(page, canvas);
    }
  };

  if (!workerReady) {
    return (
      <div className="pdf-viewer" style={{ display: "flex", flexDirection: "column", height: "100%" }}>
        <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--text-muted)" }}>
          <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: "12px" }}>
            <div style={{ width: "32px", height: "32px", borderRadius: "50%", border: "2px solid var(--border-strong)", borderTopColor: "var(--accent-primary)", animation: "spin 0.7s linear infinite" }} />
            <span>Initializing PDF renderer…</span>
          </div>
        </div>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="pdf-viewer" style={{ display: "flex", flexDirection: "column", height: "100%" }}>
        <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--text-muted)" }}>
          <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: "12px" }}>
            <div style={{ width: "32px", height: "32px", borderRadius: "50%", border: "2px solid var(--border-strong)", borderTopColor: "var(--accent-primary)", animation: "spin 0.7s linear infinite" }} />
            <span>Loading PDF…</span>
          </div>
        </div>
      </div>
    );
  }

  if (error && !pdf) {
    return (
      <div className="pdf-viewer" style={{ display: "flex", flexDirection: "column", height: "100%" }}>
        <div style={{ flex: 1, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", color: "#ef4444", textAlign: "center", padding: "24px" }}>
          <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" style={{ marginBottom: "12px" }}>
            <path d="M12 9v4M12 17h.01" />
            <circle cx="12" cy="12" r="10" />
          </svg>
          <h3 style={{ fontSize: "14px", fontWeight: "600", marginBottom: "8px" }}>Failed to load PDF</h3>
          <p style={{ fontSize: "12px", color: "var(--text-muted)" }}>{error}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="pdf-viewer" style={{ display: "flex", flexDirection: "column", height: "100%", background: "var(--background)" }}>
      {/* Toolbar */}
      <div style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "8px 12px",
        background: "var(--background-elevated)",
        borderBottom: "1px solid var(--border)",
        flexShrink: 0,
      }}>
        <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
          <button
            type="button"
            className="icon-btn icon-btn--sm"
            onClick={goToPrevPage}
            disabled={currentPage <= 1}
            title="Previous page"
            aria-label="Previous page"
          >
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="15 18 9 12 15 6" />
            </svg>
          </button>
          <span style={{ fontSize: "13px", fontWeight: "500", minWidth: "80px", textAlign: "center" }}>
            Page {currentPage} of {totalPages}
          </span>
          <button
            type="button"
            className="icon-btn icon-btn--sm"
            onClick={goToNextPage}
            disabled={currentPage >= totalPages}
            title="Next page"
            aria-label="Next page"
          >
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="9 18 15 12 9 6" />
            </svg>
          </button>
        </div>

        <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
          <label style={{ fontSize: "12px", color: "var(--text-secondary)" }}>Zoom:</label>
          <select
            value={Math.round(scale * 100)}
            onChange={(e) => {
              void parseInt((e.target as HTMLSelectElement).value, 10);
            }}
            style={{ fontSize: "12px", padding: "2px 8px", background: "var(--surface)", border: "1px solid var(--border)", borderRadius: "var(--radius-sm)" }}
            aria-label="Zoom level"
          >
            <option value="75">75%</option>
            <option value="100">100%</option>
            <option value="125">125%</option>
            <option value="150">150%</option>
            <option value="200">200%</option>
          </select>
          <input
            type="number"
            min={1}
            max={totalPages}
            value={currentPage}
            onChange={(e) => goToPage(parseInt((e.target as HTMLInputElement).value, 10))}
            onKeyDown={(e) => { if (e.key === "Enter") goToPage(parseInt((e.target as HTMLInputElement).value, 10)); }}
            style={{ width: "50px", fontSize: "12px", padding: "2px 8px", textAlign: "center", background: "var(--surface)", border: "1px solid var(--border)", borderRadius: "var(--radius-sm)" }}
            aria-label="Go to page"
          />
        </div>
      </div>

      {/* Page Container */}
      <div
        ref={containerRef}
        style={{
          flex: 1,
          overflow: "auto",
          padding: "16px",
          background: "var(--surface)",
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
        }}
      >
        {/* Pages are rendered dynamically via useEffect */}
      </div>

      {/* Per-page error indicator */}
      {pageErrors.length > 0 && (
        <div style={{
          padding: "12px",
          background: "rgba(220, 38, 38, 0.1)",
          border: "1px solid rgba(220, 38, 38, 0.3)",
          borderRadius: "var(--radius-md)",
          color: "#f87171",
          fontSize: "12px",
          margin: "16px",
        }}>
          <strong>Some pages could not be rendered:</strong>
          <ul style={{ marginTop: "6px", paddingLeft: "18px" }}>
            {pageErrors.map((e) => (
              <li key={e.page} style={{ marginBottom: "4px" }}>
                Page {e.page}: {e.message}
                <button
                  type="button"
                  onClick={() => retryPage(e.page)}
                  style={{ marginLeft: "8px", fontSize: "11px", padding: "2px 6px" }}
                >
                  Retry
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
});