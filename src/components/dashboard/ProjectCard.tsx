import { FolderIcon } from "../ui/icons";
import { NetworkIndicator } from "../layout/NetworkIndicator";
import { formatRelativeTime, templateBadgeClass, templateLabel } from "../../util/format";
import type { Project } from "../../types";

interface ProjectCardProps {
  project: Project;
  onOpen(): void;
}

export function ProjectCard({ project, onOpen }: ProjectCardProps) {
  return (
    <button
      type="button"
      className={`project-card tpl-${project.template}`}
      onClick={onOpen}
      title={`Open ${project.path}`}
    >
      <span className="project-card__top">
        <span className="project-card__icon">
          <FolderIcon size={16} />
        </span>
        <span className={templateBadgeClass(project.template)}>{templateLabel(project.template)}</span>
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
  );
}
