import { BookIcon, FolderIcon, SettingsIcon, ShieldIcon } from "../ui/icons";
import { useProjectStore } from "../../state/projectStore";
import { useNavStore } from "../../state/navStore";

export function ActivityRail() {
  const view = useNavStore((state) => state.view);
  const showHome = useNavStore((state) => state.showHome);
  const showDiary = useNavStore((state) => state.showDiary);
  const showSettings = useNavStore((state) => state.showSettings);
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

      <button
        type="button"
        className="rail__item"
        aria-current={view === "settings"}
        onClick={() => {
          closeProject();
          showSettings();
        }}
        title="Settings — appearance, theme and density"
      >
        <SettingsIcon size={18} />
        <span className="sr-only">Settings</span>
      </button>
    </nav>
  );
}
