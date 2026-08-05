import { invoke } from "@tauri-apps/api/core";
import { useCallback } from "react";
import type { ContextBuildOptions, ContextBuildOutput, CreateTaskInput, PlanningDecisionInput, StructuredFeedbackInput, Task } from "../domain";
import { useAppState } from "../state/AppStateContext";
import { hasTauriRuntime } from "./runtime";

function toErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function useTaskBridge() {
  const { dispatch } = useAppState();

  const loadTasks = useCallback(
    async (projectPath: string, options: { silent?: boolean } = {}) => {
      if (!hasTauriRuntime()) {
        return;
      }

      if (!options.silent) {
        dispatch({ type: "tasks/loadStarted" });
      }
      try {
        const tasks = await invoke<Task[]>("list_tasks", { projectPath });
        dispatch({ type: "tasks/loaded", projectPath, tasks });
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
    async (
      projectPath: string,
      taskId: string,
      commandRunId: string | undefined,
      feedback: string | StructuredFeedbackInput,
    ) => {
      try {
        const structured = typeof feedback === "string" ? { content: feedback } : feedback;
        const task = await invoke<Task>("append_feedback", {
          input: { projectPath, taskId, commandRunId, ...structured },
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
    async (
      projectPath: string | null,
      taskId: string,
      todoId: string,
      primaryAgentId?: string,
      primaryAgentSwitchReason?: string,
    ) => {
      if (!projectPath || !hasTauriRuntime()) {
        dispatch({ type: "tasks/todoSelected", taskId, todoId });
        return null;
      }

      try {
        const task = await invoke<Task>("start_todo", {
          projectPath,
          taskId,
          todoId,
          primaryAgentId,
          primaryAgentSwitchReason,
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

  const completeTodo = useCallback(
    async (projectPath: string | null, taskId: string, todoId: string) => {
      if (!projectPath || !hasTauriRuntime()) {
        dispatch({ type: "tasks/todoCompleted", taskId, todoId });
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

  const switchPrimaryAgent = useCallback(
    async (projectPath: string, taskId: string, agentId: string, reason: string) => {
      try {
        const task = await invoke<Task>("switch_primary_agent", {
          projectPath,
          taskId,
          agentId,
          reason,
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

  const buildImplementationContext = useCallback(
    async (
      projectPath: string | null,
      taskId: string,
      todoId: string,
      options?: ContextBuildOptions,
    ) => {
      if (!projectPath || !hasTauriRuntime()) {
        return null;
      }

      try {
        return await invoke<ContextBuildOutput>("build_implementation_context", {
          projectPath,
          taskId,
          todoId,
          options,
        });
      } catch (error) {
        dispatch({ type: "tasks/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  const markReadyForTesting = useCallback(
    async (projectPath: string | null, taskId: string) => {
      if (!projectPath || !hasTauriRuntime()) {
        return null;
      }

      try {
        const task = await invoke<Task>("mark_ready_for_testing", { projectPath, taskId });
        dispatch({ type: "tasks/upserted", task });
        return task;
      } catch (error) {
        dispatch({ type: "tasks/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  const completeTask = useCallback(
    async (projectPath: string | null, taskId: string) => {
      if (!projectPath || !hasTauriRuntime()) {
        return null;
      }

      try {
        const task = await invoke<Task>("complete_task", { projectPath, taskId });
        dispatch({ type: "tasks/upserted", task });
        return task;
      } catch (error) {
        dispatch({ type: "tasks/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  const regenerateTaskSummary = useCallback(
    async (projectPath: string, taskId: string) => {
      try {
        const task = await invoke<Task>("regenerate_task_summary", { projectPath, taskId });
        dispatch({ type: "tasks/upserted", task });
        return task;
      } catch (error) {
        dispatch({ type: "tasks/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  const exportTaskSummary = useCallback(
    async (
      projectPath: string,
      taskId: string,
      targetPath: string,
      format: "markdown" | "json",
    ) => {
      try {
        return await invoke<string>("export_task_summary", {
          input: { projectPath, taskId, targetPath, format },
        });
      } catch (error) {
        dispatch({ type: "tasks/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  const deleteTask = useCallback(
    async (projectPath: string | null, taskId: string) => {
      // In the browser preview there is no backend file to remove; just drop it
      // locally so the UI stays consistent.
      if (!projectPath || !hasTauriRuntime()) {
        dispatch({ type: "tasks/removed", taskId, projectPath: projectPath ?? undefined });
        return true;
      }

      try {
        await invoke("delete_task", { projectPath, taskId });
        dispatch({ type: "tasks/removed", taskId, projectPath });
        return true;
      } catch (error) {
        dispatch({ type: "tasks/loadFailed", error: toErrorMessage(error) });
        return false;
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

  const readPlanHtml = useCallback(async (projectPath: string, mdPath: string) => {
    if (!hasTauriRuntime()) {
      return null;
    }

    try {
      return await invoke<string>("read_plan_html", { projectPath, mdPath });
    } catch (error) {
      dispatch({ type: "tasks/loadFailed", error: toErrorMessage(error) });
      return null;
    }
  }, [dispatch]);

  const openPlanHtml = useCallback(async (projectPath: string, htmlPath: string) => {
    if (!hasTauriRuntime()) {
      return false;
    }

    try {
      await invoke("open_plan_html", { projectPath, htmlPath });
      return true;
    } catch (error) {
      dispatch({ type: "tasks/loadFailed", error: toErrorMessage(error) });
      return false;
    }
  }, [dispatch]);

  const openPlanViewer = useCallback(async (projectPath: string, mdPath: string) => {
    if (!hasTauriRuntime()) {
      return false;
    }

    try {
      await invoke("open_plan_viewer", { projectPath, mdPath });
      return true;
    } catch (error) {
      dispatch({ type: "tasks/loadFailed", error: toErrorMessage(error) });
      return false;
    }
  }, [dispatch]);

  const openPlanningEvidence = useCallback(async (projectPath: string, evidencePath: string) => {
    if (!hasTauriRuntime()) {
      return false;
    }

    try {
      await invoke("open_planning_evidence", { projectPath, evidencePath });
      return true;
    } catch (error) {
      dispatch({ type: "tasks/loadFailed", error: toErrorMessage(error) });
      return false;
    }
  }, [dispatch]);

  const updateTaskLifecycle = useCallback(
    async (
      command: "pause_task" | "resume_task" | "block_task" | "cancel_task",
      projectPath: string,
      taskId: string,
      reason?: string,
    ) => {
      try {
        const task = await invoke<Task>(command, {
          input: { projectPath, taskId, reason },
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

  const pauseTask = useCallback(
    (projectPath: string, taskId: string, reason?: string) =>
      updateTaskLifecycle("pause_task", projectPath, taskId, reason),
    [updateTaskLifecycle],
  );
  const resumeTask = useCallback(
    (projectPath: string, taskId: string, reason?: string) =>
      updateTaskLifecycle("resume_task", projectPath, taskId, reason),
    [updateTaskLifecycle],
  );
  const blockTask = useCallback(
    (projectPath: string, taskId: string, reason?: string) =>
      updateTaskLifecycle("block_task", projectPath, taskId, reason),
    [updateTaskLifecycle],
  );
  const cancelTask = useCallback(
    (projectPath: string, taskId: string, reason?: string) =>
      updateTaskLifecycle("cancel_task", projectPath, taskId, reason),
    [updateTaskLifecycle],
  );

  return {
    loadTasks,
    createTask,
    confirmPlan,
    startTodo,
    completeTodo,
    switchPrimaryAgent,
    buildImplementationContext,
    markReadyForTesting,
    completeTask,
    regenerateTaskSummary,
    exportTaskSummary,
    deleteTask,
    appendFeedback,
    recordPlanningDecision,
    generateRepairContext,
    readPlanHtml,
    openPlanHtml,
    openPlanViewer,
    openPlanningEvidence,
    pauseTask,
    resumeTask,
    blockTask,
    cancelTask,
  };
}
