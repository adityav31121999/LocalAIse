import { useEffect } from "react";
import { ProjectHeader } from "../layout/ProjectHeader";
import { Sidebar } from "../layout/Sidebar";
import { DirectoryView } from "../files/DirectoryView";
import { WorkspaceLayout } from "../layout/WorkspaceLayout";
import { ChatView } from "../chat/ChatView";
import { ActivityLog } from "../activity/ActivityLog";
import { ReportView } from "../research/ReportView";
import { ResearchPlanView } from "../research/ResearchPlanView";
import { ProjectDiary } from "../personal/ProjectDiary";
import { ProjectCalendarView } from "../calendar/ProjectCalendarView";
import { useProjectStore } from "../../state/projectStore";
import { useSettingsStore } from "../../state/settingsStore";
import { useNavStore } from "../../state/navStore";
import type { Project } from "../../types";

export function ProjectDashboard({ project }: { project: Project }) {
  const tab = useNavStore((state) => state.activeProjectTab);
  const setTab = useNavStore((state) => state.setActiveProjectTab);
  const sidebarCollapsed = useSettingsStore((state) => state.sidebarCollapsed);
  const toggleSidebar = useSettingsStore((state) => state.toggleSidebar);
  const closeProject = useProjectStore((state) => state.closeProject);
  const setDiary = useProjectStore((state) => state.setDiary);

  // Turning the project diary off hides its tab; never leave it selected.
  useEffect(() => {
    if (tab === "diary" && !project.diaryEnabled) setTab("chat");
  }, [tab, project.diaryEnabled, setTab]);

  return (
    <WorkspaceLayout
      sidebarCollapsed={sidebarCollapsed}
      sidebar={<Sidebar project={project} />}
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
      ) : tab === "files" ? (
        <DirectoryView project={project} />
      ) : tab === "analysis" ? (
        <ReportView project={project} />
      ) : tab === "research" ? (
        <ResearchPlanView project={project} />
      ) : tab === "calendar" ? (
        <ProjectCalendarView project={project} />
      ) : tab === "diary" ? (
        <ProjectDiary project={project} />
      ) : (
        <ActivityLog project={project} />
      )}
    </WorkspaceLayout>
  );
}
