import { FolderIcon, PencilIcon, TrashIcon } from "../ui/icons";
import { NetworkIndicator } from "../layout/NetworkIndicator";
import { formatRelativeTime, templateBadgeClass, templateLabel } from "../../util/format";
import type { Project } from "../../types";

interface ProjectCardProps {
  project: Project;
  onOpen(): void;
  onRename(): void;
  onRemove(): void;
}

export function ProjectCard({ project, onOpen, onRename, onRemove }: ProjectCardProps) {
  return (
    <div className={`project-card tpl-${project.template}`}>
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
          <span className={templateBadgeClass(project.template)}>
            {templateLabel(project.template)}
          </span>
        </span>

        <span className="project-card__name truncate">{project.name}</span>
        <span className="project-card__path truncate">{project.path}</span>

        <span className="project-card__meta">
          <span>{project.sourceCount} sources</span>
          <span>·</span>
          <span>{formatRelativeTime(project.lastOpenedAt)}</span>
          <span className="spacer" />
          <NetworkIndicator enabled={project.internetEnabled} />
        </span>
      </button>

      <div className="project-card__actions">
        <button
          type="button"
          className="btn btn--subtle btn--sm"
          onClick={onRename}
          title="Rename this project"
        >
          <PencilIcon size={12} />
          Rename
        </button>
        <button
          type="button"
          className="btn btn--subtle btn--sm project-card__remove"
          onClick={onRemove}
          title="Remove this project from the app"
        >
          <TrashIcon size={12} />
          Remove
        </button>
      </div>
    </div>
  );
}
