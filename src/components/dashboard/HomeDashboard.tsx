import { useEffect, useMemo, useRef, useState } from "react";
import {
  CalendarIcon,
  ChevronDownIcon,
  CpuIcon,
  FolderIcon,
  GlobeIcon,
  LayersIcon,
  PlusIcon,
  SearchIcon,
  SparkIcon,
} from "../ui/icons";
import { ProjectCard } from "./ProjectCard";
import { isTauri } from "../../api";
import { useProjectStore } from "../../state/projectStore";
import { useRuntimeStore } from "../../state/runtimeStore";
import { useNavStore } from "../../state/navStore";
import type { Project, RuntimeState } from "../../types";

const RUNTIME_LABEL: Record<RuntimeState, string> = {
  starting: "Starting",
  ready: "Ready",
  stopped: "Stopped",
  error: "Error",
};

/** The quantization tag in a model file name, when it has one. */
function quantizationOf(model: string): string | null {
  const match = /\b(i?q\d(?:_[a-z0-9]+)*|f16|f32|bf16)\b/i.exec(model);
  return match ? match[1].toUpperCase() : null;
}

export function HomeDashboard() {
  const projects = useProjectStore((state) => state.projects);
  const status = useProjectStore((state) => state.status);
  const error = useProjectStore((state) => state.error);
  const openProject = useProjectStore((state) => state.openProject);
  const addFolder = useProjectStore((state) => state.addFolder);
  const createProject = useProjectStore((state) => state.createProject);
  const renameProject = useProjectStore((state) => state.renameProject);
  const removeProject = useProjectStore((state) => state.removeProject);
  const setProjectInternet = useProjectStore((state) => state.setProjectInternet);
  const runtime = useRuntimeStore((state) => state.status);
  const showDiary = useNavStore((state) => state.showDiary);
  const showSettings = useNavStore((state) => state.showSettings);

  const [query, setQuery] = useState("");
  const [renameTarget, setRenameTarget] = useState<Project | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [removeTarget, setRemoveTarget] = useState<Project | null>(null);
  const [deleteFiles, setDeleteFiles] = useState(false);
  const [busy, setBusy] = useState(false);
  const [netOpen, setNetOpen] = useState(false);
  const netRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!netOpen) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!netRef.current?.contains(event.target as Node)) setNetOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setNetOpen(false);
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [netOpen]);

  const submitRename = async () => {
    if (!renameTarget || busy) return;
    setBusy(true);
    try {
      if (await renameProject(renameTarget.id, renameValue)) setRenameTarget(null);
    } finally {
      setBusy(false);
    }
  };

  const submitRemove = async () => {
    if (!removeTarget || busy) return;
    setBusy(true);
    try {
      if (await removeProject(removeTarget.id, deleteFiles)) {
        setRemoveTarget(null);
        setDeleteFiles(false);
      }
    } finally {
      setBusy(false);
    }
  };

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return projects;
    return projects.filter(
      (project) =>
        project.name.toLowerCase().includes(needle) || project.path.toLowerCase().includes(needle),
    );
  }, [projects, query]);

  const totalSources = projects.reduce((sum, project) => sum + project.sourceCount, 0);
  const internetProjects = projects.filter((project) => project.internetEnabled).length;
  const anyInternet = internetProjects > 0;
  const quantization = quantizationOf(runtime.model ?? "");

  const toggleAllInternet = async () => {
    if (busy || projects.length === 0) return;
    setBusy(true);
    try {
      await Promise.all(
        projects.map((project) => setProjectInternet(project.id, !anyInternet)),
      );
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="home">
      <div className="home__inner">
        <div className="home__head">
          <div>
            <div className="home__eyebrow">
              <SparkIcon size={13} />
              Private workspace
            </div>
            <h1 className="home__title">Your work directories</h1>
            <p className="home__subtitle">
              Each folder below is a project you own. Open one to see its sources and ask questions with a
              model running on this machine.
            </p>
          </div>

          <div className="home__actions">
            <button
              type="button"
              className="btn btn--subtle"
              onClick={() => showDiary()}
              title="Personal space journal · stored in the hidden .calendar folder"
            >
              <CalendarIcon size={14} />
              Personal journal
            </button>
            <button
              type="button"
              className="btn btn--secondary"
              disabled={!isTauri}
              title={
                isTauri
                  ? "Register a folder you already have"
                  : "Picking a folder needs the desktop app"
              }
              onClick={() => void addFolder()}
            >
              <FolderIcon size={14} />
              Open folder
            </button>
            <button
              type="button"
              className="btn btn--primary"
              disabled={!isTauri}
              title={
                isTauri ? "Create a new folder for a project" : "Creating a project needs the desktop app"
              }
              onClick={() => void createProject("general")}
            >
              <PlusIcon size={15} />
              New project
            </button>
          </div>
        </div>

        <div className="metric-grid">
          <div className="metric">
            <span className="metric__icon">
              <FolderIcon size={16} />
            </span>
            <div className="metric__body">
              <span className="metric__label">Work directories</span>
              <span className="metric__value">{projects.length}</span>
              <span className="metric__sub">
                {totalSources === 0
                  ? "No sources indexed yet"
                  : `${totalSources} source${totalSources === 1 ? "" : "s"} indexed`}
              </span>
            </div>
          </div>

          <div className="metric">
            <span className="metric__icon metric__icon--research">
              <LayersIcon size={16} />
            </span>
            <div className="metric__body">
              <span className="metric__label">Indexed sources</span>
              <span className="metric__value">{totalSources}</span>
              <span className="metric__sub">
                {projects.length === 0
                  ? "Add a folder to begin"
                  : `across ${projects.length} project${projects.length === 1 ? "" : "s"}`}
              </span>
            </div>
          </div>

          <div className="metric metric--model">
            <span className="metric__icon metric__icon--success">
              <CpuIcon size={16} />
            </span>
            <div className="metric__body">
              <span className="metric__label">
                <span className={`statusbar__dot statusbar__dot--${runtime.state}`} aria-hidden="true" />
                Local model
              </span>
              <span className="metric__value metric__value--model truncate" title={runtime.model}>
                {runtime.model || "No model"}
              </span>
              <span className="metric__meta-row">
                {quantization ? <span className="badge badge--local">{quantization}</span> : null}
                <span className="metric__sub">{RUNTIME_LABEL[runtime.state]}</span>
              </span>
            </div>
            <button
              type="button"
              className="btn btn--ghost btn--sm metric__action"
              onClick={() => showSettings("model-runtime")}
              title="Change the model and its runtime settings"
            >
              Change
            </button>
          </div>

          <div className="metric metric--net" ref={netRef}>
            <span className="metric__icon metric__icon--attention">
              <GlobeIcon size={16} />
            </span>
            <div className="metric__body">
              <span className="metric__label">Internet</span>
              <button
                type="button"
                className="metric__switch-row"
                aria-expanded={netOpen}
                aria-haspopup="menu"
                onClick={() => setNetOpen((open) => !open)}
                title="Choose which projects may reach the internet"
              >
                <span
                  className="switch"
                  data-on={anyInternet ? "true" : "false"}
                  aria-hidden="true"
                >
                  <span className="switch__thumb" />
                </span>
                <span className="metric__value metric__value--sm">
                  {projects.length === 0
                    ? "No projects"
                    : anyInternet
                      ? `On in ${internetProjects} of ${projects.length}`
                      : "Off everywhere"}
                </span>
                <ChevronDownIcon size={12} className="metric__caret" />
              </button>
              <span className="metric__sub">
                {anyInternet ? "Web answers cite their sources" : "Answers stay in local files"}
              </span>
            </div>

            {netOpen && (
              <div className="card-menu metric__popover" role="menu">
                <div className="metric__popover-head">
                  <span>Internet per project</span>
                  <span className="mono">{internetProjects}/{projects.length}</span>
                </div>
                {projects.length === 0 ? (
                  <p className="metric__popover-empty">Add a project to set its network policy.</p>
                ) : (
                  projects.map((item) => (
                    <button
                      key={item.id}
                      type="button"
                      role="menuitemcheckbox"
                      aria-checked={item.internetEnabled}
                      className="metric__net-row"
                      onClick={() => void setProjectInternet(item.id, !item.internetEnabled)}
                    >
                      <span className="truncate">{item.name}</span>
                      <span
                        className="switch switch--sm"
                        data-on={item.internetEnabled ? "true" : "false"}
                        aria-hidden="true"
                      >
                        <span className="switch__thumb" />
                      </span>
                    </button>
                  ))
                )}
                <button
                  type="button"
                  className="metric__net-all"
                  disabled={busy}
                  onClick={() => void toggleAllInternet()}
                >
                  {anyInternet ? "Turn off for every project" : "Turn on for every project"}
                </button>
              </div>
            )}
          </div>
        </div>

        <div className="home__toolbar">
          <label className="input">
            <SearchIcon size={14} />
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search folders by name or path"
              aria-label="Search work directories"
            />
          </label>
        </div>

        <div className="section-head">
          <h2>Work directories</h2>
          <span>
            {filtered.length === projects.length
              ? `${projects.length} total`
              : `${filtered.length} of ${projects.length}`}
          </span>
        </div>

        {error ? <p className="muted">Could not list folders: {error}</p> : null}

        {status === "loading" && projects.length === 0 ? (
          <p className="muted">
            <span className="spinner" aria-hidden="true" /> Reading your folders…
          </p>
        ) : filtered.length === 0 ? (
          <div className="empty-state">
            <h3>No folders to show</h3>
            <p>
              {projects.length === 0
                ? "Add a folder to get started. Your files stay on this machine and never sync anywhere."
                : `Nothing matches “${query}”. Try a different name or path.`}
            </p>
          </div>
        ) : (
          <div className="projects-grid">
            {filtered.map((project) => (
              <ProjectCard
                key={project.id}
                project={project}
                totalSources={totalSources}
                share={totalSources > 0 ? (project.sourceCount / totalSources) * 100 : 0}
                onOpen={() => void openProject(project.id)}
                onRename={() => {
                  setRenameTarget(project);
                  setRenameValue(project.name);
                }}
                onRemove={() => {
                  setRemoveTarget(project);
                  setDeleteFiles(false);
                }}
              />
            ))}
          </div>
        )}
      </div>

      {renameTarget ? (
        <div className="dialog-overlay" onClick={() => setRenameTarget(null)}>
          <div className="dialog" onClick={(event) => event.stopPropagation()} role="dialog" aria-modal="true">
            <h3 className="dialog__title">Rename project</h3>
            <p className="dialog__text">
              The folder stays at <code>{renameTarget.path}</code>.
            </p>
            <input
              type="text"
              autoFocus
              className="input dialog__input"
              value={renameValue}
              onChange={(event) => setRenameValue(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") void submitRename();
                if (event.key === "Escape") setRenameTarget(null);
              }}
            />
            <div className="dialog__actions">
              <button type="button" className="btn btn--subtle" onClick={() => setRenameTarget(null)}>
                Cancel
              </button>
              <button
                type="button"
                className="btn btn--primary"
                disabled={busy || !renameValue.trim()}
                onClick={() => void submitRename()}
              >
                Save name
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {removeTarget ? (
        <div className="dialog-overlay" onClick={() => setRemoveTarget(null)}>
          <div className="dialog" onClick={(event) => event.stopPropagation()} role="dialog" aria-modal="true">
            <h3 className="dialog__title dialog__title--danger">Remove “{removeTarget.name}”?</h3>
            <p className="dialog__text">
              The app forgets this project and stops listing it. Your files can stay exactly where they
              are.
            </p>
            <label className="dialog__check">
              <input
                type="checkbox"
                checked={deleteFiles}
                onChange={(event) => setDeleteFiles(event.target.checked)}
              />
              <span>
                Also delete the folder from disk
                <span className="muted dialog__check-path">{removeTarget.path}</span>
              </span>
            </label>
            <div className="dialog__actions">
              <button type="button" className="btn btn--subtle" onClick={() => setRemoveTarget(null)}>
                Cancel
              </button>
              <button
                type="button"
                className="btn btn--danger"
                disabled={busy}
                onClick={() => void submitRemove()}
              >
                {deleteFiles ? "Delete project and files" : "Remove from app"}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
