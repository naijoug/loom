import { useEffect, useState } from "react";
import { ThemeProvider } from "./contexts/ThemeContext";
import { AppStateProvider } from "./state/AppStateContext";
import { AppLayout } from "./layouts/AppLayout";
import { Sidebar } from "./components/Sidebar";
import { Header } from "./components/Header";
import { Board } from "./components/Board";
import { WorkspaceSplit } from "./components/Workspace";
import { SettingsPage } from "./components/Settings";
import { useAppState } from "./state/AppStateContext";
import { useTaskBridge } from "./hooks/useTaskBridge";
import "./App.css";

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
    return (
      <SettingsPage
        onBack={() => dispatch({ type: "app/viewSelected", view: "board" })}
      />
    );
  }

  const content = state.app.currentView === "board" ? <Board /> : <WorkspaceSplit />;

  return (
    <AppLayout
      sidebar={<Sidebar />}
      header={<Header />}
      sidebarCollapsed={sidebarCollapsed}
      onToggleSidebar={() => setSidebarCollapsed((value) => !value)}
    >
      {content}
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
