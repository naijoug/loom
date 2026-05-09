import { ThemeProvider } from "./contexts/ThemeContext";
import { AppStateProvider } from "./state/AppStateContext";
import { AppLayout } from "./layouts/AppLayout";
import { Sidebar } from "./components/Sidebar";
import { Header } from "./components/Header";
import { WorkspaceSplit } from "./components/Workspace";
import { SettingsPage } from "./components/Settings";
import { useAppState } from "./state/AppStateContext";
import "./App.css";

function AppContent() {
  const { state, dispatch } = useAppState();

  if (state.app.currentView === "settings") {
    return (
      <SettingsPage
        onBack={() => dispatch({ type: "app/viewSelected", view: "workspace" })}
      />
    );
  }

  return (
    <AppLayout 
      sidebar={<Sidebar />} 
      header={<Header />}
    >
      <WorkspaceSplit />
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
