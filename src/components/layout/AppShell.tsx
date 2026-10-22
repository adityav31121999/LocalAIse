import type { ReactNode } from "react";
import { ActivityRail } from "./ActivityRail";
import { NetworkIndicator } from "./NetworkIndicator";
import { useProjectStore, selectActiveProject } from "../../state/projectStore";
import { useRuntimeStore } from "../../state/runtimeStore";
import { useSettingsStore } from "../../state/settingsStore";
import { useNavStore } from "../../state/navStore";
import { MonitorIcon, MoonIcon, SunIcon } from "../ui/icons";
import { runtimeDotClass, runtimeLabel } from "../../util/format";

function StatusBar() {
  const runtime = useRuntimeStore((state) => state.status);
  const sources = useProjectStore((state) => state.sources);
  const projectCount = useProjectStore((state) => state.projects.length);
  const project = useProjectStore(selectActiveProject);
  const colorMode = useSettingsStore((state) => state.colorMode);
  const cycleColorMode = useSettingsStore((state) => state.cycleColorMode);
  const view = useNavStore((state) => state.view);

  const indexed = sources.filter((file) => file.status === "indexed").length;
  const ThemeIcon = colorMode === "light" ? SunIcon : colorMode === "dark" ? MoonIcon : MonitorIcon;

  return (
    <footer className="statusbar">
      <span className="statusbar__item">
        <span className={runtimeDotClass(runtime.state)} />
        llama-server · {runtimeLabel(runtime.state)}
        {runtime.port ? <span className="mono">:{runtime.port}</span> : null}
      </span>

      <span className="statusbar__item">
        {view === "settings"
          ? "Appearance · stored on this machine"
          : view === "diary"
            ? "Personal space · stored on this machine"
            : project
              ? `Indexed ${indexed}/${sources.length} sources`
              : `${projectCount} work directories`}
      </span>

      <span className="statusbar__spacer" />

      {project ? <NetworkIndicator enabled={project.internetEnabled} /> : null}

      <button
        type="button"
        className="statusbar__item"
        onClick={cycleColorMode}
        title="Switch between system, dark and light appearance"
      >
        <ThemeIcon size={13} />
        {colorMode === "system" ? "System theme" : colorMode === "dark" ? "Dark" : "Light"}
      </button>
    </footer>
  );
}

export function AppShell({ children }: { children: ReactNode }) {
  return (
    <div className="app-shell">
      <ActivityRail />
      <div className="app-main">
        {children}
        <StatusBar />
      </div>
    </div>
  );
}
