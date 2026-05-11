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

export function PlanningPane() {
  const { state } = useAppState();
  const { loadAgents, runPlanningDiscussion } = useAgentBridge();
  const { createTask, loadTasks } = useTaskBridge();
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

  const mentionedAgents = useMemo(() => {
    const names = extractMentionNames(requirement);
    return state.agents.filter((agent) => {
      const aliases = mentionAliases(agent);
      return agent.enabled && agent.available && names.some((name) => aliases.has(name));
    });
  }, [requirement, state.agents]);

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
      agentIds: mentionedAgents.map((agent) => agent.id),
    });
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
              Final plan ready
            </div>
            <div>{task.discussionSummary}</div>
            {task.planTodos.length > 0 && (
              <ol className="todo-preview-list">
                {task.planTodos.map((todo) => (
                  <li key={todo.id}>{todo.title}</li>
                ))}
              </ol>
            )}
          </div>
        )}
      </div>

      <form className="planning-composer" onSubmit={handleDiscuss}>
        <textarea
          className="composer-input"
          value={requirement}
          onChange={(event) => setRequirement(event.target.value)}
          placeholder="Describe the requirement. Mention agents with @codex, @claude-code, @claude, @amp."
          disabled={state.app.isLoadingTasks}
        />
        <div className="composer-toolbar">
          <div className="agent-chip-row">
            {mentionedAgents.map((agent) => (
              <span className={`agent-chip adapter-${agent.adapterType}`} key={agent.id}>
                <AtSign size={12} />
                {agent.name}
                <span className="agent-chip-meta">{adapterLabel(agent.adapterType)}</span>
              </span>
            ))}
            {mentionedAgents.length === 0 && (
              <span className="agent-chip-hint">
                Mention an available real Agent: @codex, @claude-code, @claude, or @amp.
              </span>
            )}
          </div>
          <Button
            type="submit"
            variant="primary"
            iconRight={<Send size={14} />}
            disabled={!requirement.trim() || mentionedAgents.length === 0 || state.app.isLoadingTasks}
          >
            Discuss
          </Button>
        </div>
        {state.app.taskError && <div className="inline-error">{state.app.taskError}</div>}
      </form>
    </div>
  );
}
