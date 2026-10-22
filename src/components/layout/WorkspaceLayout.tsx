import type { ReactNode } from "react";

interface WorkspaceLayoutProps {
  header: ReactNode;
  sidebar: ReactNode;
  sidebarCollapsed: boolean;
  children: ReactNode;
}

export function WorkspaceLayout({
  header,
  sidebar,
  sidebarCollapsed,
  children,
}: WorkspaceLayoutProps) {
  return (
    <div className="workspace">
      {header}
      <div className={`workspace__body${sidebarCollapsed ? " workspace__body--collapsed" : ""}`}>
        <aside className="workspace__sidebar" aria-hidden={sidebarCollapsed}>
          {sidebar}
        </aside>
        <main className="workspace__main">{children}</main>
      </div>
    </div>
  );
}
