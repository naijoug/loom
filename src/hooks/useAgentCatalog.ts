import { useCallback } from "react";
import { agentsApi, providersApi } from "../api/agents";
import type { AgentDiagnostic, AgentConfigInput, ModelProviderInput } from "../domain";
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
    // A broken providers file must not hide the Agents; Settings shows its error on its own page.
    providersApi
      .list()
      .then((providers) => dispatch({ type: "providers/loaded", providers }))
      .catch(() => undefined);
    try {
      dispatch({
        type: "agents/loaded",
        agents: await agentsApi.list(),
      });
    } catch (error) {
      dispatch({ type: "agents/loadFailed", error: toErrorMessage(error) });
    }
  }, [dispatch]);

  // Provider calls throw, so the Providers page can show the error (or a revision conflict) next to the form.
  const loadProviders = useCallback(async () => {
    assertTauriRuntime("Loading model providers");
    const providers = await providersApi.list();
    dispatch({ type: "providers/loaded", providers });
    return providers;
  }, [dispatch]);

  const saveProvider = useCallback(
    async (input: ModelProviderInput) => {
      assertTauriRuntime("Saving a model provider");
      const providers = await providersApi.save(input);
      dispatch({ type: "providers/loaded", providers });
      return providers;
    },
    [dispatch],
  );

  const deleteProvider = useCallback(
    async (providerId: string, expectedRevision: number) => {
      assertTauriRuntime("Removing a model provider");
      const providers = await providersApi.remove(providerId, expectedRevision);
      dispatch({ type: "providers/loaded", providers });
      return providers;
    },
    [dispatch],
  );

  const testProvider = useCallback(async (providerId: string) => {
    assertTauriRuntime("Testing a model provider");
    return providersApi.test(providerId);
  }, []);

  const createAgent = useCallback(
    async (input: AgentConfigInput) => {
      dispatch({ type: "agents/loadStarted" });
      try {
        assertTauriRuntime("Creating an Agent");

        const agent = await agentsApi.create(input);
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

        const agents = await agentsApi.setEnabled(agentId, enabled);
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

        const agents = await agentsApi.update(agentId, input);
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

        const agents = await agentsApi.remove(agentId);
        dispatch({ type: "agents/loaded", agents });
        return agents;
      } catch (error) {
        dispatch({ type: "agents/loadFailed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  const diagnoseAgents = useCallback(
    async (agentId?: string) => {
      if (!hasTauriRuntime()) {
        return [] as AgentDiagnostic[];
      }
      const diagnostics = await agentsApi.diagnose(agentId);
      dispatch({ type: "agents/diagnosed", diagnostics });
      return diagnostics;
    },
    [dispatch],
  );

  return {
    loadAgents,
    createAgent,
    setAgentEnabled,
    updateAgent,
    deleteAgent,
    diagnoseAgents,
    loadProviders,
    saveProvider,
    deleteProvider,
    testProvider,
  };
}
