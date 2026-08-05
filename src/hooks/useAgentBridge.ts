import { useCallback, useEffect } from "react";
import { invokeCommand, listenToEvent, TAURI_COMMANDS, TAURI_EVENTS } from "../api";
import type {
  AgentConfig,
  AgentDiagnostic,
  AgentConfigInput,
  PlanningAgentLogEvent,
  PlanningAgentStatusEvent,
  PlanningDiscussionInput,
  PrepareAgentInvocationInput,
  PreparedAgentInvocation,
  Task,
} from "../domain";
import { useAppState } from "../state/AppStateContext";
import { hasTauriRuntime } from "./runtime";

function toErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function assertTauriRuntime(action: string) {
  if (!hasTauriRuntime()) {
    throw new Error(`${action} requires the Tauri desktop runtime.`);
  }
}

export function useAgentBridge() {
  const { state, dispatch } = useAppState();

  useEffect(() => {
    if (!hasTauriRuntime()) {
      return;
    }

    let unlistenStatus: (() => void) | null = null;
    let unlistenLog: (() => void) | null = null;
    let cancelled = false;
    void listenToEvent<PlanningAgentStatusEvent>(TAURI_EVENTS.planningAgentStatus, (event) => {
      dispatch({ type: "planning/progressUpdated", event });
    }).then((cleanup) => {
      if (cancelled) {
        cleanup();
      } else {
        unlistenStatus = cleanup;
      }
    });
    void listenToEvent<PlanningAgentLogEvent>(TAURI_EVENTS.planningAgentLog, (event) => {
      dispatch({ type: "planning/logReceived", event });
    }).then((cleanup) => {
      if (cancelled) {
        cleanup();
      } else {
        unlistenLog = cleanup;
      }
    });

    return () => {
      cancelled = true;
      unlistenStatus?.();
      unlistenLog?.();
    };
  }, [dispatch]);

  const loadAgents = useCallback(async () => {
    if (!hasTauriRuntime()) {
      return;
    }

    dispatch({ type: "agents/loadStarted" });
    try {
      dispatch({
        type: "agents/loaded",
        agents: await invokeCommand<AgentConfig[]>(TAURI_COMMANDS.listAgents),
      });
    } catch (error) {
      dispatch({ type: "agents/loadFailed", error: toErrorMessage(error) });
    }
  }, [dispatch]);

  const createAgent = useCallback(
    async (input: AgentConfigInput) => {
      dispatch({ type: "agents/loadStarted" });
      try {
        assertTauriRuntime("Creating an Agent");

        const agent = await invokeCommand<AgentConfig>(TAURI_COMMANDS.createAgent, { input });
        dispatch({ type: "agents/created", agent });
        return agent;
      } catch (error) {
        dispatch({ type: "agents/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  const setAgentEnabled = useCallback(
    async (agentId: string, enabled: boolean) => {
      dispatch({ type: "agents/loadStarted" });
      try {
        assertTauriRuntime("Updating an Agent");

        const agents = await invokeCommand<AgentConfig[]>(TAURI_COMMANDS.setAgentEnabled, {
          agentId,
          enabled,
        });
        dispatch({ type: "agents/loaded", agents });
        return agents;
      } catch (error) {
        dispatch({ type: "agents/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  const updateAgent = useCallback(
    async (agentId: string, input: AgentConfigInput) => {
      dispatch({ type: "agents/loadStarted" });
      try {
        assertTauriRuntime("Updating an Agent");

        const agents = await invokeCommand<AgentConfig[]>(TAURI_COMMANDS.updateAgent, {
          agentId,
          input,
        });
        dispatch({ type: "agents/loaded", agents });
        return agents;
      } catch (error) {
        dispatch({ type: "agents/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  const deleteAgent = useCallback(
    async (agentId: string) => {
      dispatch({ type: "agents/loadStarted" });
      try {
        assertTauriRuntime("Deleting an Agent");

        const agents = await invokeCommand<AgentConfig[]>(TAURI_COMMANDS.deleteAgent, { agentId });
        dispatch({ type: "agents/loaded", agents });
        return agents;
      } catch (error) {
        dispatch({ type: "agents/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

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

  const prepareAgentInvocation = useCallback(
    async (input: PrepareAgentInvocationInput) => {
      try {
        assertTauriRuntime("Preparing an Agent invocation");
        return await invokeCommand<PreparedAgentInvocation>(
          TAURI_COMMANDS.prepareAgentInvocation,
          { input },
        );
      } catch (error) {
        dispatch({ type: "agents/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  const diagnoseAgents = useCallback(async () => {
    if (!hasTauriRuntime()) {
      return [] as AgentDiagnostic[];
    }
    return invokeCommand<AgentDiagnostic[]>(TAURI_COMMANDS.diagnoseAgents);
  }, []);

  return {
    loadAgents,
    createAgent,
    setAgentEnabled,
    updateAgent,
    deleteAgent,
    runPlanningDiscussion,
    runPlanReviews,
    retryPlanningAgent,
    prepareAgentInvocation,
    diagnoseAgents,
  };
}
