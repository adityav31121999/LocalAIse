import { BookIcon, ChevronLeftIcon, DocIcon, PanelLeftIcon } from "../ui/icons";
import { templateBadgeClass, templateLabel } from "../../util/format";
import type { Project, WorkspaceTab } from "../../types";

interface ProjectHeaderProps {
  project: Project;
  tab: WorkspaceTab;
  onTabChange(tab: WorkspaceTab): void;
  onBack(): void;
  onToggleDiary(enabled: boolean): void;
  onToggleSidebar(): void;
  sidebarCollapsed: boolean;
}

export function ProjectHeader({
  project,
  tab,
  onTabChange,
  onBack,
  onToggleDiary,
  onToggleSidebar,
  sidebarCollapsed,
}: ProjectHeaderProps) {
  return (
    <header className="project-header">
      <button type="button" className="icon-btn icon-btn--sm" onClick={onBack} title="All work directories">
        <ChevronLeftIcon size={16} />
        <span className="sr-only">Back to work directories</span>
      </button>

      <button
        type="button"
        className="icon-btn icon-btn--sm"
        onClick={onToggleSidebar}
        aria-pressed={!sidebarCollapsed}
        title={sidebarCollapsed ? "Show sources panel" : "Hide sources panel"}
      >
        <PanelLeftIcon size={16} />
        <span className="sr-only">Toggle sources panel</span>
      </button>

      <div className="project-header__identity">
        <div className="project-header__name">
          <DocIcon size={14} />
          <span className="truncate">{project.name}</span>
          <span className={templateBadgeClass(project.template)}>{templateLabel(project.template)}</span>
          <span className="badge">{project.sourceCount} sources</span>
        </div>
        <div className="project-header__path truncate" title={project.path}>
          {project.path}
        </div>
      </div>

      <span className="project-header__spacer" />

      <div className="tabs" role="tablist" aria-label="Project views">
        <button
          type="button"
          role="tab"
          className="tab"
          aria-selected={tab === "chat"}
          onClick={() => onTabChange("chat")}
        >
          Chat
        </button>
        <button
          type="button"
          role="tab"
          className="tab"
          aria-selected={tab === "files"}
          onClick={() => onTabChange("files")}
        >
          Files
        </button>
        <button
          type="button"
          role="tab"
          className="tab"
          aria-selected={tab === "analysis"}
          onClick={() => onTabChange("analysis")}
        >
          Analysis
        </button>
        <button
          type="button"
          role="tab"
          className="tab"
          aria-selected={tab === "research"}
          onClick={() => onTabChange("research")}
        >
          Research
        </button>
        {project.diaryEnabled ? (
          <button
            type="button"
            role="tab"
            className="tab"
            aria-selected={tab === "diary"}
            onClick={() => onTabChange("diary")}
          >
            Diary
          </button>
        ) : null}
        <button
          type="button"
          role="tab"
          className="tab"
          aria-selected={tab === "activity"}
          onClick={() => onTabChange("activity")}
        >
          Activity
        </button>
      </div>

      <button
        type="button"
        className={`icon-btn icon-btn--sm${project.diaryEnabled ? " icon-btn--active" : ""}`}
        aria-pressed={project.diaryEnabled}
        title={
          project.diaryEnabled
            ? "Project diary is on — pages live in diary/"
            : "Add a diary/ folder with one Markdown page per day"
        }
        onClick={() => onToggleDiary(!project.diaryEnabled)}
      >
        <BookIcon size={15} />
        <span className="sr-only">
          {project.diaryEnabled ? "Turn the project diary off" : "Turn the project diary on"}
        </span>
      </button>
    </header>
  );
}
