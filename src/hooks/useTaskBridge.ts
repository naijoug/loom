import { invoke } from "@tauri-apps/api/core";
import { useCallback } from "react";
import type { CreateTaskInput, PlanningDecisionInput, Task } from "../domain";
import { useAppState } from "../state/AppStateContext";
import { hasTauriRuntime } from "./runtime";

function toErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function useTaskBridge() {
  const { dispatch } = useAppState();

  const loadTasks = useCallback(
    async (projectPath: string) => {
      dispatch({ type: "tasks/loadStarted" });
      try {
        if (!hasTauriRuntime()) {
          dispatch({ type: "tasks/loaded", tasks: [] });
          return;
        }

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
        if (!hasTauriRuntime()) {
          throw new Error("Task creation requires the Tauri desktop runtime.");
        }

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

  const recordPlanningDecision = useCallback(
    async (input: PlanningDecisionInput) => {
      try {
        const task = await invoke<Task>("record_planning_decision", { input });
        dispatch({ type: "tasks/upserted", task });
        return task;
      } catch (error) {
        dispatch({ type: "tasks/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  const confirmPlan = useCallback(
    async (projectPath: string, taskId: string) => {
      try {
        const task = await invoke<Task>("confirm_plan", { projectPath, taskId });
        dispatch({ type: "tasks/upserted", task });
        return task;
      } catch (error) {
        dispatch({ type: "tasks/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  const startTodo = useCallback(
    async (projectPath: string | null, taskId: string, todoId: string) => {
      dispatch({ type: "tasks/todoSelected", taskId, todoId });

      if (!projectPath || !hasTauriRuntime()) {
        return null;
      }

      try {
        const task = await invoke<Task>("start_todo", { projectPath, taskId, todoId });
        dispatch({ type: "tasks/upserted", task });
        return task;
      } catch (error) {
        dispatch({ type: "tasks/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  const completeTodo = useCallback(
    async (projectPath: string | null, taskId: string, todoId: string) => {
      dispatch({ type: "tasks/todoCompleted", taskId, todoId });

      if (!projectPath || !hasTauriRuntime()) {
        return null;
      }

      try {
        const task = await invoke<Task>("complete_todo", { projectPath, taskId, todoId });
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

  return {
    loadTasks,
    createTask,
    confirmPlan,
    startTodo,
    completeTodo,
    appendFeedback,
    recordPlanningDecision,
    generateRepairContext,
  };
}
