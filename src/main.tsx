import React from "react";
import ReactDOM from "react-dom/client";
import "./styles/theme.css";
import App from "./App";
import { createPreviewState } from "./features/chat/preview";
import { UpdateProvider } from "./features/app-update/UpdateProvider";
import { ErrorBoundary } from "./components/common/ErrorBoundary";
import { I18nProvider } from "./i18n";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode><I18nProvider><ErrorBoundary><UpdateProvider><App initialState={import.meta.env.DEV && new URLSearchParams(window.location.search).get("fixture") === "simple-chat" ? createPreviewState() : undefined} /></UpdateProvider></ErrorBoundary></I18nProvider></React.StrictMode>,
);
