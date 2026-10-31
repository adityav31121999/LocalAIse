import { useEffect, useRef, useState } from "react";
import {
  ChevronDownIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  CopyIcon,
  FolderIcon,
  PanelLeftIcon,
} from "../ui/icons";
import { templateBadgeClass, templateLabel } from "../../util/format";
import type { Project, WorkspaceTab } from "../../types";

interface ProjectHeaderProps {
  project: Project;
  tab: WorkspaceTab;
  onTabChange(tab: WorkspaceTab): void;
  onBack(): void;
  onToggleSidebar(): void;
  sidebarCollapsed: boolean;
}

const TABS: { id: WorkspaceTab; label: string }[] = [
  { id: "chat", label: "Workspace" },
  { id: "files", label: "Files" },
  { id: "analysis", label: "Analysis" },
  { id: "research", label: "Research" },
  { id: "journal", label: "Journal" },
  { id: "activity", label: "Activity" },
];

/**
 * One compact bar for the whole project: the window-level navigation lives in
 * the rail, this row only owns the project crumb and its scoped views.
 */
export function ProjectHeader({
  project,
  tab,
  onTabChange,
  onBack,
  onToggleSidebar,
  sidebarCollapsed,
}: ProjectHeaderProps) {
  const [crumbOpen, setCrumbOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const crumbRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!crumbOpen) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!crumbRef.current?.contains(event.target as Node)) setCrumbOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setCrumbOpen(false);
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [crumbOpen]);

  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(false), 1600);
    return () => window.clearTimeout(timer);
  }, [copied]);

  const copyPath = async () => {
    try {
      await navigator.clipboard.writeText(project.path);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  };

  return (
    <header className="project-header">
      <div className="project-header__crumbs">
        <button
          type="button"
          className="icon-btn icon-btn--sm"
          onClick={onBack}
          title="All work directories"
        >
          <ChevronLeftIcon size={16} />
          <span className="sr-only">Back to work directories</span>
        </button>

        <span className="crumb__root">Workspace</span>
        <ChevronRightIcon size={12} className="crumb__sep" />

        <div className="crumb" ref={crumbRef}>
          <button
            type="button"
            className="crumb__pill"
            aria-expanded={crumbOpen}
            aria-haspopup="menu"
            onClick={() => setCrumbOpen((open) => !open)}
            title={project.path}
          >
            <FolderIcon size={13} />
            <span className="truncate">{project.name}</span>
            <ChevronDownIcon size={12} className="crumb__caret" />
          </button>

          {crumbOpen && (
            <div className="crumb__menu" role="menu">
              <div className="crumb__menu-head">
                <span className="truncate">{project.name}</span>
                <span className={templateBadgeClass(project.template)}>
                  {templateLabel(project.template)}
                </span>
              </div>
              <div className="crumb__menu-path mono" title={project.path}>
                {project.path}
              </div>
              <div className="crumb__menu-meta">
                <span>{project.sourceCount} sources</span>
                <span>·</span>
                <span>{project.model}</span>
              </div>
              <div className="crumb__menu-actions">
                <button type="button" role="menuitem" onClick={() => void copyPath()}>
                  <CopyIcon size={13} />
                  {copied ? "Path copied" : "Copy path"}
                </button>
              </div>
            </div>
          )}
        </div>
      </div>

      <nav className="tabs" role="tablist" aria-label="Project views">
        {TABS.map((item) => (
          <button
            key={item.id}
            type="button"
            role="tab"
            className="tab"
            aria-selected={tab === item.id}
            onClick={() => onTabChange(item.id)}
          >
            {item.label}
          </button>
        ))}
      </nav>

      <span className="project-header__spacer" />

      <div className="project-header__actions">
        <button
          type="button"
          className="icon-btn icon-btn--sm"
          onClick={onToggleSidebar}
          aria-pressed={!sidebarCollapsed}
          title={sidebarCollapsed ? "Show sources panel" : "Hide sources panel"}
        >
          <PanelLeftIcon size={15} />
          <span className="sr-only">Toggle sources panel</span>
        </button>
      </div>
    </header>
  );
}
