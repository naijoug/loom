import { invoke } from "@tauri-apps/api/core";
import { useCallback } from "react";
import {
  EMPTY_PROJECT_AGENT_PREFERENCES,
  type ProjectAgentPreferences,
} from "../domain";
import { hasTauriRuntime } from "./runtime";

export function useProjectPreferencesBridge() {
  const loadProjectAgentPreferences = useCallback(async (projectPath: string) => {
    if (!hasTauriRuntime()) {
      return { ...EMPTY_PROJECT_AGENT_PREFERENCES };
    }
    return invoke<ProjectAgentPreferences>("load_project_agent_preferences", { projectPath });
  }, []);

  const saveProjectAgentPreferences = useCallback(
    async (projectPath: string, preferences: ProjectAgentPreferences) => {
      if (!hasTauriRuntime()) {
        return { ...preferences, updatedAtMs: Date.now() };
      }
      return invoke<ProjectAgentPreferences>("save_project_agent_preferences", {
        projectPath,
        preferences,
      });
    },
    [],
  );

  return { loadProjectAgentPreferences, saveProjectAgentPreferences };
}
