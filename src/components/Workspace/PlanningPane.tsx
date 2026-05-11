import { useEffect, useMemo, useState, type FormEvent } from "react";
import { AtSign, Bot, CheckCircle2, FileText, Send } from "lucide-react";
import { useAgentBridge } from "../../hooks/useAgentBridge";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useAppState } from "../../state/AppStateContext";
import { Button } from "../common/Button";
import "./Workspace.css";

function taskTitleFromRequirement(requirement: string) {
  const firstLine = requirement.trim().split(/\r?\n/)[0] ?? "Planning task";
  return firstLine.slice(0, 48) || "Planning task";
}

function extractMentionNames(value: string) {
  return Array.from(value.matchAll(/@([\w-]+)/g), (match) => match[1].toLowerCase());
}

function mentionAliases(agent: { id: string; name: string; command: string; adapterType: string }) {
  const normalizedName = agent.name.toLowerCase().replace(/\s+/g, "-");
  const aliases = new Set([
    agent.id.toLowerCase(),
    agent.name.toLowerCase(),
    normalizedName,
    agent.command.toLowerCase(),
  ]);

  switch (agent.adapterType) {
    case "codex_cli":
      aliases.add("codex");
      aliases.add("codex-cli");
      break;
    case "claude_code_cli":
      aliases.add("claude");
      aliases.add("claude-code");
      aliases.add("claudecode");
      break;
    case "amp_cli":
      aliases.add("amp");
      aliases.add("amp-cli");
      break;
    case "dummy":
      aliases.add("dummy");
      aliases.add("mock");
      break;
    default:
      break;
  }

  return aliases;
}

function adapterLabel(adapterType: string) {
  switch (adapterType) {
    case "codex_cli":
      return "Codex CLI";
    case "claude_code_cli":
      return "Claude Code";
    case "amp_cli":
      return "Amp CLI";
    case "dummy":
      return "Mock/Test";
    default:
      return "CLI";
  }
}

function isRealPlanningAgent(agent: { adapterType: string; enabled: boolean; available: boolean }) {
  return (
    agent.enabled &&
    agent.available &&
    ["codex_cli", "claude_code_cli", "amp_cli"].includes(agent.adapterType)
  );
}

