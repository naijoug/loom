import { ThemeProvider } from "./contexts/ThemeContext";
import type { AppState } from "./state/model";
import { AppStateProvider } from "./state/AppStateContext";
import { AppShell } from "./shell/AppShell";
import "./App.css";

export function AppContent() {
  return <AppShell />;
}

function App({ initialState }: { initialState?: AppState }) {
  return (
    <ThemeProvider>
      <AppStateProvider initialStateOverride={initialState}>
        <AppContent />
      </AppStateProvider>
    </ThemeProvider>
  );
}

export default App;
