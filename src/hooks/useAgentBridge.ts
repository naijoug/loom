import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect } from "react";
import type {
  AgentConfig,
  AgentConfigInput,
  PlanningAgentLogEvent,
  PlanningAgentStatusEvent,
  PlanningDiscussionInput,
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
    void listen<PlanningAgentStatusEvent>("loom://planning-agent-status", (event) => {
      dispatch({ type: "planning/progressUpdated", event: event.payload });
    }).then((cleanup) => {
      if (cancelled) {
        cleanup();
      } else {
        unlistenStatus = cleanup;
      }
    });
    void listen<PlanningAgentLogEvent>("loom://planning-agent-log", (event) => {
      dispatch({ type: "planning/logReceived", event: event.payload });
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
      dispatch({ type: "agents/loaded", agents: await invoke<AgentConfig[]>("list_agents") });
    } catch (error) {
      dispatch({ type: "agents/loadFailed", error: toErrorMessage(error) });
    }
  }, [dispatch]);

  const createAgent = useCallback(
    async (input: AgentConfigInput) => {
      dispatch({ type: "agents/loadStarted" });
      try {
        assertTauriRuntime("Creating an Agent");

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

  const setAgentEnabled = useCallback(
    async (agentId: string, enabled: boolean) => {
      dispatch({ type: "agents/loadStarted" });
      try {
        assertTauriRuntime("Updating an Agent");

        const agents = await invoke<AgentConfig[]>("set_agent_enabled", { agentId, enabled });
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

        const agents = await invoke<AgentConfig[]>("update_agent", { agentId, input });
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

        const agents = await invoke<AgentConfig[]>("delete_agent", { agentId });
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

        const task = await invoke<Task>("run_planning_discussion", { input });
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

        const task = await invoke<Task>("run_plan_reviews", { projectPath, taskId });
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

        const task = await invoke<Task>("retry_planning_agent", {
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

  return {
    loadAgents,
    createAgent,
    setAgentEnabled,
    updateAgent,
    deleteAgent,
    runPlanningDiscussion,
    runPlanReviews,
    retryPlanningAgent,
  };
}
