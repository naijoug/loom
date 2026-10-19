import { lazy, Suspense, useState } from "react";
import { ThemeProvider } from "./contexts/ThemeContext";
import { AppStateProvider } from "./state/AppStateContext";
import { ChatShell } from "./features/chat/ChatShell";
import { useAppState } from "./state/AppStateContext";
import "./App.css";

const WorkflowShell = lazy(() => import("./features/tasks/WorkflowShell").then((module) => ({ default: module.WorkflowShell })));
const Board = lazy(() => import("./components/Board").then((module) => ({ default: module.Board })));
const WorkspaceSplit = lazy(() =>
  import("./components/Workspace").then((module) => ({ default: module.WorkspaceSplit })),
);
const SettingsPage = lazy(() =>
  import("./components/Settings").then((module) => ({ default: module.SettingsPage })),
);
const ChatPage = lazy(() =>
  import("./features/chat").then((module) => ({ default: module.ChatPage })),
);

function ViewFallback() {
  return <div className="app-view-loading" role="status">正在加载工作区…</div>;
}

export function AppContent() {
  const { state, dispatch } = useAppState();
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);

  if (state.app.currentView === "settings") {
    return <Suspense fallback={<ViewFallback />}><SettingsPage onBack={() => dispatch({ type: "app/viewSelected", view: "chat" })} /></Suspense>;
  }

  let content;
  if (state.app.currentView === "chat") {
    content = <ChatPage />;
  } else if (state.app.currentView === "board") {
    content = <Board />;
  } else {
    content = <WorkspaceSplit />;
  }

  const Shell = state.app.currentView === "chat" ? ChatShell : WorkflowShell;
  return (
    <Suspense fallback={<ViewFallback />}>
      <Shell sidebarCollapsed={sidebarCollapsed} onToggleSidebar={() => setSidebarCollapsed((value) => !value)}>
        <Suspense fallback={<ViewFallback />}>{content}</Suspense>
      </Shell>
    </Suspense>
  );
}

function App() {
  return (
    <ThemeProvider>
      <AppStateProvider>
        <AppContent />
      </AppStateProvider>
    </ThemeProvider>
  );
}

export default App;
