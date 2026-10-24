import { CalendarIcon, CpuIcon, FolderIcon, HomeIcon, PaletteIcon, ShieldIcon } from "../ui/icons";
import { useProjectStore, selectActiveProject } from "../../state/projectStore";
import { useNavStore } from "../../state/navStore";
import { useSettingsStore } from "../../state/settingsStore";
import { useRuntimeStore } from "../../state/runtimeStore";

export function ActivityRail() {
  const view = useNavStore((state) => state.view);
  const showHome = useNavStore((state) => state.showHome);
  const showThemeSettings = useNavStore((state) => state.showThemeSettings);
  const closeThemeSettings = useNavStore((state) => state.closeThemeSettings);
  const showModelConfig = useNavStore((state) => state.showModelConfig);
  const closeModelConfig = useNavStore((state) => state.closeModelConfig);
  const activeProjectTab = useNavStore((state) => state.activeProjectTab);
  const setActiveProjectTab = useNavStore((state) => state.setActiveProjectTab);

  const projects = useProjectStore((state) => state.projects);
  const project = useProjectStore(selectActiveProject);
  const openProject = useProjectStore((state) => state.openProject);
  const closeProject = useProjectStore((state) => state.closeProject);
  const themeName = useSettingsStore((state) => state.themeName);
  const runtimeStatus = useRuntimeStore((state) => state.status);

  const isModelReady = runtimeStatus.state === "ready";

  const handleHomeClick = () => {
    closeProject();
    showHome();
  };

  const handleProjectClick = () => {
    if (project) {
      if (activeProjectTab === "calendar") {
        setActiveProjectTab("chat");
      }
      showHome();
    } else if (projects.length > 0) {
      void openProject(projects[0].id);
    } else {
      showHome();
    }
  };

  const handleCalendarClick = () => {
    if (project) {
      showHome();
      setActiveProjectTab("calendar");
    }
  };

  return (
    <nav className="rail" aria-label="Primary navigation">
      <div className="rail__brand" title="localAIse — private, local-first workspace">
        <ShieldIcon size={18} />
      </div>

      {/* 3 buttons on top: Home, Project, Calendar (while project is open) */}
      <div className="rail__nav">
        {/* 1. Home button */}
        <button
          type="button"
          className="rail__item"
          aria-current={view === "home" && project == null}
          onClick={handleHomeClick}
          title="Home — Work directories"
        >
          <HomeIcon size={18} />
          <span className="sr-only">Home</span>
        </button>

        {/* 2. Project button */}
        <button
          type="button"
          className="rail__item"
          aria-current={view === "home" && project != null && activeProjectTab !== "calendar"}
          onClick={handleProjectClick}
          title={
            project
              ? `Project workspace · ${project.name}`
              : projects.length > 0
                ? `Open project (${projects[0].name})`
                : "Project workspace"
          }
        >
          <FolderIcon size={18} />
          <span className="sr-only">Project</span>
        </button>

        {/* 3. Calendar button (only appears while project is open) */}
        {project && (
          <button
            type="button"
            className="rail__item"
            aria-current={view === "home" && activeProjectTab === "calendar"}
            onClick={handleCalendarClick}
            title={`Project Calendar & Linked Diary — ${project.name}`}
          >
            <CalendarIcon size={18} />
            <span className="sr-only">Calendar</span>
          </button>
        )}
      </div>

      {/* 2 buttons on bottom: Theme, Model settings (redesigned with sleek, cohesive styling) */}
      <div className="rail__settings">
        {/* Theme Settings Button */}
        <button
          type="button"
          className="rail__item"
          aria-current={view === "theme-settings"}
          onClick={() => {
            if (view === "theme-settings") {
              closeThemeSettings();
            } else {
              showThemeSettings();
            }
          }}
          title={`Theme settings (${themeName})`}
          aria-label="Theme settings"
        >
          <PaletteIcon size={18} />
        </button>

        {/* Model Configuration Button */}
        <button
          type="button"
          className="rail__item"
          aria-current={view === "model-config"}
          onClick={() => {
            if (view === "model-config") {
              closeModelConfig();
            } else {
              showModelConfig();
            }
          }}
          title={`Model settings (${runtimeStatus.model || "Local model"}${isModelReady ? " · Ready" : ""})`}
          aria-label="Model configuration"
        >
          <CpuIcon size={18} />
          {isModelReady && <span className="rail__status-indicator" title="Model ready" />}
        </button>
      </div>
    </nav>
  );
}
