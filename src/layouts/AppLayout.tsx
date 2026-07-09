import type { ReactNode } from "react";
import { PanelLeftClose, PanelLeftOpen } from "lucide-react";
import "./AppLayout.css";

interface AppLayoutProps {
  sidebar: ReactNode;
  header: ReactNode;
  children: ReactNode;
  sidebarCollapsed?: boolean;
  onToggleSidebar?: () => void;
}

// Stable mount point in the header's right edge. Views (e.g. the planning room)
// portal their own panel toggle here so it sits on the same horizontal line as
// the sidebar toggle on the left, in every collapsed/expanded combination.
export const HEADER_ACTIONS_SLOT_ID = "loom-header-actions";

export function AppLayout({
  sidebar,
  header,
  children,
  sidebarCollapsed = false,
  onToggleSidebar,
}: AppLayoutProps) {
  return (
    <div className={`loom-app app-layout${sidebarCollapsed ? " sidebar-collapsed" : ""}`}>
      <aside className="loom-sidebar app-sidebar">{sidebar}</aside>
      <main className="loom-main app-main">
        <header className="loom-header app-header" data-tauri-drag-region>
          <button
            type="button"
            className="hd-icon-btn panel-toggle-button"
            title={sidebarCollapsed ? "展开侧栏" : "收起侧栏"}
            aria-label={sidebarCollapsed ? "展开侧栏" : "收起侧栏"}
            onClick={onToggleSidebar}
          >
            {sidebarCollapsed ? <PanelLeftOpen size={16} /> : <PanelLeftClose size={16} />}
          </button>
          <div className="app-header-main">{header}</div>
          <div className="hd-actions app-header-actions" id={HEADER_ACTIONS_SLOT_ID} />
        </header>
        <div className="app-content">{children}</div>
      </main>
    </div>
  );
}
