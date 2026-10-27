import {
  FolderIcon,
  HomeIcon,
  SettingsIcon,
} from "../ui/icons";
import { useProjectStore, selectActiveProject } from "../../state/projectStore";
import { useNavStore } from "../../state/navStore";

/**
 * The app-level rail. It owns navigation that exists outside any one project:
 * home, the open project, model & hardware, and settings. Views
 * that belong to a project live in the project header instead.
 */
export function ActivityRail() {
  const view = useNavStore((state) => state.view);
  const showHome = useNavStore((state) => state.showHome);
  const showSettings = useNavStore((state) => state.showSettings);

  const projects = useProjectStore((state) => state.projects);
  const project = useProjectStore(selectActiveProject);
  const openProject = useProjectStore((state) => state.openProject);
  const closeProject = useProjectStore((state) => state.closeProject);

  const handleHomeClick = () => {
    closeProject();
    showHome();
  };

  const handleProjectClick = () => {
    if (project) showHome();
    else if (projects.length > 0) void openProject(projects[0].id);
    else showHome();
  };

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
          title="Home \u2014 work directories"
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
              ? `Project workspace \u00b7 ${project.name}`
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
          onClick={() => showSettings("appearance")}
          title="Settings \u2014 appearance, models and runtime"
        >
          <SettingsIcon size={18} />
          <span className="sr-only">Settings</span>
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