/**
 * PDF Viewer Component — renders PDF pages visually using pdf.js.
 *
 * Panel-responsive resizing: in "Fit Width" mode (the default) the PDF scales
 * proportionally as the containing panel is shrunk or expanded. Pages are
 * re-rendered at the new scale (uniform X/Y scaling), so text never shrinks
 * horizontally while keeping its original vertical size, and the page always
 * tracks the panel width. A `ResizeObserver` watches the scroll container and
 * re-fits on every panel resize. Manual zoom ("custom" mode) keeps a fixed
 * scale regardless of panel size.
 */

import { useCallback, useEffect, useRef, useState, memo } from "react";
import type { ChangeEvent } from "react";
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

/** Manual zoom bounds (custom mode). */
const MIN_SCALE = 0.4;
const MAX_SCALE = 3.0;
/** Lower bound for auto-fit so a tiny panel still renders something. */
const FIT_MIN_SCALE = 0.1;
/** Discrete manual zoom stops. */
const ZOOM_LEVELS = [0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 3.0];
/** Horizontal padding inside the scroll container (matches `padding: 16px`). */
const CONTAINER_PADDING = 16;
/** Extra breathing room subtracted when computing fit-width scale. */
const FIT_GUTTER = 16;
/** Hard canvas bitmap cap (browser limits), independent of CSS size. */
const MAX_SAFE_DIM = 16384;
/** Ignore sub-pixel scale churn to avoid render thrash / resize loops. */
const SCALE_EPSILON = 0.001;

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/** Round to 2dp to keep rendering stable across repeated re-fits. */
function roundScale(value: number): number {
  return Math.round(value * 100) / 100;
}

