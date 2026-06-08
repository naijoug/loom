import { useEffect } from "react";
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

  useEffect(() => {
    if (state.projects.current) {
      void loadTasks(state.projects.current.path);
    }
  }, [loadTasks, state.projects.current]);

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
