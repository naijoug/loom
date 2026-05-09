import type { ReactNode } from "react";
import "./AppLayout.css";

interface AppLayoutProps {
  sidebar: ReactNode;
  header: ReactNode;
  children: ReactNode;
}

export function AppLayout({ sidebar, header, children }: AppLayoutProps) {
  return (
    <div className="app-layout">
      <aside className="app-sidebar">{sidebar}</aside>
      <main className="app-main">
        <header className="app-header">{header}</header>
        <div className="app-content">{children}</div>
      </main>
    </div>
  );
}
