import { ThemeProvider } from "./contexts/ThemeContext";
import { AppStateProvider } from "./state/AppStateContext";
import { AppLayout } from "./layouts/AppLayout";
import { Sidebar } from "./components/Sidebar";
import { Header } from "./components/Header";
import { WorkspaceSplit } from "./components/Workspace";
import "./App.css";

function AppContent() {
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
