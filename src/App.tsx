import { lazy, Suspense, useEffect, useState } from "react";
import { ThemeProvider } from "./contexts/ThemeContext";
import { AppStateProvider } from "./state/AppStateContext";
import { AppLayout } from "./layouts/AppLayout";
import { Sidebar } from "./components/Sidebar";
import { Header } from "./components/Header";
import { useAppState } from "./state/AppStateContext";
import { useTaskBridge } from "./hooks/useTaskBridge";
import "./App.css";

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

function AppContent() {
  const { state, dispatch } = useAppState();
  const { loadTasks } = useTaskBridge();
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);

  useEffect(() => {
    if (state.projects.current) {
      void loadTasks(state.projects.current.path);
    }
  }, [loadTasks, state.projects.current]);

  // Esc returns from a task's detail view back to the project board.
  useEffect(() => {
    if (state.app.currentView !== "task-detail") {
      return;
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        dispatch({ type: "app/viewSelected", view: "board" });
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [state.app.currentView, dispatch]);

  if (state.app.currentView === "settings") {
    return <Suspense fallback={<ViewFallback />}><SettingsPage onBack={() => dispatch({ type: "app/viewSelected", view: "board" })} /></Suspense>;
  }

  let content;
  if (state.app.currentView === "chat") {
    content = <ChatPage />;
  } else if (state.app.currentView === "board") {
    content = <Board />;
  } else {
    content = <WorkspaceSplit />;
  }

  return (
    <AppLayout
      sidebar={<Sidebar />}
      header={<Header />}
      sidebarCollapsed={sidebarCollapsed}
      onToggleSidebar={() => setSidebarCollapsed((value) => !value)}
    >
      <Suspense fallback={<ViewFallback />}>{content}</Suspense>
    </AppLayout>
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
