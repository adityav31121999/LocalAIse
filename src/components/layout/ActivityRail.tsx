import { BookIcon, FolderIcon, ShieldIcon, PaletteIcon, CpuIcon } from "../ui/icons";
import { useProjectStore } from "../../state/projectStore";
import { useNavStore } from "../../state/navStore";

export function ActivityRail() {
  const view = useNavStore((state) => state.view);
  const showHome = useNavStore((state) => state.showHome);
  const showDiary = useNavStore((state) => state.showDiary);
  const showThemeSettings = useNavStore((state) => state.showThemeSettings);
  const showModelConfig = useNavStore((state) => state.showModelConfig);
  const closeProject = useProjectStore((state) => state.closeProject);

  return (
    <nav className="rail" aria-label="Primary">
      <div className="rail__brand" title="localAIse — private, local-first workspace">
        <ShieldIcon size={17} />
      </div>

      <div className="rail__nav">
        <button
          type="button"
          className="rail__item"
          aria-current={view === "home"}
          onClick={() => {
            closeProject();
            showHome();
          }}
          title="Work directories"
        >
          <FolderIcon size={18} />
          <span className="sr-only">Work directories</span>
        </button>

        <button
          type="button"
          className="rail__item"
          aria-current={view === "diary"}
          onClick={() => {
            closeProject();
            showDiary();
          }}
          title="Diary and personal pages"
        >
          <BookIcon size={18} />
          <span className="sr-only">Diary</span>
        </button>
      </div>

      <div className="rail__settings-buttons">
        <button
          type="button"
          className="rail__item rail__item--settings"
          aria-current={view === "theme-settings"}
          onClick={() => {
            closeProject();
            showThemeSettings();
          }}
          title="Theme settings — colors, mode, density"
        >
          <PaletteIcon size={18} />
          <span className="sr-only">Theme settings</span>
        </button>

        <button
          type="button"
          className="rail__item rail__item--settings"
          aria-current={view === "model-config"}
          onClick={() => {
            showModelConfig();
          }}
          title="Model configuration — inference, hardware, models"
        >
          <CpuIcon size={18} />
          <span className="sr-only">Model configuration</span>
        </button>
      </div>
    </nav>
  );
}
