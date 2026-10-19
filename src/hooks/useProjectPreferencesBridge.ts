import { useCallback } from "react";
import { invokeCommand, TAURI_COMMANDS } from "../api";
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
    return invokeCommand<ProjectAgentPreferences>(TAURI_COMMANDS.loadProjectAgentPreferences, {
      projectPath,
    });
  }, []);

  const saveProjectAgentPreferences = useCallback(
    async (projectPath: string, preferences: ProjectAgentPreferences) => {
      if (!hasTauriRuntime()) {
        return { ...preferences, updatedAtMs: Date.now() };
      }
      return invokeCommand<ProjectAgentPreferences>(TAURI_COMMANDS.saveProjectAgentPreferences, {
        projectPath,
        preferences,
      });
    },
    [],
  );

  return { loadProjectAgentPreferences, saveProjectAgentPreferences };
}
