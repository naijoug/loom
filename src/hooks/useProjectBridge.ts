import { useCallback } from "react";
import { invokeCommand, TAURI_COMMANDS } from "../api";
import type { ProjectSummary } from "../domain";
import { useAppState } from "../state/AppStateContext";
import { hasTauriRuntime } from "./runtime";

function toErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function useProjectBridge() {
  const { dispatch } = useAppState();

  const loadRecentProjects = useCallback(async () => {
    if (!hasTauriRuntime()) {
      return;
    }

    dispatch({ type: "projects/loadStarted" });

    try {
      const projects = await invokeCommand<ProjectSummary[]>(TAURI_COMMANDS.listRecentProjects);
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

        const project = await invokeCommand<ProjectSummary>(TAURI_COMMANDS.registerProject, {
          path,
        });
        dispatch({ type: "projects/registered", project });
        return project;
      } catch (error) {
        dispatch({ type: "projects/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  const removeRecentProject = useCallback(
    async (project: ProjectSummary) => {
      dispatch({ type: "projects/loadStarted" });

      try {
        if (!hasTauriRuntime()) {
          dispatch({
            type: "projects/removed",
            projectId: project.id,
            projectPath: project.path,
          });
          return true;
        }

        const projects = await invokeCommand<ProjectSummary[]>(TAURI_COMMANDS.removeRecentProject, {
          projectId: project.id,
        });
        dispatch({ type: "projects/recentLoaded", projects });
        dispatch({
          type: "projects/removed",
          projectId: project.id,
          projectPath: project.path,
        });
        return true;
      } catch (error) {
        dispatch({ type: "projects/loadFailed", error: toErrorMessage(error) });
        return false;
      }
    },
    [dispatch],
  );

  return {
    loadRecentProjects,
    removeRecentProject,
    registerProject,
  };
}
