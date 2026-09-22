import { useCallback } from "react";
import { invokeCommand, TAURI_COMMANDS } from "../api";
import type { AgentConfig, AgentDiagnostic, AgentConfigInput, PrepareAgentInvocationInput, PreparedAgentInvocation } from "../domain";
import { useAppState } from "../state/AppStateContext";
import { hasTauriRuntime } from "./runtime";

function toErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
function assertTauriRuntime(action: string) {
  if (!hasTauriRuntime()) throw new Error(`${action} requires the Tauri desktop runtime.`);
}

/** Agent configuration and diagnostics have no workflow subscriptions. */
export function useAgentCatalog() {
  const { dispatch } = useAppState();
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

  return { loadAgents, createAgent, setAgentEnabled, updateAgent, deleteAgent, prepareAgentInvocation, diagnoseAgents };
}
