import { useMemo, useState } from "react";
import { CalendarIcon, PlusIcon, SearchIcon, SparkIcon } from "../ui/icons";
import { ProjectCard } from "./ProjectCard";
import { isTauri } from "../../api";
import { useProjectStore } from "../../state/projectStore";
import { useRuntimeStore } from "../../state/runtimeStore";
import { useNavStore } from "../../state/navStore";
import { runtimeDotClass } from "../../util/format";
import type { Project } from "../../types";

export function HomeDashboard() {
  const projects = useProjectStore((state) => state.projects);
  const status = useProjectStore((state) => state.status);
  const error = useProjectStore((state) => state.error);
  const openProject = useProjectStore((state) => state.openProject);
  const addFolder = useProjectStore((state) => state.addFolder);
  const createProject = useProjectStore((state) => state.createProject);
  const renameProject = useProjectStore((state) => state.renameProject);
  const removeProject = useProjectStore((state) => state.removeProject);
  const runtime = useRuntimeStore((state) => state.status);
  const showDiary = useNavStore((state) => state.showDiary);
  const [query, setQuery] = useState("");
  const [renameTarget, setRenameTarget] = useState<Project | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [removeTarget, setRemoveTarget] = useState<Project | null>(null);
  const [deleteFiles, setDeleteFiles] = useState(false);
  const [busy, setBusy] = useState(false);

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
              className="btn btn--secondary"
              onClick={() => showDiary()}
              title="Personal Space Calendar & Diary · Stored in database folder .calendar"
              style={{ display: "inline-flex", alignItems: "center", gap: "6px" }}
            >
              <CalendarIcon size={14} />
              Personal Calendar
              <span
                style={{
                  fontSize: "10px",
                  padding: "1px 5px",
                  borderRadius: "3px",
                  background: "var(--surface-2)",
                  color: "var(--text-muted)",
                  fontFamily: "var(--font-mono)",
                }}
              >
                .calendar
              </span>
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

        <div className="home__stats">
          <div className="stat">
            <div className="stat__label">Work directories</div>
            <div className="stat__value">{projects.length}</div>
          </div>
          <div className="stat">
            <div className="stat__label">Indexed sources</div>
            <div className="stat__value">{totalSources}</div>
          </div>
          <div className="stat">
            <div className="stat__label">Local model</div>
            <div className="stat__value stat__value--sm">
              <span className={runtimeDotClass(runtime.state)} />
              {runtime.model}
            </div>
          </div>
          <div className="stat">
            <div className="stat__label">Internet</div>
            <div className="stat__value stat__value--sm">
              <span className={internetProjects > 0 ? "dot dot--attention" : "dot dot--success"} />
              {internetProjects > 0
                ? `On in ${internetProjects} of ${projects.length} projects`
                : "Off everywhere"}
            </div>
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
        <div
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(0, 0, 0, 0.6)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 100,
          }}
          onClick={() => setRenameTarget(null)}
        >
          <div
            style={{
              background: "var(--surface)",
              border: "1px solid var(--border)",
              borderRadius: "var(--radius-lg)",
              padding: "24px",
              width: "100%",
              maxWidth: "420px",
            }}
            onClick={(event) => event.stopPropagation()}
          >
            <h3 style={{ fontSize: "16px", fontWeight: "600", marginBottom: "8px" }}>
              Rename project
            </h3>
            <p className="muted" style={{ fontSize: "13px", marginBottom: "16px" }}>
              The folder stays at <code>{renameTarget.path}</code>.
            </p>
            <input
              type="text"
              autoFocus
              value={renameValue}
              onChange={(event) => setRenameValue(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") void submitRename();
                if (event.key === "Escape") setRenameTarget(null);
              }}
              style={{
                width: "100%",
                padding: "8px 12px",
                borderRadius: "var(--radius-md)",
                border: "1px solid var(--border)",
                background: "var(--background)",
                color: "var(--text)",
                fontSize: "14px",
                marginBottom: "20px",
              }}
            />
            <div style={{ display: "flex", justifyContent: "flex-end", gap: "8px" }}>
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
        <div
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(0, 0, 0, 0.6)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 100,
          }}
          onClick={() => setRemoveTarget(null)}
        >
          <div
            style={{
              background: "var(--surface)",
              border: "1px solid var(--border)",
              borderRadius: "var(--radius-lg)",
              padding: "24px",
              width: "100%",
              maxWidth: "440px",
            }}
            onClick={(event) => event.stopPropagation()}
          >
            <h3 style={{ fontSize: "16px", fontWeight: "600", color: "#ef4444", marginBottom: "10px" }}>
              Remove “{removeTarget.name}”?
            </h3>
            <p style={{ fontSize: "13px", color: "var(--text-secondary)", marginBottom: "14px", lineHeight: 1.5 }}>
              The app forgets this project and stops listing it. Your files can stay
              exactly where they are.
            </p>
            <label
              style={{
                display: "flex",
                alignItems: "flex-start",
                gap: "8px",
                padding: "10px",
                marginBottom: "20px",
                border: "1px solid var(--border)",
                borderRadius: "var(--radius-md)",
                fontSize: "13px",
                cursor: "pointer",
              }}
            >
              <input
                type="checkbox"
                checked={deleteFiles}
                onChange={(event) => setDeleteFiles(event.target.checked)}
                style={{ marginTop: "2px" }}
              />
              <span>
                Also delete the folder from disk
                <span className="muted" style={{ display: "block", fontSize: "12px" }}>
                  {removeTarget.path}
                </span>
              </span>
            </label>
            <div style={{ display: "flex", justifyContent: "flex-end", gap: "8px" }}>
              <button type="button" className="btn btn--subtle" onClick={() => setRemoveTarget(null)}>
                Cancel
              </button>
              <button
                type="button"
                className="btn btn--primary"
                disabled={busy}
                style={{ background: "#dc2626", color: "#fff" }}
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
