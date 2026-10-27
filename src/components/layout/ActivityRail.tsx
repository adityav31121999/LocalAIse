import {
  FolderIcon,
  HomeIcon,
  ModelIcon,
  SettingsIcon,
  ThemeIcon,
} from "../ui/icons";
import { useProjectStore, selectActiveProject } from "../../state/projectStore";
import { useNavStore } from "../../state/navStore";
import { useSettingsStore } from "../../state/settingsStore";
import { useRuntimeStore } from "../../state/runtimeStore";

/**
 * The app-level rail. It owns navigation that exists outside any one project:
 * home, the open project, appearance, model & hardware, and settings. Views
 * that belong to a project live in the project header instead.
 */
export function ActivityRail() {
  const view = useNavStore((state) => state.view);
  const showHome = useNavStore((state) => state.showHome);
  const showSettings = useNavStore((state) => state.showSettings);
  const showThemeSettings = useNavStore((state) => state.showThemeSettings);
  const closeThemeSettings = useNavStore((state) => state.closeThemeSettings);
  const showModelConfig = useNavStore((state) => state.showModelConfig);
  const closeModelConfig = useNavStore((state) => state.closeModelConfig);

  const projects = useProjectStore((state) => state.projects);
  const project = useProjectStore(selectActiveProject);
  const openProject = useProjectStore((state) => state.openProject);
  const closeProject = useProjectStore((state) => state.closeProject);
  const themeName = useSettingsStore((state) => state.themeName);
  const runtimeStatus = useRuntimeStore((state) => state.status);

  const isModelReady = runtimeStatus.state === "ready";
  const isModelLoading = runtimeStatus.state === "starting";
  const hasModel = Boolean(runtimeStatus.model);

  const handleHomeClick = () => {
    closeProject();
    showHome();
  };

  const handleProjectClick = () => {
    if (project) showHome();
    else if (projects.length > 0) void openProject(projects[0].id);
    else showHome();
  };

  const modelDotColor = !hasModel
    ? "var(--accent-attention)"
    : isModelLoading
      ? "var(--accent-research)"
      : isModelReady
        ? "var(--accent-success)"
        : "var(--text-faint)";

  const modelDotTitle = !hasModel
    ? "No model loaded"
    : isModelLoading
      ? "Model loading…"
      : isModelReady
        ? "Model ready"
        : "Model not ready";

  return (
    <nav className="rail" aria-label="Primary navigation">
      <div className="rail__brand" title="localAIse — private, local-first workspace">
        <ShieldMark />
      </div>

      <div className="rail__nav">
        <button
          type="button"
          className="rail__item"
          aria-current={view === "home" && project == null}
          onClick={handleHomeClick}
          title="Home — work directories"
        >
          <HomeIcon size={18} />
          <span className="sr-only">Home</span>
        </button>

        <button
          type="button"
          className="rail__item"
          aria-current={view === "home" && project != null}
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
      </div>

      <div className="rail__settings">
        <button
          type="button"
          className="rail__item"
          aria-current={view === "settings"}
          onClick={showSettings}
          title="Settings — appearance, models and runtime"
        >
          <SettingsIcon size={18} />
          <span className="sr-only">Settings</span>
        </button>

        <button
          type="button"
          className="rail__item"
          aria-current={view === "theme-settings"}
          onClick={() => {
            if (view === "theme-settings") closeThemeSettings();
            else showThemeSettings();
          }}
          title={`Appearance (${themeName})`}
        >
          <ThemeIcon size={18} />
          <span className="sr-only">Appearance</span>
        </button>

        <button
          type="button"
          className="rail__item"
          aria-current={view === "model-config"}
          onClick={() => {
            if (view === "model-config") closeModelConfig();
            else showModelConfig();
          }}
          title={`Model & hardware (${runtimeStatus.model || "no model"} · ${modelDotTitle})`}
        >
          <ModelIcon size={18} />
          <span
            className="rail__status"
            style={{ background: modelDotColor }}
            title={modelDotTitle}
            aria-hidden="true"
          >
            {isModelLoading && <span className="rail__status-spin" />}
          </span>
          <span className="sr-only">Model &amp; hardware</span>
        </button>
      </div>
    </nav>
  );
}

function ShieldMark() {
  return (
    <svg
      width="18"
      height="18"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M12 3l7 3v6c0 4.5-3 7.6-7 9-4-1.4-7-4.5-7-9V6z" />
      <path d="M9 12l2 2 4-4" />
    </svg>
  );
}
