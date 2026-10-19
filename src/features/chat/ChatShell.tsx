import type { ReactNode } from "react";
import { AppLayout } from "../../layouts/AppLayout";
import { Sidebar } from "../../components/Sidebar";
import { useAppState } from "../../state/AppStateContext";
import "../../components/Header/Header.css";

export interface ShellProps {
  children: ReactNode;
  sidebarCollapsed: boolean;
  onToggleSidebar: () => void;
}

export function ChatShell({ children, ...layout }: ShellProps) {
  const { state } = useAppState();
  return (
    <AppLayout {...layout} sidebar={<Sidebar showTasks={false} />} header={
      <div className="main-header" data-tauri-drag-region>
        <nav className="hd-breadcrumb" aria-label="面包屑">
          <span className="hd-crumb">{state.projects.current?.name ?? "未选择项目"}</span>
          <span className="hd-crumb-sep">›</span>
          <span className="hd-crumb hd-crumb-current">对话</span>
        </nav>
      </div>
    }>{children}</AppLayout>
  );
}
