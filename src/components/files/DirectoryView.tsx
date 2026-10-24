import { useCallback, useEffect, useRef, useState } from "react";
import {
  ChevronRightIcon,
  DocIcon,
  FolderIcon,
} from "../ui/icons";
import { listDirectory } from "../../api/files";
import { useProjectEvents } from "../../hooks/useProjectEvents";
import type { DirectoryEntry, Project } from "../../types";

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

interface NodeProps {
  project: Project;
  path: string;
  name: string;
  isDir: boolean;
  depth: number;
}

function TreeNode({ project, path, name, isDir, depth }: NodeProps) {
  const [expanded, setExpanded] = useState(false);
  const [children, setChildren] = useState<DirectoryEntry[]>([]);
  const [status, setStatus] = useState<"idle" | "loading" | "ready" | "error">("idle");
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    if (!isDir) return;
    setStatus("loading");
    listDirectory(project.id, path)
      .then((items) => {
        setChildren(items);
        setStatus("ready");
        setError(null);
      })
      .catch((loadError) => {
        setChildren([]);
        setError(message(loadError));
        setStatus("error");
      });
  }, [project.id, path, isDir]);

  useEffect(() => {
    if (expanded && status === "idle") load();
  }, [expanded, status, load]);

  if (!isDir) {
    return (
      <div className="files__row" style={{ paddingLeft: depth * 14 }} role="listitem">
        <span className="files__icon">
          <DocIcon size={16} />
        </span>
        <span className="files__name truncate">{name}</span>
      </div>
    );
  }

  return (
    <div role="listitem">
      <div className="files__row" style={{ paddingLeft: depth * 14 }}>
        <button
          type="button"
          className="files__item"
          onClick={() => setExpanded((value) => !value)}
          title={path || project.name}
        >
          <span className="files__icon files__icon--folder">
            <FolderIcon size={16} />
          </span>
          <ChevronRightIcon
            size={12}
            style={{ transform: expanded ? "rotate(90deg)" : "none", transition: "transform 120ms" }}
          />
          <span className="files__name truncate">{name}</span>
        </button>
      </div>
      {expanded ? (
        <div role="list">
          {status === "loading" ? (
            <p className="files__note" style={{ paddingLeft: (depth + 1) * 14 }}>
              Reading the folder…
            </p>
          ) : status === "error" ? (
            <p className="files__note" style={{ paddingLeft: (depth + 1) * 14 }}>
              {error}
            </p>
          ) : children.length === 0 ? (
            <p className="files__note" style={{ paddingLeft: (depth + 1) * 14 }}>
              Empty
            </p>
          ) : (
            children.map((entry) => (
              <TreeNode
                key={entry.path}
                project={project}
                path={entry.path}
                name={entry.name}
                isDir={entry.isDir}
                depth={depth + 1}
              />
            ))
          )}
        </div>
      ) : null}
    </div>
  );
}

function RootNode({ project }: { project: Project }) {
  const [children, setChildren] = useState<DirectoryEntry[]>([]);
  const [status, setStatus] = useState<"idle" | "loading" | "ready" | "error">("idle");
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);

  const load = useCallback(() => {
    setStatus("loading");
    listDirectory(project.id, "")
      .then((items) => {
        setChildren(items);
        setStatus("ready");
        setError(null);
      })
      .catch((loadError) => {
        setChildren([]);
        setError(message(loadError));
        setStatus("error");
      });
  }, [project.id]);

  useEffect(() => {
    load();
  }, [load, revision]);

  const watching = useProjectEvents(project.id);
  const wasWatching = useRef(false);
  useEffect(() => {
    if (wasWatching.current && !watching) setRevision((v) => v + 1);
    wasWatching.current = watching;
  }, [watching]);

  return (
    <div role="list">
      {status === "loading" ? (
        <p className="files__note" style={{ paddingLeft: 14 }}>
          Reading the folder…
        </p>
      ) : status === "error" ? (
        <p className="files__note" style={{ paddingLeft: 14 }}>
          {error}
        </p>
      ) : children.length === 0 ? (
        <p className="files__note" style={{ paddingLeft: 14 }}>
          Empty
        </p>
      ) : (
        children.map((entry) => (
          <TreeNode
            key={entry.path}
            project={project}
            path={entry.path}
            name={entry.name}
            isDir={entry.isDir}
            depth={0}
          />
        ))
      )}
    </div>
  );
}

export function DirectoryView({ project }: { project: Project }) {
  return (
    <div className="files">
      <div className="files__scroll">
        <RootNode project={project} />
      </div>
    </div>
  );
}
