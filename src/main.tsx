import React from "react";
import { SkillsPreviewContext } from "./features/skills/previewContext";
import ReactDOM from "react-dom/client";
import "./styles/theme.css";
import App from "./App";
import { createPreviewState } from "./features/chat/preview";
import { UpdateProvider } from "./features/app-update/UpdateProvider";
import { ErrorBoundary } from "./components/common/ErrorBoundary";
import { I18nProvider } from "./i18n";

async function render() {
  const skillsPreview = import.meta.env.DEV && new URLSearchParams(window.location.search).has("skillsFixture") ? (await import("./features/skills/preview")).previewSkillsApi : null;
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode><SkillsPreviewContext.Provider value={skillsPreview}><I18nProvider><ErrorBoundary><UpdateProvider><App initialState={import.meta.env.DEV && new URLSearchParams(window.location.search).get("fixture") === "simple-chat" ? createPreviewState() : undefined} /></UpdateProvider></ErrorBoundary></I18nProvider></SkillsPreviewContext.Provider></React.StrictMode>,
  );
}
void render();
