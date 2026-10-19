import { useEffect, useMemo, useState, type FormEvent } from "react";
import { Bot, X } from "lucide-react";
import {
  EMPTY_PROJECT_AGENT_PREFERENCES,
  type AgentConfig,
  type ProjectAgentPreferences,
  type ProjectSummary,
  type Task,
} from "../../domain";
import { useAgentCatalog } from "../../hooks/useAgentCatalog";
import { useProjectPreferencesBridge } from "../../hooks/useProjectPreferencesBridge";
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
  const { loadAgents } = useAgentCatalog();
  const { loadProjectAgentPreferences } = useProjectPreferencesBridge();
  const { createTask } = useTaskBridge();
  const suggested = useMemo(() => suggestedPrimaryAgent(state.agents), [state.agents]);
  const planningAgents = useMemo(() => state.agents.filter(canPlan), [state.agents]);
  const [title, setTitle] = useState("");
  const [rawRequirement, setRawRequirement] = useState("");
  const [primaryAgentId, setPrimaryAgentId] = useState("");
  const [selectedPlanningAgentIds, setSelectedPlanningAgentIds] = useState<string[]>([]);
  const [planningSelectionInitialized, setPlanningSelectionInitialized] = useState(false);
  const [preferences, setPreferences] = useState<ProjectAgentPreferences>({
    ...EMPTY_PROJECT_AGENT_PREFERENCES,
  });
  const [preferencesLoaded, setPreferencesLoaded] = useState(false);

  useEffect(() => {
    void loadAgents();
  }, [loadAgents]);

  useEffect(() => {
    let cancelled = false;
    setPreferencesLoaded(false);
    void loadProjectAgentPreferences(project.path)
      .then((loaded) => {
        if (!cancelled) {
          setPreferences(loaded);
          setPreferencesLoaded(true);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setPreferencesLoaded(true);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [project.path, loadProjectAgentPreferences]);

  useEffect(() => {
    if (!preferencesLoaded || primaryAgentId) {
      return;
    }
    const preferred = state.agents.find(
      (agent) => agent.id === preferences.implementationAgentId && canImplement(agent),
    );
    if (preferred ?? suggested) {
      setPrimaryAgentId((preferred ?? suggested)?.id ?? "");
    }
  }, [preferencesLoaded, preferences.implementationAgentId, primaryAgentId, state.agents, suggested]);

  useEffect(() => {
    if (preferencesLoaded && !planningSelectionInitialized && planningAgents.length > 0) {
      const availableIds = new Set(planningAgents.map((agent) => agent.id));
      const preferred = preferences.planningAgentIds.filter((agentId) => availableIds.has(agentId));
      setSelectedPlanningAgentIds(
        preferred.length > 0 ? preferred : planningAgents.slice(0, 3).map((agent) => agent.id),
      );
      setPlanningSelectionInitialized(true);
    }
  }, [preferences.planningAgentIds, preferencesLoaded, planningAgents, planningSelectionInitialized]);

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
            <h2 id="new-task-title">新建任务</h2>
            <p>项目：{project.name}</p>
          </div>
          <button type="button" className="task-modal-close" aria-label="关闭" onClick={onClose}>
            <X size={16} />
          </button>
        </div>

        <form className="task-modal-form" onSubmit={handleSubmit}>
          <label className="task-field">
            <span>标题</span>
            <input
              value={title}
              onChange={(event) => setTitle(event.target.value)}
              placeholder="例如：实现仪表盘筛选功能"
              autoFocus
            />
          </label>

          <label className="task-field">
            <span>需求描述</span>
            <textarea
              value={rawRequirement}
              onChange={(event) => setRawRequirement(event.target.value)}
              placeholder="描述需求、约束与预期结果。"
            />
          </label>

          <label className="task-field">
            <span>邀请 Agent 参与讨论</span>
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
                <span className="task-agent-empty">没有可用的规划 Agent</span>
              )}
            </div>
          </label>

          <label className="task-field">
            <span>建议主 Agent</span>
            <div className="agent-select-row">
              <Bot size={15} />
              <select
                value={primaryAgentId}
                onChange={(event) => setPrimaryAgentId(event.target.value)}
              >
                <option value="">暂不指定主 Agent</option>
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
              建议：{suggested.name}
              {suggested.adapterType === "claude_code_cli" ? " · 更适合实施" : ""}
            </div>
          )}

          {state.app.taskError && <div className="task-modal-error">{state.app.taskError}</div>}

          <div className="task-modal-actions">
            <Button type="button" variant="ghost" onClick={onClose}>
              取消
            </Button>
            <Button
              type="submit"
              variant="primary"
              disabled={!title.trim() || !rawRequirement.trim() || state.app.isLoadingTasks}
            >
              开始讨论
            </Button>
          </div>
        </form>
      </div>
    </div>
  );
}
