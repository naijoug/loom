import { useCallback } from "react";
import { invokeCommand, TAURI_COMMANDS } from "../api";
import type { Task } from "../domain";
import { useAppState } from "../state/AppStateContext";
import { hasTauriRuntime } from "./runtime";

function toErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function useImplementationReviewBridge() {
  const { dispatch } = useAppState();

  const runImplementationReviews = useCallback(
    async (projectPath: string, taskId: string, reviewerAgentIds: string[]) => {
      if (!hasTauriRuntime()) {
        dispatch({ type: "tasks/loadFailed", error: "Implementation Review requires the Tauri desktop runtime." });
        return null;
      }
      try {
        const task = await invokeCommand<Task>(TAURI_COMMANDS.runImplementationReviews, {
          input: { projectPath, taskId, reviewerAgentIds },
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

  const decideImplementationReviewFinding = useCallback(
    async (
      projectPath: string,
      taskId: string,
      findingId: string,
      decision: "resolved" | "accepted_risk" | "dismissed",
      reason: string,
    ) => {
      try {
        const task = await invokeCommand<Task>(
          TAURI_COMMANDS.decideImplementationReviewFinding,
          {
          input: { projectPath, taskId, findingId, decision, reason },
          },
        );
        dispatch({ type: "tasks/upserted", task });
        return task;
      } catch (error) {
        dispatch({ type: "tasks/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  return { runImplementationReviews, decideImplementationReviewFinding };
}
