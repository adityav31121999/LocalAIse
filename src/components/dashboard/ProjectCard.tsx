import { useEffect, useRef, useState } from "react";
import {
  CopyIcon,
  FolderIcon,
  MoreIcon,
  PencilIcon,
  TrashIcon,
} from "../ui/icons";
import { NetworkIndicator } from "../layout/NetworkIndicator";
import { formatRelativeTime, templateBadgeClass, templateLabel } from "../../util/format";
import type { Project } from "../../types";

interface ProjectCardProps {
  project: Project;
  /** This project's share of all indexed sources, 0–100. */
  share: number;
  totalSources: number;
  onOpen(): void;
  onRename(): void;
  onRemove(): void;
}

/**
 * The whole card opens the project; secondary actions hide behind a small
 * kebab menu so the card itself stays a single, obvious click target.
 */
export function ProjectCard({
  project,
  share,
  totalSources,
  onOpen,
  onRename,
  onRemove,
}: ProjectCardProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menuOpen) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) setMenuOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setMenuOpen(false);
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [menuOpen]);

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
    <article className={`project-card tpl-${project.template}`}>
      <button
        type="button"
        className="project-card__open"
        onClick={onOpen}
        title={`Open ${project.path}`}
      >
        <span className="project-card__top">
          <span className="project-card__icon">
            <FolderIcon size={16} />
          </span>
          <span className="project-card__badges">
            <span className={templateBadgeClass(project.template)}>
              {templateLabel(project.template)}
            </span>
            {project.diaryEnabled ? <span className="badge">Journal</span> : null}
          </span>
        </span>

        <span className="project-card__name truncate">{project.name}</span>
        <span className="project-card__path truncate">{project.path}</span>

        <span className="card-gauge">
          <span className="card-gauge__head">
            <span>Sources</span>
            <span className="mono">
              {project.sourceCount}
              {totalSources > 0 ? ` / ${totalSources}` : ""}
            </span>
          </span>
          <span className="card-gauge__bar" aria-hidden="true">
            <span
              className="card-gauge__fill"
              style={{ width: `${Math.max(project.sourceCount > 0 ? 4 : 0, share)}%` }}
            />
          </span>
        </span>

        <span className="project-card__meta">
          <span>{formatRelativeTime(project.lastOpenedAt)}</span>
          <span className="spacer" />
          <NetworkIndicator enabled={project.internetEnabled} />
        </span>
      </button>

      <div className="project-card__menu" ref={menuRef}>
        <button
          type="button"
          className="icon-btn icon-btn--sm"
          aria-haspopup="menu"
          aria-expanded={menuOpen}
          aria-label={`Actions for ${project.name}`}
          title="Project actions"
          onClick={() => setMenuOpen((open) => !open)}
        >
          <MoreIcon size={15} />
        </button>

        {menuOpen && (
          <div className="card-menu" role="menu">
            <button
              type="button"
              role="menuitem"
              onClick={() => {
                setMenuOpen(false);
                onRename();
              }}
            >
              <PencilIcon size={13} />
              Rename
            </button>
            <button
              type="button"
              role="menuitem"
              onClick={() => void copyPath()}
            >
              <CopyIcon size={13} />
              {copied ? "Path copied" : "Copy path"}
            </button>
            <button
              type="button"
              role="menuitem"
              className="card-menu__danger"
              onClick={() => {
                setMenuOpen(false);
                onRemove();
              }}
            >
              <TrashIcon size={13} />
              Remove
            </button>
          </div>
        )}
      </div>
    </article>
  );
}
