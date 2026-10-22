import { useEffect, useState } from "react";
import { ProjectHeader } from "../layout/ProjectHeader";
import { DirectoryView } from "../files/DirectoryView";
import { WorkspaceLayout } from "../layout/WorkspaceLayout";
import { ChatView } from "../chat/ChatView";
import { ActivityLog } from "../activity/ActivityLog";
import { ReportView } from "../research/ReportView";
import { ResearchPlanView } from "../research/ResearchPlanView";
import { ProjectDiary } from "../personal/ProjectDiary";
import { useProjectStore } from "../../state/projectStore";
import { useSettingsStore } from "../../state/settingsStore";
import type { Project, WorkspaceTab } from "../../types";

export function ProjectDashboard({ project }: { project: Project }) {
  const [tab, setTab] = useState<WorkspaceTab>("chat");
  const sidebarCollapsed = useSettingsStore((state) => state.sidebarCollapsed);
  const toggleSidebar = useSettingsStore((state) => state.toggleSidebar);
  const closeProject = useProjectStore((state) => state.closeProject);
  const setDiary = useProjectStore((state) => state.setDiary);

  // Turning the project diary off hides its tab; never leave it selected.
  useEffect(() => {
    if (tab === "diary" && !project.diaryEnabled) setTab("chat");
  }, [tab, project.diaryEnabled]);

  return (
    <WorkspaceLayout
      sidebarCollapsed={sidebarCollapsed}
      sidebar={<DirectoryView project={project} />}
      header={
        <ProjectHeader
          project={project}
          tab={tab}
          onTabChange={setTab}
          onBack={closeProject}
          onToggleDiary={(enabled) => void setDiary(enabled)}
          onToggleSidebar={toggleSidebar}
          sidebarCollapsed={sidebarCollapsed}
        />
      }
    >
      {tab === "chat" ? (
        <ChatView project={project} />
      ) : tab === "analysis" ? (
        <ReportView project={project} />
      ) : tab === "research" ? (
        <ResearchPlanView project={project} />
      ) : tab === "diary" ? (
        <ProjectDiary project={project} />
      ) : (
        <ActivityLog project={project} />
      )}
    </WorkspaceLayout>
  );
}
