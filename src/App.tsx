import { lazy, Suspense } from "react";
import { ThemeProvider } from "./contexts/ThemeContext";
import type { AppState } from "./state/model";
import { AppStateProvider } from "./state/AppStateContext";
import { ChatShell } from "./features/chat/ChatShell";
import { useAppState } from "./state/AppStateContext";
import "./App.css";
import { useT } from "./i18n";

const SettingsPage = lazy(() =>
  import("./components/Settings").then((module) => ({ default: module.SettingsPage })),
);
const ChatPage = lazy(() =>
  import("./features/chat").then((module) => ({ default: module.ChatPage })),
);

function ViewFallback() {
  const t = useT();
  return <div className="app-view-loading" role="status">{t("loading")}</div>;
}

export function AppContent() {
  const { state, dispatch } = useAppState();
  return <>
    <div className="app-chat-container" hidden={state.app.currentView === "settings"}>
      <Suspense fallback={<ViewFallback />}><ChatShell><ChatPage /></ChatShell></Suspense>
    </div>
    {state.app.currentView === "settings" && <Suspense fallback={<ViewFallback />}><SettingsPage onBack={() => dispatch({ type: "app/viewSelected", view: "chat" })} /></Suspense>}
  </>;
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
