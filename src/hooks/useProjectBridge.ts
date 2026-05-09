import { invoke } from "@tauri-apps/api/core";
import { useCallback } from "react";
import type { ProjectSummary } from "../domain";
import { useAppState } from "../state/AppStateContext";
import { hasTauriRuntime } from "./runtime";

function toErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function useProjectBridge() {
  const { dispatch } = useAppState();

  const loadRecentProjects = useCallback(async () => {
    dispatch({ type: "projects/loadStarted" });

    try {
      if (!hasTauriRuntime()) {
        dispatch({ type: "projects/recentLoaded", projects: [] });
        return;
      }

      const projects = await invoke<ProjectSummary[]>("list_recent_projects");
      dispatch({ type: "projects/recentLoaded", projects });
    } catch (error) {
      dispatch({ type: "projects/loadFailed", error: toErrorMessage(error) });
    }
  }, [dispatch]);

  const registerProject = useCallback(
    async (path: string) => {
      dispatch({ type: "projects/loadStarted" });

      try {
        if (!hasTauriRuntime()) {
          throw new Error("Project registration requires the Tauri desktop runtime.");
        }

        const project = await invoke<ProjectSummary>("register_project", { path });
        dispatch({ type: "projects/registered", project });
        return project;
      } catch (error) {
        dispatch({ type: "projects/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  return {
    loadRecentProjects,
    registerProject,
  };
}
