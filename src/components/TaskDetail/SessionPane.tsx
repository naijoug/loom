import { useEffect, useMemo, useState, type FormEvent } from "react";
import {
  CheckCircle2,
  Circle,
  FileText,
  Play,
  PlayCircle,
  Send,
  Terminal,
} from "lucide-react";
import type { AgentConfig, PlanTodoItem, PlanTodoStatus, ProjectSummary, Task } from "../../domain";
import { useAgentBridge } from "../../hooks/useAgentBridge";
import { useCommandBridge } from "../../hooks/useCommandBridge";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useAppState } from "../../state/AppStateContext";
import { Button } from "../common/Button";
import "./TaskDetail.css";

interface SessionPaneProps {
  project: ProjectSummary;
  task: Task;
}

const TODO_STATUS_LABELS: Record<PlanTodoStatus, string> = {
  pending: "Pending",
  implementing: "Implementing",
  done: "Done",
  blocked: "Blocked",
};

function hasImplementationCapability(agent: AgentConfig) {
  return (
    agent.enabled &&
    agent.available &&
    agent.adapterType !== "dummy" &&
    agent.capabilities.includes("implementation")
  );
}

function replaceRuntimePlaceholders(args: string[], projectPath: string, prompt: string) {
  return args.map((arg) =>
    arg.split("{projectPath}").join(projectPath).split("{prompt}").join(prompt),
  );
}

function buildImplementationPrompt(task: Task, todo: PlanTodoItem, todoIndex: number) {
  return [
    "# Loom Implementation Handoff",
    "",
    "You are implementing one selected todo from a confirmed Loom plan.",
    "",
    `Project: ${task.projectPath}`,
    `Task: ${task.title}`,
    `Todo ${todoIndex + 1}: ${todo.title}`,
    "",
    "Todo description:",
    todo.description,
    "",
    "Execution rules:",
    "- Modify only the files needed for this todo.",
    "- Preserve unrelated user changes.",
    "- Run the smallest relevant verification before reporting completion.",
    "- Report changed files, verification evidence, blockers, and remaining risk.",
    "",
    "Confirmed final plan:",
    task.finalPlan ?? "(none)",
  ].join("\n");
}

function buildAgentCommandArgs(agent: AgentConfig, projectPath: string, prompt: string) {
  if (agent.args.length > 0) {
    const args = replaceRuntimePlaceholders(agent.args, projectPath, prompt);
    return agent.args.some((arg) => arg.includes("{prompt}")) ? args : [...args, prompt];
  }

  switch (agent.adapterType) {
    case "codex_cli":
      return [
        "exec",
        "--cd",
        projectPath,
        "--sandbox",
        agent.canWriteFiles ? "workspace-write" : "read-only",
        prompt,
      ];
    case "claude_code_cli":
      return [
        "-p",
        prompt,
        "--permission-mode",
        agent.canWriteFiles ? "acceptEdits" : "plan",
        "--output-format",
        "text",
      ];
    default:
      return [prompt];
  }
}

function agentInitials(agent?: AgentConfig | null) {
  if (!agent) {
    return "Ag";
  }

  return agent.name
    .split(/\s+/)
    .map((part) => part[0])
    .join("")
    .slice(0, 2);
}

function summarizeCommand(command: string) {
  return command.length > 140 ? `${command.slice(0, 140)}...` : command;
}

