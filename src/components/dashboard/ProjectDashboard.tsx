import { useEffect } from "react";
import { ProjectHeader } from "../layout/ProjectHeader";
import { Sidebar } from "../layout/Sidebar";
import { DirectoryView } from "../files/DirectoryView";
import { WorkspaceLayout } from "../layout/WorkspaceLayout";
import { ChatView } from "../chat/ChatView";
import { ActivityLog } from "../activity/ActivityLog";
import { ReportView } from "../research/ReportView";
import { ResearchPlanView } from "../research/ResearchPlanView";
import { ProjectCalendarView as ProjectJournal } from "../calendar/ProjectCalendarView";
import { useProjectStore } from "../../state/projectStore";
import { useSettingsStore } from "../../state/settingsStore";
import { useNavStore } from "../../state/navStore";
import { RuntimeConfigProvider } from "../settings/runtime-config";
import type { Project } from "../../types";

export function ProjectDashboard({ project }: { project: Project }) {
  const tab = useNavStore((state) => state.activeProjectTab);
  const setTab = useNavStore((state) => state.setActiveProjectTab);
  const sidebarCollapsed = useSettingsStore((state) => state.sidebarCollapsed);
  const toggleSidebar = useSettingsStore((state) => state.toggleSidebar);
  const closeProject = useProjectStore((state) => state.closeProject);
  const setDiary = useProjectStore((state) => state.setDiary);

  // A configuration tab can be left open from the previous project; always
  // land on the workspace instead of an empty view.
  useEffect(() => {
    if (!["chat", "files", "analysis", "research", "journal", "activity"].includes(tab)) {
      setTab("chat");
    }
  }, [tab, setTab]);

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
      <RuntimeConfigProvider active={true}>
        {tab === "chat" ? (
          <ChatView project={project} />
        ) : tab === "files" ? (
          <DirectoryView project={project} />
        ) : tab === "analysis" ? (
          <ReportView project={project} />
        ) : tab === "research" ? (
          <ResearchPlanView project={project} />
        ) : tab === "journal" ? (
          <ProjectJournal project={project} />
        ) : (
          <ActivityLog project={project} />
        )}
      </RuntimeConfigProvider>
    </WorkspaceLayout>
  );
}
