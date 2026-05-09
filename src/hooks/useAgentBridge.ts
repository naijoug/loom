import { invoke } from "@tauri-apps/api/core";
import { useCallback } from "react";
import type { AgentConfig, AgentConfigInput, PlanningDiscussionInput, Task } from "../domain";
import { useAppState } from "../state/AppStateContext";
import { hasTauriRuntime } from "./runtime";

function toErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function useAgentBridge() {
  const { dispatch } = useAppState();

  const loadAgents = useCallback(async () => {
    dispatch({ type: "agents/loadStarted" });
    try {
      if (!hasTauriRuntime()) {
        dispatch({ type: "agents/loaded", agents: [] });
        return;
      }

      dispatch({ type: "agents/loaded", agents: await invoke<AgentConfig[]>("list_agents") });
    } catch (error) {
      dispatch({ type: "agents/loadFailed", error: toErrorMessage(error) });
    }
  }, [dispatch]);

  const createAgent = useCallback(
    async (input: AgentConfigInput) => {
      dispatch({ type: "agents/loadStarted" });
      try {
        const agent = await invoke<AgentConfig>("create_agent", { input });
        dispatch({ type: "agents/created", agent });
        return agent;
      } catch (error) {
        dispatch({ type: "agents/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  const runDummyPlanning = useCallback(
    async (projectPath: string, taskId: string, agentId: string) => {
      try {
        const task = await invoke<Task>("run_dummy_planning", {
          projectPath,
          taskId,
          agentId,
        });
        dispatch({ type: "tasks/upserted", task });
        return task;
      } catch (error) {
        dispatch({ type: "tasks/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  const runPlanningDiscussion = useCallback(
    async (input: PlanningDiscussionInput) => {
      try {
        if (!hasTauriRuntime()) {
          throw new Error("Planning discussion requires the Tauri desktop runtime.");
        }

        const task = await invoke<Task>("run_planning_discussion", { input });
        dispatch({ type: "tasks/upserted", task });
        return task;
      } catch (error) {
        dispatch({ type: "tasks/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  return { loadAgents, createAgent, runDummyPlanning, runPlanningDiscussion };
}
