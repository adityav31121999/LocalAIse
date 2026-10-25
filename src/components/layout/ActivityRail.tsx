import { CalendarIcon, FolderIcon, HomeIcon, ShieldIcon, ThemeIcon, ModelIcon } from "../ui/icons";
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
  const isModelLoading = runtimeStatus.state === "starting";
  const hasModel = Boolean(runtimeStatus.model);

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

  // Shared button styles for theme/model settings
  const settingsButtonStyle = {
    width: 32,
    height: 32,
    borderRadius: 6,
    border: "none",
    background: "transparent",
    color: "#94A3B8", // slate-400
    cursor: "pointer",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    position: "relative",
    transition: "background-color 150ms ease, color 150ms ease",
  } as React.CSSProperties;

  const getActiveAccent = (type: "theme" | "model") => {
    if (type === "theme") return "#67C7F5"; // sky-300
    return "#45D6A0"; // emerald-400
  };

  const getStatusColor = () => {
    if (!hasModel) return "#FBBF24"; // amber-400
    if (isModelLoading) return "#67C7F5"; // sky-300 (spinner color)
    if (isModelReady) return "#45D6A0"; // emerald-400
    return "#94A3B8"; // slate-400
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

      {/* 2 buttons on bottom: Theme, Model settings (IDE-style technical controls) */}
      <div className="rail__settings">
        {/* Theme Settings Button — split circle appearance icon */}
        <button
          type="button"
          style={{
            ...settingsButtonStyle,
            color: view === "theme-settings" ? getActiveAccent("theme") : "#94A3B8",
            backgroundColor: view === "theme-settings" ? "rgba(103, 199, 245, 0.12)" : "transparent",
          }}
          onMouseEnter={(e) => {
            if (view !== "theme-settings") {
              e.currentTarget.style.color = "#E2E8F0";
              e.currentTarget.style.backgroundColor = "#1E293B";
            }
          }}
          onMouseLeave={(e) => {
            if (view !== "theme-settings") {
              e.currentTarget.style.color = "#94A3B8";
              e.currentTarget.style.backgroundColor = "transparent";
            }
          }}
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
          <ThemeIcon size={16} />
          {view === "theme-settings" && (
            <span
              style={{
                position: "absolute",
                left: 0,
                top: "50%",
                transform: "translateY(-50%)",
                width: 2,
                height: "60%",
                background: getActiveAccent("theme"),
                borderRadius: "0 2px 2px 0",
              }}
              aria-hidden="true"
            />
          )}
        </button>

        {/* Model Configuration Button — neural chip with status dot */}
        <button
          type="button"
          style={{
            ...settingsButtonStyle,
            color: view === "model-config" ? getActiveAccent("model") : "#94A3B8",
            backgroundColor: view === "model-config" ? "rgba(69, 214, 160, 0.12)" : "transparent",
          }}
          onMouseEnter={(e) => {
            if (view !== "model-config") {
              e.currentTarget.style.color = "#E2E8F0";
              e.currentTarget.style.backgroundColor = "#1E293B";
            }
          }}
          onMouseLeave={(e) => {
            if (view !== "model-config") {
              e.currentTarget.style.color = "#94A3B8";
              e.currentTarget.style.backgroundColor = "transparent";
            }
          }}
          aria-current={view === "model-config"}
          onClick={() => {
            if (view === "model-config") {
              closeModelConfig();
            } else {
              showModelConfig();
            }
          }}
          title={`Model settings (${runtimeStatus.model || "Local model"}${isModelReady ? " · Ready" : isModelLoading ? " · Loading" : hasModel ? "" : " · Not loaded"})`}
          aria-label="Model configuration"
        >
          <ModelIcon size={16} />
          {/* Status indicator dot / spinner */}
          <span
            style={{
              position: "absolute",
              bottom: 4,
              right: 4,
              width: 7,
              height: 7,
              borderRadius: "50%",
              background: getStatusColor(),
              boxShadow: "0 0 0 1px rgba(0,0,0,0.2)",
              animation: isModelLoading ? "rail-spin 1s linear infinite" : "none",
            }}
            title={!hasModel ? "No model loaded" : isModelLoading ? "Model loading…" : isModelReady ? "Model ready" : "Model not ready"}
            aria-hidden="true"
          >
            {isModelLoading && (
              <span
                style={{
                  display: "block",
                  width: "100%",
                  height: "100%",
                  border: "1.5px solid currentColor",
                  borderTopColor: "transparent",
                  borderRadius: "50%",
                  animation: "rail-spin 0.7s linear infinite",
                }}
              />
            )}
          </span>
          {view === "model-config" && (
            <span
              style={{
                position: "absolute",
                left: 0,
                top: "50%",
                transform: "translateY(-50%)",
                width: 2,
                height: "60%",
                background: getActiveAccent("model"),
                borderRadius: "0 2px 2px 0",
              }}
              aria-hidden="true"
            />
          )}
        </button>

        {/* Keyframes for loading spinner */}
        <style dangerouslySetInnerHTML={{ __html: `
          @keyframes rail-spin {
            to { transform: rotate(360deg); }
          }
        ` }} />
      </div>
    </nav>
  );
}
