import { useCallback } from "react";
import { invokeCommand, TAURI_COMMANDS } from "../api";
import type { PlanningDiscussionInput, Task } from "../domain";
import { useAppState } from "../state/AppStateContext";
import { hasTauriRuntime } from "./runtime";
import { useAgentCatalog } from "./useAgentCatalog";

function toErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
function assertTauriRuntime(action: string) {
  if (!hasTauriRuntime()) throw new Error(`${action} requires the Tauri desktop runtime.`);
}

/** Workflow actions; the advanced route owns the single event subscription. */
export function useAgentBridge() {
  const { state, dispatch } = useAppState();
  const catalog = useAgentCatalog();
  const runPlanningDiscussion = useCallback(
    async (input: PlanningDiscussionInput) => {
      dispatch({ type: "tasks/loadStarted" });
      try {
        assertTauriRuntime("Planning discussion");

        const agents = state.agents
          .filter((agent) => input.agentIds.includes(agent.id))
          .map((agent) => ({ id: agent.id, name: agent.name }));
        dispatch({ type: "planning/progressQueued", taskId: input.taskId, agents });

        const task = await invokeCommand<Task>(TAURI_COMMANDS.runPlanningDiscussion, { input });
        dispatch({ type: "tasks/upserted", task });
        return task;
      } catch (error) {
        dispatch({ type: "tasks/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch, state.agents],
  );

  const runPlanReviews = useCallback(
    async (projectPath: string, taskId: string) => {
      dispatch({ type: "tasks/loadStarted" });
      try {
        assertTauriRuntime("Plan review");

        const task = await invokeCommand<Task>(TAURI_COMMANDS.runPlanReviews, {
          projectPath,
          taskId,
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

  const retryPlanningAgent = useCallback(
    async (projectPath: string, taskId: string, agentId: string) => {
      dispatch({ type: "tasks/loadStarted" });
      try {
        assertTauriRuntime("Planning retry");

        const task = await invokeCommand<Task>(TAURI_COMMANDS.retryPlanningAgent, {
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

  return { ...catalog, runPlanningDiscussion, runPlanReviews, retryPlanningAgent };
}
