import { useEffect } from "react";
import { AppLayout } from "../../layouts/AppLayout";
import { Sidebar } from "../../components/Sidebar";
import { Header } from "../../components/Header";
import { useAppState } from "../../state/AppStateContext";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { usePlanningEvents } from "../../hooks/usePlanningEvents";
import type { ShellProps } from "../chat/ChatShell";

export function WorkflowShell({ children, ...layout }: ShellProps) {
  const { state, dispatch } = useAppState();
  const { loadTasks } = useTaskBridge();
  usePlanningEvents();
  const projectPath = state.projects.current?.path;
  useEffect(() => {
    if (projectPath) void loadTasks(projectPath);
  }, [projectPath, loadTasks]);
  useEffect(() => {
    if (state.app.currentView !== "task-detail") return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") dispatch({ type: "app/viewSelected", view: "board" });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [state.app.currentView, dispatch]);
  return <AppLayout {...layout} sidebar={<Sidebar />} header={<Header />}>{children}</AppLayout>;
}
