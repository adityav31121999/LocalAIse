import type { ReactNode } from "react";
import { ActivityRail } from "./ActivityRail";
import { NetworkIndicator } from "./NetworkIndicator";
import { useProjectStore, selectActiveProject } from "../../state/projectStore";
import { useRuntimeStore } from "../../state/runtimeStore";
import { useSettingsStore } from "../../state/settingsStore";
import { useNavStore } from "../../state/navStore";
import { MonitorIcon, MoonIcon, SunIcon } from "../ui/icons";
import { runtimeLabel } from "../../util/format";

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
  const context = runtime.profile?.contextTokens ?? 0;

  const scopeLabel =
    view === "settings"
      ? "Appearance · stored on this machine"
      : view === "diary"
        ? "Personal space · stored on this machine"
        : project
          ? `Indexed ${indexed}/${sources.length} sources`
          : `${projectCount} work directories`;

  return (
    <footer className="statusbar">
      <span className={`statusbar__pill statusbar__pill--${runtime.state}`}>
        <span className={`statusbar__dot statusbar__dot--${runtime.state}`} aria-hidden="true" />
        llama-server
        <span className="statusbar__pill-strong">{runtimeLabel(runtime.state)}</span>
        {runtime.port ? <span className="mono statusbar__pill-mono">:{runtime.port}</span> : null}
      </span>

      {context > 0 ? (
        <span className="statusbar__pill">
          <span className="statusbar__pill-key">Context</span>
          <span className="mono">{Math.round(context / 1024)}K</span>
        </span>
      ) : null}

      <span className="statusbar__item">{scopeLabel}</span>

      <span className="statusbar__spacer" />

      {project ? <NetworkIndicator enabled={project.internetEnabled} /> : null}

      <button
        type="button"
        className="statusbar__item statusbar__button"
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
