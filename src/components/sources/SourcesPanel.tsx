import { Sidebar } from "../layout/Sidebar";
import type { Project } from "../../types";

export function SourcesPanel({ project }: { project: Project }) {
  return <Sidebar project={project} />;
}