export const PDFViewer = memo(function PDFViewer({
  file,
  initialPage = 1,
  onPageChange,
  scale = 1.5,
  maxCanvasSize = 4096,
}: PDFViewerProps) {
  const [pdf, setPdf] = useState<pdfjsLib.PDFDocumentProxy | null>(null);
  const [currentPage, setCurrentPage] = useState(initialPage);
  const [totalPages, setTotalPages] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [pageErrors, setPageErrors] = useState<PageError[]>([]);
  const [workerReady, setWorkerReady] = useState(false);
  const [currentScale, setCurrentScale] = useState<number>(scale);
  /** "fit" = auto-resize with panel; "custom" = fixed manual zoom. */
  const [zoomMode, setZoomMode] = useState<"fit" | "custom">("fit");

  const containerRef = useRef<HTMLDivElement>(null);
  const pageCanvasRefs = useRef<Map<number, HTMLCanvasElement>>(new Map());
  const textLayerRefs = useRef<Map<number, HTMLDivElement>>(new Map());
  const pageWrapperRefs = useRef<Map<number, HTMLDivElement>>(new Map());
  const renderTasksRef = useRef<Map<number, pdfjsLib.RenderTask>>(new Map());
  const textLayerTasksRef = useRef<Map<number, pdfjsLib.TextLayer>>(new Map());
  /** True after the first successful load; later reloads of the same view
   * keep the already-rendered pages visible instead of flashing a spinner. */
  const loadedOnceRef = useRef(false);
  /** Cache page-1 base width (scale=1) so re-fits don't need to await pdf.js. */
  const baseWidthRef = useRef<number>(0);
  /** rAF handle used to coalesce bursts of resize events. */
  const resizeRafRef = useRef<number | null>(null);

  // Prop mirrors, so the callbacks below can read them without being keyed
  // on them. The load effect must only depend on the actual document input.
  const initialPageRef = useRef(initialPage);
  initialPageRef.current = initialPage;
  const currentPageRef = useRef(currentPage);
  currentPageRef.current = currentPage;
  const onPageChangeRef = useRef(onPageChange);
  onPageChangeRef.current = onPageChange;

  // Sync externally supplied initial scale (does not fight internal fit/zoom;
  // only re-runs when the prop value itself changes).
  useEffect(() => {
    setCurrentScale(scale);
  }, [scale]);

  /** Compute the scale that fits page 1 to the available panel width. */
  const computeFitScale = useCallback((): number | null => {
    const container = containerRef.current;
    const baseWidth = baseWidthRef.current;
    if (!container || baseWidth <= 0) return null;
    const available =
      container.clientWidth - CONTAINER_PADDING * 2 - FIT_GUTTER;
    if (available <= 0) return null;
    // Proportional fit: scale page uniformly to available width. No upper
    // cap in fit mode so expanding the panel always grows the PDF; lower
    // bound keeps a hair-thin panel renderable.
    return clamp(available / baseWidth, FIT_MIN_SCALE, Number.POSITIVE_INFINITY);
  }, []);

  /** Recompute and apply fit-width scale; keeps panel-tracking stable. */
  const handleFitWidth = useCallback(async () => {
    if (!pdf || !containerRef.current) return;
    try {
      if (baseWidthRef.current <= 0) {
        const page = await pdf.getPage(1);
        const vp = page.getViewport({ scale: 1.0 });
        baseWidthRef.current = vp.width;
      }
      const next = computeFitScale();
      if (next === null) return;
      const rounded = roundScale(next);
      setCurrentScale((prev) =>
        Math.abs(prev - rounded) < SCALE_EPSILON ? prev : rounded,
      );
      setZoomMode("fit");
    } catch (e) {
      console.warn("Could not calculate fit width scale:", e);
    }
  }, [pdf, computeFitScale]);

  const handleZoomSelect = (e: ChangeEvent<HTMLSelectElement>) => {
    const val = e.target.value;
    if (val === "fit") {
      void handleFitWidth();
    } else {
      setZoomMode("custom");
      const num = parseInt(val, 10);
      if (!isNaN(num) && num > 0) {
        setCurrentScale(clamp(num / 100, MIN_SCALE, MAX_SCALE));
      }
    }
  };

  const handleZoomIn = () => {
    setZoomMode("custom");
    const next = ZOOM_LEVELS.find((l) => l > currentScale + 0.05);
    if (next) {
      setCurrentScale(next);
    } else {
      setCurrentScale((prev) => clamp(roundScale(prev + 0.25), MIN_SCALE, MAX_SCALE));
    }
  };

  const handleZoomOut = () => {
    setZoomMode("custom");
    const prevLevels = [...ZOOM_LEVELS].reverse();
    const prev = prevLevels.find((l) => l < currentScale - 0.05);
    if (prev) {
      setCurrentScale(prev);
    } else {
      setCurrentScale((p) => clamp(roundScale(p - 0.25), MIN_SCALE, MAX_SCALE));
    }
  };

  /**
   * Panel-resize tracking.
   *
   * A ResizeObserver on the scroll container re-fits the PDF whenever the
   * panel is shrunk or expanded. Re-fitting is only applied in "fit" mode so
   * an explicit manual zoom ("custom") is left untouched. Events are
   * coalesced with requestAnimationFrame and a scale epsilon to avoid render
   * thrash and observer feedback loops, and rendering is debounced so a
   * continuous drag re-renders at most once per animation frame.
   */
  useEffect(() => {
    const container = containerRef.current;
    if (!container || !pdf) return;

    const scheduleRefit = () => {
      if (zoomMode !== "fit") return;
      if (resizeRafRef.current !== null) cancelAnimationFrame(resizeRafRef.current);
      resizeRafRef.current = requestAnimationFrame(() => {
        resizeRafRef.current = null;
        void handleFitWidth();
      });
    };

    const observer = new ResizeObserver(scheduleRefit);
    observer.observe(container);
    // Fit once immediately for the current panel size.
    scheduleRefit();

    return () => {
      observer.disconnect();
      if (resizeRafRef.current !== null) {
        cancelAnimationFrame(resizeRafRef.current);
        resizeRafRef.current = null;
      }
    };
  }, [pdf, zoomMode, handleFitWidth]);

  // Ctrl/Cmd + Wheel to zoom (switches to manual "custom" mode).
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const handleWheel = (e: WheelEvent) => {
      if (e.ctrlKey || e.metaKey) {
        e.preventDefault();
        const delta = e.deltaY < 0 ? 0.15 : -0.15;
        setCurrentScale((prev) => clamp(roundScale(prev + delta), MIN_SCALE, MAX_SCALE));
        setZoomMode("custom");
      }
    };

    container.addEventListener("wheel", handleWheel, { passive: false });
    return () => {
      container.removeEventListener("wheel", handleWheel);
    };
  }, []);

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
    void initWorker();
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
      // Cache page-1 base width for fit-width computations.
      try {
        const firstPage = await doc.getPage(1);
        baseWidthRef.current = firstPage.getViewport({ scale: 1.0 }).width;
      } catch {
        baseWidthRef.current = 0;
      }
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
    void loadPdf();
  }, [loadPdf]);

  // Render a single page to canvas and textLayer. Renders uniformly in X and
  // Y at `currentScale`; CSS size tracks the scaled page exactly so text is
  // never squeezed horizontally while keeping its original vertical size.
  const renderPage = useCallback(
    async (
      pageNum: number,
      canvas: HTMLCanvasElement,
      textLayerDiv: HTMLDivElement,
      pageWrapper: HTMLDivElement,
    ) => {
      if (!pdf) return;
      try {
        const page = await pdf.getPage(pageNum);
        const viewport = page.getViewport({ scale: currentScale });

        // Handle HiDPI displays (cap at 2x)
        const dpr = Math.min(window.devicePixelRatio || 1, 2);
        let outputScale = dpr;

        // Clamp to max canvas dimensions to avoid browser limits. This only
        // reduces bitmap resolution; CSS size still tracks the scaled page.
        if (
          viewport.width * outputScale > MAX_SAFE_DIM ||
          viewport.height * outputScale > MAX_SAFE_DIM ||
          viewport.width * outputScale > maxCanvasSize ||
          viewport.height * outputScale > maxCanvasSize
        ) {
          outputScale = Math.min(
            maxCanvasSize / viewport.width,
            maxCanvasSize / viewport.height,
            MAX_SAFE_DIM / viewport.width,
            MAX_SAFE_DIM / viewport.height,
          );
        }

        const outputViewport = page.getViewport({ scale: currentScale * outputScale });

        const cssWidth = Math.max(1, Math.floor(viewport.width));
        const cssHeight = Math.max(1, Math.floor(viewport.height));

        pageWrapper.style.width = `${cssWidth}px`;
        pageWrapper.style.height = `${cssHeight}px`;
        pageWrapper.style.maxWidth = "100%";
        pageWrapper.style.setProperty("--scale-factor", currentScale.toString());
        pageWrapper.style.setProperty("--total-scale-factor", currentScale.toString());
        pageWrapper.style.setProperty("--scale-round-x", "1px");
        pageWrapper.style.setProperty("--scale-round-y", "1px");
        pageWrapper.style.setProperty("--user-unit", "1");

        canvas.width = Math.floor(outputViewport.width);
        canvas.height = Math.floor(outputViewport.height);
        canvas.style.width = `${cssWidth}px`;
        canvas.style.height = `${cssHeight}px`;
        canvas.style.maxWidth = "100%";

        textLayerDiv.style.width = `${cssWidth}px`;
        textLayerDiv.style.height = `${cssHeight}px`;
        textLayerDiv.style.setProperty("--scale-factor", currentScale.toString());
        textLayerDiv.style.setProperty("--total-scale-factor", currentScale.toString());
        textLayerDiv.style.setProperty("--scale-round-x", "1px");
        textLayerDiv.style.setProperty("--scale-round-y", "1px");
        textLayerDiv.style.setProperty("--user-unit", "1");

        const context = canvas.getContext("2d");
        if (!context) throw new Error("Canvas context unavailable");

        // Fill with white background first (PDF pages may have transparent backgrounds)
        context.fillStyle = "white";
        context.fillRect(0, 0, canvas.width, canvas.height);

        // High-quality rendering
        context.imageSmoothingEnabled = true;
        context.imageSmoothingQuality = "high";

        // Render Canvas
        const task = page.render({ canvas, canvasContext: context, viewport: outputViewport });
        renderTasksRef.current.set(pageNum, task);
        await task.promise;
        if (renderTasksRef.current.get(pageNum) === task) {
          renderTasksRef.current.delete(pageNum);
        }

        // Render Text Layer for text selection
        try {
          const textContent = await page.getTextContent();
          const textLayer = new pdfjsLib.TextLayer({
            textContentSource: textContent,
            container: textLayerDiv,
            viewport: viewport,
          });
          textLayerTasksRef.current.set(pageNum, textLayer);
          await textLayer.render();
          if (textLayerTasksRef.current.get(pageNum) === textLayer) {
            textLayerTasksRef.current.delete(pageNum);
          }
        } catch (textErr) {
          if (!isRenderCancelled(textErr)) {
            console.warn(`TextLayer error on page ${pageNum}:`, textErr);
          }
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
    },
    [pdf, currentScale, maxCanvasSize],
  );

  // Render all pages when the PDF loads or its scale changes. Scale changes
  // (including panel-driven re-fits) re-render every page at the new uniform
  // scale rather than letting CSS shrink only one axis.
  useEffect(() => {
    if (!pdf || !containerRef.current) return;

    const container = containerRef.current;
    container.innerHTML = ""; // Clear previous renders
    pageCanvasRefs.current.clear();
    textLayerRefs.current.clear();
    pageWrapperRefs.current.clear();

    // Races the previous pass: any render still in flight belongs to a
    // canvas that is about to be dropped.
    renderTasksRef.current.forEach((task) => task.cancel());
    renderTasksRef.current.clear();
    textLayerTasksRef.current.forEach((tl) => {
      try {
        tl.cancel();
      } catch {
        // cancelling an already-finished text layer is a no-op
      }
    });
    textLayerTasksRef.current.clear();

    const pageNumbers = Array.from({ length: totalPages }, (_, i) => i + 1);

    pageNumbers.forEach(async (pageNum) => {
      const pageWrapper = document.createElement("div");
      pageWrapper.className = "pdf-page";
      pageWrapper.style.position = "relative";
      pageWrapper.style.display = "block";
      pageWrapper.style.marginLeft = "auto";
      pageWrapper.style.marginRight = "auto";
      pageWrapper.style.marginBottom = "16px";
      pageWrapper.style.boxShadow = "0 2px 8px rgba(0,0,0,0.12)";
      pageWrapper.style.background = "white";
      pageWrapper.style.maxWidth = "100%";

      const canvas = document.createElement("canvas");
      canvas.className = "pdf-page-canvas";
      canvas.style.display = "block";

      const textLayerDiv = document.createElement("div");
      textLayerDiv.className = "textLayer";

      pageWrapper.appendChild(canvas);
      pageWrapper.appendChild(textLayerDiv);
      container.appendChild(pageWrapper);

      pageCanvasRefs.current.set(pageNum, canvas);
      textLayerRefs.current.set(pageNum, textLayerDiv);
      pageWrapperRefs.current.set(pageNum, pageWrapper);

      await renderPage(pageNum, canvas, textLayerDiv, pageWrapper);
    });

    // Drop this pass's renders when the view changes or the component unmounts,
    // so a slow page render cannot write into a canvas nobody displays.
    return () => {
      renderTasksRef.current.forEach((task) => task.cancel());
      renderTasksRef.current.clear();
      textLayerTasksRef.current.forEach((tl) => {
        try {
          tl.cancel();
        } catch {
          // cancelling an already-finished text layer is a no-op
        }
      });
      textLayerTasksRef.current.clear();
    };
  }, [pdf, totalPages, renderPage]);

  // Track visible page on scroll
  useEffect(() => {
    const container = containerRef.current;
    if (!container || pageWrapperRefs.current.size === 0) return;

    const observer = new IntersectionObserver(
      (entries) => {
        let bestEntry: IntersectionObserverEntry | null = null;
        for (const entry of entries) {
          if (entry.isIntersecting) {
            if (!bestEntry || entry.intersectionRatio > bestEntry.intersectionRatio) {
              bestEntry = entry;
            }
          }
        }
        if (bestEntry) {
          for (const [pageNum, wrapper] of pageWrapperRefs.current.entries()) {
            if (wrapper === bestEntry.target) {
              setCurrentPage(pageNum);
              onPageChangeRef.current?.(pageNum, totalPages);
              break;
            }
          }
        }
      },
      {
        root: container,
        threshold: [0.1, 0.4, 0.7],
      },
    );

    pageWrapperRefs.current.forEach((wrapper) => observer.observe(wrapper));
    return () => observer.disconnect();
  }, [totalPages, currentScale]);

  // Handle page navigation
  const goToPage = (page: number) => {
    if (page < 1 || page > totalPages) return;
    setCurrentPage(page);
    onPageChange?.(page, totalPages);

    // Scroll to the page wrapper
    const wrapper = pageWrapperRefs.current.get(page);
    wrapper?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  const goToPrevPage = () => goToPage(currentPage - 1);
  const goToNextPage = () => goToPage(currentPage + 1);

  const retryPage = (page: number) => {
    const canvas = pageCanvasRefs.current.get(page);
    const textLayerDiv = textLayerRefs.current.get(page);
    const pageWrapper = pageWrapperRefs.current.get(page);
    if (canvas && textLayerDiv && pageWrapper) {
      textLayerDiv.innerHTML = "";
      void renderPage(page, canvas, textLayerDiv, pageWrapper);
    }
  };

  if (!workerReady) {
    return (
      <div className="pdf-viewer" style={{ display: "flex", flexDirection: "column", height: "100%", minWidth: 0 }}>
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
      <div className="pdf-viewer" style={{ display: "flex", flexDirection: "column", height: "100%", minWidth: 0 }}>
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
      <div className="pdf-viewer" style={{ display: "flex", flexDirection: "column", height: "100%", minWidth: 0 }}>
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

  const fitSelected = zoomMode === "fit";

  return (
    <div
      className="pdf-viewer"
      style={{ display: "flex", flexDirection: "column", height: "100%", width: "100%", minWidth: 0, minHeight: 0, background: "var(--background)" }}
    >
      {/* Toolbar */}
      <div style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "8px 12px",
        background: "var(--background-elevated)",
        borderBottom: "1px solid var(--border)",
        flexShrink: 0,
        gap: "8px",
      }}>
        <div style={{ display: "flex", alignItems: "center", gap: "6px" }}>
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
          <span style={{ fontSize: "12px", fontWeight: "500", minWidth: "75px", textAlign: "center", color: "var(--text)" }}>
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

        <div style={{ display: "flex", alignItems: "center", gap: "6px" }}>
          <label style={{ fontSize: "12px", color: "var(--text-secondary)" }}>Zoom:</label>
          <button
            type="button"
            className="icon-btn icon-btn--sm"
            onClick={handleZoomOut}
            disabled={currentScale <= MIN_SCALE}
            title="Zoom out"
            aria-label="Zoom out"
            style={{ width: "24px", height: "24px" }}
          >
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="11" cy="11" r="8" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" />
              <line x1="8" y1="11" x2="14" y2="11" />
            </svg>
          </button>

          <select
            value={fitSelected ? "fit" : Math.round(currentScale * 100).toString()}
            onChange={handleZoomSelect}
            style={{
              fontSize: "12px",
              padding: "2px 6px",
              height: "24px",
              background: "var(--surface)",
              color: "var(--text)",
              border: "1px solid var(--border)",
              borderRadius: "var(--radius-sm)",
              cursor: "pointer",
            }}
            aria-label="Zoom level"
          >
            <option value="fit">Fit Width</option>
            <option value="50">50%</option>
            <option value="75">75%</option>
            <option value="100">100%</option>
            <option value="125">125%</option>
            <option value="150">150%</option>
            <option value="175">175%</option>
            <option value="200">200%</option>
            <option value="300">300%</option>
            {!["fit", "50", "75", "100", "125", "150", "175", "200", "300"].includes(
              Math.round(currentScale * 100).toString()
            ) && !fitSelected && (
              <option value={Math.round(currentScale * 100).toString()}>
                {Math.round(currentScale * 100)}%
              </option>
            )}
          </select>

          <button
            type="button"
            className="icon-btn icon-btn--sm"
            onClick={handleZoomIn}
            disabled={currentScale >= MAX_SCALE}
            title="Zoom in"
            aria-label="Zoom in"
            style={{ width: "24px", height: "24px" }}
          >
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="11" cy="11" r="8" />
              <line x1="11" y1="8" x2="11" y2="14" />
              <line x1="8" y1="11" x2="14" y2="11" />
            </svg>
          </button>

          <button
            type="button"
            className="icon-btn icon-btn--sm"
            onClick={() => void handleFitWidth()}
            title="Fit width — resize PDF with panel"
            aria-label="Fit width"
            aria-pressed={fitSelected}
            style={{ width: "24px", height: "24px" }}
          >
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="4 8 4 4 8 4" />
              <polyline points="20 8 20 4 16 4" />
              <polyline points="4 16 4 20 8 20" />
              <polyline points="20 16 20 20 16 20" />
            </svg>
          </button>

          <div style={{ width: "1px", height: "16px", background: "var(--border)", margin: "0 2px" }} />

          <input
            type="number"
            min={1}
            max={totalPages}
            value={currentPage}
            onChange={(e) => {
              const val = parseInt(e.target.value, 10);
              if (!isNaN(val)) goToPage(val);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                const val = parseInt(e.currentTarget.value, 10);
                if (!isNaN(val)) goToPage(val);
              }
            }}
            style={{
              width: "44px",
              height: "24px",
              fontSize: "12px",
              padding: "2px 4px",
              textAlign: "center",
              background: "var(--surface)",
              color: "var(--text)",
              border: "1px solid var(--border)",
              borderRadius: "var(--radius-sm)",
            }}
            title="Jump to page"
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
          padding: `${CONTAINER_PADDING}px`,
          background: "var(--surface)",
          minWidth: 0,
          minHeight: 0,
          width: "100%",
          boxSizing: "border-box",
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