export function PlanningPane() {
  const { state } = useAppState();
  const { loadAgents, runPlanningDiscussion } = useAgentBridge();
  const { confirmPlan, createTask, loadTasks } = useTaskBridge();
  const project = state.projects.current;
  const task = state.tasks.find((candidate) => candidate.id === state.app.selectedTaskId) ?? null;
  const [requirement, setRequirement] = useState("");

  useEffect(() => {
    void loadAgents();
  }, [loadAgents]);

  useEffect(() => {
    if (project) {
      void loadTasks(project.path);
    }
  }, [loadTasks, project]);

  const mentionedAgentNames = useMemo(() => extractMentionNames(requirement), [requirement]);
  const hasExplicitAgentMentions = mentionedAgentNames.length > 0;
  const mentionedAgents = useMemo(
    () =>
      state.agents.filter((agent) => {
        const aliases = mentionAliases(agent);
        return agent.enabled && agent.available && mentionedAgentNames.some((name) => aliases.has(name));
      }),
    [mentionedAgentNames, state.agents],
  );
  const defaultPlanningAgents = useMemo(
    () => state.agents.filter((agent) => isRealPlanningAgent(agent)),
    [state.agents],
  );
  const selectedPlanningAgents = hasExplicitAgentMentions ? mentionedAgents : defaultPlanningAgents;

  const conversationEvents = task?.agentInvocations ?? [];

  async function handleDiscuss(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (!project || !requirement.trim()) {
      return;
    }

    const activeTask =
      task ??
      (await createTask({
        projectPath: project.path,
        title: taskTitleFromRequirement(requirement),
        rawRequirement: requirement.trim(),
      }));

    if (!activeTask) {
      return;
    }

    await runPlanningDiscussion({
      projectPath: project.path,
      taskId: activeTask.id,
      requirement: requirement.trim(),
      agentIds: selectedPlanningAgents.map((agent) => agent.id),
    });
  }

  async function handleConfirmPlan() {
    if (!project || !task?.finalPlan) {
      return;
    }

    await confirmPlan(project.path, task.id);
  }

  if (!project) {
    return (
      <div className="workspace-empty-state">
        <div className="workspace-empty-title">No project selected</div>
        <div className="workspace-empty-copy">
          Add or select a project from the left project list.
        </div>
      </div>
    );
  }

  return (
    <div className="planning-workspace">
      <div className="pane-header planning-header">
        <span className="pane-header-title">PLAN DISCUSSION</span>
        {task?.finalPlanPath && (
          <span className="plan-path">
            <FileText size={14} />
            {task.finalPlanPath}
          </span>
        )}
      </div>

      <div className="planning-stream">
        {(task || requirement.trim()) && (
          <div className="conversation-message user-message">
            <div className="message-author">You</div>
            <div>{task?.rawRequirement ?? requirement.replace(/\s*@[\w-]+/g, "")}</div>
          </div>
        )}

        {conversationEvents.map((invocation) => (
          <div className="conversation-message agent-message" key={invocation.id}>
            <div className="message-author">
              <Bot size={14} />
              {invocation.agentName} · {invocation.status}
            </div>
            <div>{invocation.outputSummary}</div>
            {invocation.evidenceRef && (
              <div className="evidence-ref">{invocation.evidenceRef}</div>
            )}
            {invocation.status === "failed" && invocation.stderrTail?.length > 0 && (
              <pre className="invocation-error">{invocation.stderrTail.join("\n")}</pre>
            )}
          </div>
        ))}

        {task?.discussionSummary && (
          <div className="conversation-message final-message">
            <div className="message-author">
              <CheckCircle2 size={14} />
              {task.status === "plan_review" ? "Final plan ready for review" : "Final plan confirmed"}
            </div>
            <div>{task.discussionSummary}</div>
            {task.finalPlan && <pre className="final-plan-preview">{task.finalPlan}</pre>}
            {task.planTodos.length > 0 && (
              <ol className="todo-preview-list">
                {task.planTodos.map((todo) => (
                  <li key={todo.id}>{todo.title}</li>
                ))}
              </ol>
            )}
            {task.status === "plan_review" && task.finalPlan && (
              <Button
                type="button"
                variant="primary"
                onClick={handleConfirmPlan}
                disabled={state.app.isLoadingTasks}
              >
                Confirm Plan
              </Button>
            )}
          </div>
        )}
      </div>

      <form className="planning-composer" onSubmit={handleDiscuss}>
        <textarea
          className="composer-input"
          value={requirement}
          onChange={(event) => setRequirement(event.target.value)}
          placeholder="Describe the requirement. By default Loom uses all available real planning Agents; mention @codex, @claude-code, or @amp to narrow it."
          disabled={state.app.isLoadingTasks}
        />
        <div className="composer-toolbar">
          <div className="agent-chip-row">
            {selectedPlanningAgents.map((agent) => (
              <span className={`agent-chip adapter-${agent.adapterType}`} key={agent.id}>
                <AtSign size={12} />
                {agent.name}
                <span className="agent-chip-meta">{adapterLabel(agent.adapterType)}</span>
              </span>
            ))}
            {selectedPlanningAgents.length === 0 && (
              <span className="agent-chip-hint">
                {hasExplicitAgentMentions
                  ? "No enabled available Agent matches the @mention; Loom will not fall back silently."
                  : "Enable Codex, Claude Code, or Amp in Settings, or mention a specific Agent."}
              </span>
            )}
            {!hasExplicitAgentMentions && selectedPlanningAgents.length > 0 && (
              <span className="agent-chip-hint">Defaulting to all available real planning Agents.</span>
            )}
          </div>
          <Button
            type="submit"
            variant="primary"
            iconRight={<Send size={14} />}
            disabled={
              !requirement.trim() ||
              selectedPlanningAgents.length === 0 ||
              state.app.isLoadingTasks
            }
          >
            Discuss
          </Button>
        </div>
        {state.app.taskError && <div className="inline-error">{state.app.taskError}</div>}
      </form>
    </div>
  );
}
