import { invoke } from "@tauri-apps/api/core";
import { useCallback } from "react";
import type { CreateTaskInput, Task } from "../domain";
import { useAppState } from "../state/AppStateContext";

function toErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function useTaskBridge() {
  const { dispatch } = useAppState();

  const loadTasks = useCallback(
    async (projectPath: string) => {
      dispatch({ type: "tasks/loadStarted" });
      try {
        const tasks = await invoke<Task[]>("list_tasks", { projectPath });
        dispatch({ type: "tasks/loaded", tasks });
      } catch (error) {
        dispatch({ type: "tasks/loadFailed", error: toErrorMessage(error) });
      }
    },
    [dispatch],
  );

  const createTask = useCallback(
    async (input: CreateTaskInput) => {
      dispatch({ type: "tasks/loadStarted" });
      try {
        const task = await invoke<Task>("create_task", { input });
        dispatch({ type: "tasks/upserted", task });
        return task;
      } catch (error) {
        dispatch({ type: "tasks/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  const appendFeedback = useCallback(
    async (projectPath: string, taskId: string, commandRunId: string | undefined, content: string) => {
      try {
        const task = await invoke<Task>("append_feedback", {
          input: { projectPath, taskId, commandRunId, content },
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

  const generateRepairContext = useCallback(
    async (projectPath: string, taskId: string) => {
      try {
        const task = await invoke<Task>("generate_repair_context", { projectPath, taskId });
        dispatch({ type: "tasks/upserted", task });
        return task;
      } catch (error) {
        dispatch({ type: "tasks/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  return { loadTasks, createTask, appendFeedback, generateRepairContext };
}