export function SessionPane({ project, task }: SessionPaneProps) {
  const { state } = useAppState();
  const { loadAgents } = useAgentBridge();
  const { startCommandRun } = useCommandBridge();
  const { appendFeedback, completeTodo, markReadyForTesting, startTodo } = useTaskBridge();
  const implementationAgents = useMemo(
    () => state.agents.filter((agent) => hasImplementationCapability(agent)),
    [state.agents],
  );
  const [selectedAgentId, setSelectedAgentId] = useState(task.primaryAgentId ?? "");
  const [guidance, setGuidance] = useState("");
  const selectedAgent =
    implementationAgents.find((agent) => agent.id === selectedAgentId) ??
    implementationAgents.find((agent) => agent.id === task.primaryAgentId) ??
    implementationAgents[0] ??
    null;
  const activeTodo =
    task.planTodos.find((todo) => todo.id === state.app.selectedTodoId) ??
    task.planTodos.find((todo) => todo.status === "implementing") ??
    task.planTodos[0] ??
    null;
  const commandRunning = state.commandRuns.some((run) => run.taskId === task.id && run.status === "running");
  const latestRun =
    state.commandRuns
      .filter((run) => run.taskId === task.id)
      .sort((left, right) => right.startedAtMs - left.startedAtMs)[0] ?? null;
  const latestRunLogs = latestRun ? state.commandLogs[latestRun.id] ?? [] : [];
  const allTodosDone =
    task.planTodos.length > 0 && task.planTodos.every((todo) => todo.status === "done");
  const canMarkReadyForTesting = task.status === "reviewing" && allTodosDone;

  useEffect(() => {
    void loadAgents();
  }, [loadAgents]);

  useEffect(() => {
    if (!selectedAgentId && selectedAgent) {
      setSelectedAgentId(selectedAgent.id);
    }
  }, [selectedAgent, selectedAgentId]);

  async function handleStartTodo(todo: PlanTodoItem, todoIndex: number) {
    if (!selectedAgent) {
      return;
    }

    const updatedTask = await startTodo(project.path, task.id, todo.id, selectedAgent.id);
    const prompt = buildImplementationPrompt(updatedTask ?? task, todo, todoIndex);
    await startCommandRun({
      program: selectedAgent.command,
      args: buildAgentCommandArgs(selectedAgent, project.path, prompt),
      cwd: project.path,
      taskId: task.id,
    });
  }

  async function handleCompleteTodo(todo: PlanTodoItem) {
    await completeTodo(project.path, task.id, todo.id);
  }

  async function handleGuidanceSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (!guidance.trim()) {
      return;
    }

    const updated = await appendFeedback(project.path, task.id, latestRun?.id, guidance.trim());
    if (updated) {
      setGuidance("");
    }
  }

  async function handleMarkReadyForTesting() {
    await markReadyForTesting(project.path, task.id);
  }

  return (
    <div className="session-pane">
      <div className="session-header">
        <div className="testing-stepper">
          <span className="testing-step step-done">Plan</span>
          <span className="testing-step step-active">Implement</span>
          <span className="testing-step">Test</span>
          <span className="testing-step">Done</span>
        </div>
        <span className="testing-status-pill testing-status-ok">
          <span />
          {selectedAgent?.name ?? "No implementation Agent"}
        </span>
      </div>

      <div className="session-grid">
        <section className="session-main-panel">
          <div className="session-thread">
            <article className="session-turn">
              <div className="session-avatar user">ME</div>
              <div className="session-turn-body">
                <div className="session-name">Task</div>
                <p>{task.title}</p>
                {activeTodo && <p>{activeTodo.description}</p>}
              </div>
            </article>

            <article className="session-turn">
              <div className="session-avatar agent">{agentInitials(selectedAgent)}</div>
              <div className="session-turn-body">
                <div className="session-name">
                  {selectedAgent?.name ?? "Implementation Agent"}
                  <span>agent loop</span>
                </div>
                <div className="session-thinking">
                  Reading the confirmed plan and preparing a scoped implementation run for the selected todo.
                </div>

                <div className="session-tool">
                  <div className="session-tool-header">
                    <FileText size={14} />
                    <b>Read</b>
                    <span>{activeTodo?.planRef ?? task.finalPlanPath ?? "confirmed plan"}</span>
                    <em>scope</em>
                  </div>
                  <pre>{task.finalPlan ?? "Confirm a plan before implementation."}</pre>
                </div>

                {latestRun && (
                  <div className="session-tool">
                    <div className="session-tool-header">
                      <Terminal size={14} />
                      <b>Run</b>
                      <span>{summarizeCommand(latestRun.command)}</span>
                      <em>{latestRun.status}</em>
                    </div>
                    <pre>
                      {latestRunLogs.length > 0
                        ? latestRunLogs.map((entry) => `${entry.stream.toUpperCase()} ${entry.line}`).join("\n")
                        : "Agent process started. Waiting for output..."}
                    </pre>
                  </div>
                )}

                <p className="session-agent-copy">
                  {activeTodo
                    ? `Ready to implement "${activeTodo.title}". Start the todo, capture command evidence, then mark it done after review.`
                    : "Confirm a plan to generate implementation todos."}
                </p>
              </div>
            </article>
          </div>

          <form className="session-composer" onSubmit={handleGuidanceSubmit}>
            <input
              value={guidance}
              onChange={(event) => setGuidance(event.target.value)}
              placeholder={`Steer ${selectedAgent?.name ?? "the Agent"} - add a constraint, answer, or approve next step...`}
              disabled={!selectedAgent}
            />
            <Button type="submit" variant="primary" iconRight={<Send size={14} />} disabled={!guidance.trim()}>
              Send
            </Button>
          </form>
        </section>

        <aside className="session-task-panel">
          <div className="session-task-header">
            <span>Task</span>
            <span className="testing-status-pill testing-status-ok">
              <span />
              In Progress
            </span>
          </div>
          <div className="session-task-body">
            <div>
              <div className="debug-card-label">Assignee</div>
              <label className="session-assignee">
                <div className="session-avatar agent">{agentInitials(selectedAgent)}</div>
                <select
                  value={selectedAgent?.id ?? ""}
                  disabled={implementationAgents.length === 0 || commandRunning}
                  onChange={(event) => setSelectedAgentId(event.target.value)}
                >
                  {implementationAgents.map((agent) => (
                    <option value={agent.id} key={agent.id}>
                      {agent.name}
                    </option>
                  ))}
                </select>
              </label>
            </div>

            <div>
              <div className="debug-card-label">Subtasks</div>
              <div className="session-subtasks">
                {task.planTodos.length === 0 && <span>No todos yet.</span>}
                {task.planTodos.map((todo, todoIndex) => {
                  const active = todo.id === activeTodo?.id;
                  const done = todo.status === "done";

                  return (
                    <div className={`session-subtask ${active ? "active" : ""}`} key={todo.id}>
                      <button
                        type="button"
                        className="session-subtask-main"
                        disabled={done || !selectedAgent || commandRunning}
                        onClick={() => void handleStartTodo(todo, todoIndex)}
                      >
                        {todo.status === "implementing" ? <PlayCircle size={15} /> : <Play size={15} />}
                        <span className={done ? "done" : ""}>{todo.title}</span>
                        <em>{TODO_STATUS_LABELS[todo.status]}</em>
                      </button>
                      <button
                        type="button"
                        className="session-subtask-check"
                        disabled={done}
                        onClick={() => void handleCompleteTodo(todo)}
                      >
                        {done ? <CheckCircle2 size={14} /> : <Circle size={10} />}
                      </button>
                    </div>
                  );
                })}
              </div>
            </div>

            <div>
              <div className="debug-card-label">Files changed</div>
              <div className="session-file-list">
                {task.agentInvocations.slice(-3).map((invocation) => (
                  <div key={invocation.id}>
                    <span>{invocation.evidenceRef ?? invocation.agentName}</span>
                    <b>{invocation.status}</b>
                  </div>
                ))}
                {task.agentInvocations.length === 0 && <span>No file evidence recorded yet.</span>}
              </div>
            </div>

            <Button
              type="button"
              variant="primary"
              disabled={!canMarkReadyForTesting}
              onClick={handleMarkReadyForTesting}
            >
              Mark ready for testing
            </Button>
          </div>
        </aside>
      </div>
    </div>
  );
}
