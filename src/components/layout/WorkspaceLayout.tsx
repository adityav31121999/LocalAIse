import type { ReactNode } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useSettingsStore } from "../../state/settingsStore";

const MIN_SIDEBAR_WIDTH = 200;
const MAX_SIDEBAR_WIDTH = 500;

interface WorkspaceLayoutProps {
  header: ReactNode;
  sidebar: ReactNode;
  sidebarCollapsed: boolean;
  children: ReactNode;
}

export function WorkspaceLayout({
  header,
  sidebar,
  sidebarCollapsed,
  children,
}: WorkspaceLayoutProps) {
  const sidebarWidth = useSettingsStore((state) => state.sidebarWidth);
  const setSidebarWidth = useSettingsStore((state) => state.setSidebarWidth);

  const [dragging, setDragging] = useState(false);
  const sidebarElRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!dragging) return;
    document.body.classList.add("workspace-resizing");
    const handleMove = (event: PointerEvent) => {
      const sidebar = sidebarElRef.current;
      if (!sidebar) return;
      const rect = sidebar.getBoundingClientRect();
      const next = event.clientX - rect.left;
      setSidebarWidth(Math.min(MAX_SIDEBAR_WIDTH, Math.max(MIN_SIDEBAR_WIDTH, next)));
    };
    const stop = () => setDragging(false);
    window.addEventListener("pointermove", handleMove);
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
    return () => {
      document.body.classList.remove("workspace-resizing");
      window.removeEventListener("pointermove", handleMove);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
    };
  }, [dragging, setSidebarWidth]);

  const handleSplitterDoubleClick = useCallback(() => {
    setSidebarWidth(268);
  }, [setSidebarWidth]);

  return (
    <div className="workspace">
      {header}
      <div
        className={`workspace__body${sidebarCollapsed ? " workspace__body--collapsed" : ""}`}
        style={{
          gridTemplateColumns: sidebarCollapsed
            ? "0 0px minmax(0, 1fr)"
            : `${sidebarWidth}px 9px minmax(0, 1fr)`,
          gridTemplateRows: "minmax(0, 1fr)",
        }}
      >
        <aside
          className="workspace__sidebar"
          ref={sidebarElRef}
          aria-hidden={sidebarCollapsed}
          style={{ width: sidebarCollapsed ? 0 : sidebarWidth }}
        >
          {sidebar}
        </aside>
        <div
          className="workspace__splitter"
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize sources panel"
          aria-valuenow={sidebarWidth}
          aria-valuemin={MIN_SIDEBAR_WIDTH}
          aria-valuemax={MAX_SIDEBAR_WIDTH}
          data-dragging={dragging}
          onPointerDown={(event) => {
            event.preventDefault();
            setDragging(true);
          }}
          onDoubleClick={handleSplitterDoubleClick}
          title="Drag to resize · double-click to reset"
        />
        <main className="workspace__main">{children}</main>
      </div>
    </div>
  );
}
