import React from "react";
import { SkillsPreviewContext } from "./features/skills/previewContext";
import { LearnPreviewContext } from "./features/learn/previewContext";
import ReactDOM from "react-dom/client";
import "./styles/theme.css";
import App from "./App";
import { createPreviewState } from "./features/chat/preview";
import { UpdateProvider } from "./components/app-update/UpdateProvider";
import { ErrorBoundary } from "./components/common/ErrorBoundary";
import { I18nProvider } from "./i18n";

async function render() {
  const skillsPreview = import.meta.env.DEV && new URLSearchParams(window.location.search).has("skillsFixture") ? (await import("./features/skills/preview")).previewSkillsApi : null;
  const learnPreview = import.meta.env.DEV && new URLSearchParams(window.location.search).has("learnFixture") ? (await import("./features/learn/preview")).previewLearnApi : null;
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode><SkillsPreviewContext.Provider value={skillsPreview}><LearnPreviewContext.Provider value={learnPreview}><I18nProvider><ErrorBoundary><UpdateProvider><App initialState={import.meta.env.DEV && new URLSearchParams(window.location.search).get("fixture") === "simple-chat" ? createPreviewState() : undefined} /></UpdateProvider></ErrorBoundary></I18nProvider></LearnPreviewContext.Provider></SkillsPreviewContext.Provider></React.StrictMode>,
  );
}
void render();
