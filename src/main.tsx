import React from "react";
import ReactDOM from "react-dom/client";
import "./styles/theme.css";
import App from "./App";
import { PlanningPreviewApp } from "./preview/PlanningPreviewApp";

const previewMode = window.location.pathname.startsWith("/preview/planning");

if (previewMode) {
  localStorage.setItem("loom-theme", "dark");
  document.documentElement.setAttribute("data-theme", "dark");
}

const RootApp = previewMode
  ? PlanningPreviewApp
  : App;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <RootApp />
  </React.StrictMode>,
);
