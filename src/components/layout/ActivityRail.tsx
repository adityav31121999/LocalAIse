import {
  FolderIcon,
  HomeIcon,
  SettingsIcon,
} from "../ui/icons";
import { useProjectStore, selectActiveProject } from "../../state/projectStore";
import { useNavStore } from "../../state/navStore";
import appIcon from "../../assets/branding/localaise.ico";

/**
 * The app-level rail. It owns navigation that exists outside any one project:
 * home, the open project, model & hardware, and settings. Views
 * that belong to a project live in the project header instead.
 */
interface ActivityRailProps {
  collapsed?: boolean;
}

export function ActivityRail({ collapsed = false }: ActivityRailProps) {
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

  if (collapsed) {
    return (
      <nav className="rail rail--collapsed" aria-label="Primary navigation">
        <div className="rail__brand" title="localAIse — private, local-first workspace">
          <img className="rail__brand-mark" src={appIcon} alt="" aria-hidden="true" />
        </div>

        <div className="rail__nav">
          <button
            type="button"
            className="rail__item"
            aria-current={view === "home" && project == null}
            onClick={handleHomeClick}
            title="Home — work directories (Ctrl+1)"
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
                ? `Project workspace · ${project.name} (Ctrl+2)`
                : projects.length > 0
                ? `Open project (${projects[0].name}) (Ctrl+2)`
                : "Project workspace (Ctrl+2)"
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
            title="Settings — appearance, models and runtime (Ctrl+,)"
          >
            <SettingsIcon size={18} />
            <span className="sr-only">Settings</span>
          </button>
        </div>
      </nav>
    );
  }

  return (
    <nav className="rail" aria-label="Primary navigation">
      <div className="rail__brand" title="localAIse — private, local-first workspace">
        <img className="rail__brand-mark" src={appIcon} alt="" aria-hidden="true" />
      </div>

      <div className="rail__nav">
        <button
          type="button"
          className="rail__item"
          aria-current={view === "home" && project == null}
          onClick={handleHomeClick}
          title="Home — work directories (Ctrl+1)"
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
              ? `Project workspace · ${project.name} (Ctrl+2)`
              : projects.length > 0
              ? `Open project (${projects[0].name}) (Ctrl+2)`
              : "Project workspace (Ctrl+2)"
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
          title="Settings — appearance, models and runtime (Ctrl+,)"
        >
          <SettingsIcon size={18} />
          <span className="sr-only">Settings</span>
        </button>
      </div>
    </nav>
  );
}