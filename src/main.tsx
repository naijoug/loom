import React, { lazy, Suspense } from "react";
import ReactDOM from "react-dom/client";
import "./styles/theme.css";

const App = lazy(() => import("./App"));
const PlanningPreviewApp = lazy(() =>
  import("./preview/PlanningPreviewApp").then((module) => ({ default: module.PlanningPreviewApp })),
);

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
    <Suspense fallback={<div className="app-bootstrap-loading" role="status">正在启动 Loom…</div>}>
      <RootApp />
    </Suspense>
  </React.StrictMode>,
);
