import { useCallback } from "react";
import { invokeCommand, TAURI_COMMANDS, type TauriCommand } from "../api";
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
        const tasks = await invokeCommand<Task[]>(TAURI_COMMANDS.listTasks, { projectPath });
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

        const task = await invokeCommand<Task>(TAURI_COMMANDS.createTask, { input });
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
        const task = await invokeCommand<Task>(TAURI_COMMANDS.appendFeedback, {
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
        const task = await invokeCommand<Task>(TAURI_COMMANDS.recordPlanningDecision, { input });
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
        const task = await invokeCommand<Task>(TAURI_COMMANDS.confirmPlan, { projectPath, taskId });
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
        const task = await invokeCommand<Task>(TAURI_COMMANDS.startTodo, {
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
        const task = await invokeCommand<Task>(TAURI_COMMANDS.completeTodo, {
          projectPath,
          taskId,
          todoId,
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

  const switchPrimaryAgent = useCallback(
    async (projectPath: string, taskId: string, agentId: string, reason: string) => {
      try {
        const task = await invokeCommand<Task>(TAURI_COMMANDS.switchPrimaryAgent, {
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
        return await invokeCommand<ContextBuildOutput>(TAURI_COMMANDS.buildImplementationContext, {
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
        const task = await invokeCommand<Task>(TAURI_COMMANDS.markReadyForTesting, {
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

  const completeTask = useCallback(
    async (projectPath: string | null, taskId: string) => {
      if (!projectPath || !hasTauriRuntime()) {
        return null;
      }

      try {
        const task = await invokeCommand<Task>(TAURI_COMMANDS.completeTask, { projectPath, taskId });
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
        const task = await invokeCommand<Task>(TAURI_COMMANDS.regenerateTaskSummary, {
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

  const exportTaskSummary = useCallback(
    async (
      projectPath: string,
      taskId: string,
      targetPath: string,
      format: "markdown" | "json",
    ) => {
      try {
        return await invokeCommand<string>(TAURI_COMMANDS.exportTaskSummary, {
          input: { projectPath, taskId, targetPath, format },
        });
      } catch (error) {
        dispatch({ type: "tasks/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  const exportDiagnosticBundle = useCallback(
    async (
      projectPath: string,
      taskId: string | undefined,
      targetPath: string,
      includeLogTails: boolean,
    ) => {
      try {
        return await invokeCommand<string>(TAURI_COMMANDS.exportDiagnosticBundle, {
          input: { projectPath, taskId, targetPath, includeLogTails },
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
        await invokeCommand<void>(TAURI_COMMANDS.deleteTask, { projectPath, taskId });
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
        const task = await invokeCommand<Task>(TAURI_COMMANDS.generateRepairContext, {
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

  const readPlanHtml = useCallback(async (projectPath: string, mdPath: string) => {
    if (!hasTauriRuntime()) {
      return null;
    }

    try {
      return await invokeCommand<string>(TAURI_COMMANDS.readPlanHtml, { projectPath, mdPath });
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
      await invokeCommand<void>(TAURI_COMMANDS.openPlanHtml, { projectPath, htmlPath });
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
      await invokeCommand<void>(TAURI_COMMANDS.openPlanViewer, { projectPath, mdPath });
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
      await invokeCommand<void>(TAURI_COMMANDS.openPlanningEvidence, {
        projectPath,
        evidencePath,
      });
      return true;
    } catch (error) {
      dispatch({ type: "tasks/loadFailed", error: toErrorMessage(error) });
      return false;
    }
  }, [dispatch]);

  const updateTaskLifecycle = useCallback(
    async (
      command: TauriCommand,
      projectPath: string,
      taskId: string,
      reason?: string,
    ) => {
      try {
        const task = await invokeCommand<Task>(command, {
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
      updateTaskLifecycle(TAURI_COMMANDS.pauseTask, projectPath, taskId, reason),
    [updateTaskLifecycle],
  );
  const resumeTask = useCallback(
    (projectPath: string, taskId: string, reason?: string) =>
      updateTaskLifecycle(TAURI_COMMANDS.resumeTask, projectPath, taskId, reason),
    [updateTaskLifecycle],
  );
  const blockTask = useCallback(
    (projectPath: string, taskId: string, reason?: string) =>
      updateTaskLifecycle(TAURI_COMMANDS.blockTask, projectPath, taskId, reason),
    [updateTaskLifecycle],
  );
  const cancelTask = useCallback(
    (projectPath: string, taskId: string, reason?: string) =>
      updateTaskLifecycle(TAURI_COMMANDS.cancelTask, projectPath, taskId, reason),
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
    exportDiagnosticBundle,
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
