import { useEffect, useMemo, useState, type FormEvent } from "react";
import { Bot, X } from "lucide-react";
import type { AgentConfig, ProjectSummary, Task } from "../../domain";
import { useAgentBridge } from "../../hooks/useAgentBridge";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useAppState } from "../../state/AppStateContext";
import { Button } from "../common/Button";
import "./Board.css";

interface NewTaskModalProps {
  project: ProjectSummary;
  onClose: () => void;
  onCreated?: (task: Task) => void;
}

function canImplement(agent: AgentConfig) {
  return agent.enabled && agent.available && agent.capabilities.includes("implementation");
}

function canPlan(agent: AgentConfig) {
  return agent.enabled && agent.available && agent.capabilities.includes("planning");
}

function suggestedPrimaryAgent(agents: AgentConfig[]) {
  const implementationAgents = agents.filter(canImplement);

  return (
    implementationAgents.find((agent) => agent.adapterType === "claude_code_cli") ??
    implementationAgents[0] ??
    agents.find((agent) => agent.enabled && agent.available) ??
    null
  );
}

export function NewTaskModal({ project, onClose, onCreated }: NewTaskModalProps) {
  const { state, dispatch } = useAppState();
  const { loadAgents } = useAgentBridge();
  const { createTask } = useTaskBridge();
  const suggested = useMemo(() => suggestedPrimaryAgent(state.agents), [state.agents]);
  const planningAgents = useMemo(() => state.agents.filter(canPlan), [state.agents]);
  const [title, setTitle] = useState("");
  const [rawRequirement, setRawRequirement] = useState("");
  const [primaryAgentId, setPrimaryAgentId] = useState("");
  const [selectedPlanningAgentIds, setSelectedPlanningAgentIds] = useState<string[]>([]);
  const [planningSelectionInitialized, setPlanningSelectionInitialized] = useState(false);

  useEffect(() => {
    void loadAgents();
  }, [loadAgents]);

  useEffect(() => {
    if (!primaryAgentId && suggested) {
      setPrimaryAgentId(suggested.id);
    }
  }, [primaryAgentId, suggested]);

  useEffect(() => {
    if (!planningSelectionInitialized && planningAgents.length > 0) {
      setSelectedPlanningAgentIds(planningAgents.slice(0, 3).map((agent) => agent.id));
      setPlanningSelectionInitialized(true);
    }
  }, [planningAgents, planningSelectionInitialized]);

  function togglePlanningAgent(agentId: string) {
    setSelectedPlanningAgentIds((current) =>
      current.includes(agentId)
        ? current.filter((id) => id !== agentId)
        : [...current, agentId],
    );
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    const trimmedTitle = title.trim();
    const trimmedRequirement = rawRequirement.trim();
    if (!trimmedTitle || !trimmedRequirement) {
      return;
    }

    const task = await createTask({
      projectPath: project.path,
      title: trimmedTitle,
      rawRequirement: trimmedRequirement,
      selectedPlanningAgentIds,
      primaryAgentId: primaryAgentId || undefined,
    });

    if (task) {
      onCreated?.(task);
      dispatch({ type: "app/viewSelected", view: "planning" });
      onClose();
    }
  }

  return (
    <div className="task-modal-backdrop" role="presentation">
      <div className="task-modal" role="dialog" aria-modal="true" aria-labelledby="new-task-title">
        <div className="task-modal-header">
          <div>
            <h2 id="new-task-title">New task</h2>
            <p>in {project.name}</p>
          </div>
          <button type="button" className="task-modal-close" aria-label="Close" onClick={onClose}>
            <X size={16} />
          </button>
        </div>

        <form className="task-modal-form" onSubmit={handleSubmit}>
          <label className="task-field">
            <span>Title</span>
            <input
              value={title}
              onChange={(event) => setTitle(event.target.value)}
              placeholder="Implement the dashboard filter"
              autoFocus
            />
          </label>

          <label className="task-field">
            <span>Description</span>
            <textarea
              value={rawRequirement}
              onChange={(event) => setRawRequirement(event.target.value)}
              placeholder="Describe the requirement, constraints, and expected outcome."
            />
          </label>

          <label className="task-field">
            <span>Invite agents to discuss</span>
            <div className="task-agent-chip-row">
              {planningAgents.map((agent) => {
                const selected = selectedPlanningAgentIds.includes(agent.id);

                return (
                  <button
                    type="button"
                    className={`task-agent-chip ${selected ? "selected" : ""}`}
                    key={agent.id}
                    onClick={() => togglePlanningAgent(agent.id)}
                  >
                    <span className={`task-agent-avatar agent-${agent.adapterType}`}>{agent.name.slice(0, 2)}</span>
                    <span>{agent.name}</span>
                    <span>{selected ? "✓" : "+"}</span>
                  </button>
                );
              })}
              {planningAgents.length === 0 && (
                <span className="task-agent-empty">No planning agents available</span>
              )}
            </div>
          </label>

          <label className="task-field">
            <span>Suggested primary</span>
            <div className="agent-select-row">
              <Bot size={15} />
              <select
                value={primaryAgentId}
                onChange={(event) => setPrimaryAgentId(event.target.value)}
              >
                <option value="">No primary Agent yet</option>
                {state.agents.filter(canImplement).map((agent) => (
                  <option key={agent.id} value={agent.id}>
                    {agent.name}
                  </option>
                ))}
              </select>
            </div>
          </label>

          {suggested && (
            <div className="suggested-primary-note">
              Suggested: {suggested.name}
              {suggested.adapterType === "claude_code_cli" ? " · strongest for Implement" : ""}
            </div>
          )}

          {state.app.taskError && <div className="task-modal-error">{state.app.taskError}</div>}

          <div className="task-modal-actions">
            <Button type="button" variant="ghost" onClick={onClose}>
              Cancel
            </Button>
            <Button
              type="submit"
              variant="primary"
              disabled={!title.trim() || !rawRequirement.trim() || state.app.isLoadingTasks}
            >
              Start discussion
            </Button>
          </div>
        </form>
      </div>
    </div>
  );
}
