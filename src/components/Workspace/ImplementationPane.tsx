import { Bot, CheckCircle2, Circle, Play, PlayCircle } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { AgentConfig, PlanTodoItem, PlanTodoStatus, Task } from "../../domain";
import { useAgentBridge } from "../../hooks/useAgentBridge";
import { useCommandBridge } from "../../hooks/useCommandBridge";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useAppState } from "../../state/AppStateContext";
import { Button } from "../common/Button";
import "./Workspace.css";

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
    case "amp_cli":
      return ["-x", prompt];
    default:
      return [prompt];
  }
}

export function ImplementationPane() {
  const { state } = useAppState();
  const { startTodo, completeTodo, markReadyForTesting } = useTaskBridge();
  const { loadAgents } = useAgentBridge();
  const { startCommandRun } = useCommandBridge();
  const task = state.tasks.find((candidate) => candidate.id === state.app.selectedTaskId) ?? null;
  const projectPath = state.projects.current?.path ?? null;
  const taskId = task?.id ?? null;
  const todos = task?.planTodos ?? [];
  const implementationAgents = useMemo(
    () => state.agents.filter((agent) => hasImplementationCapability(agent)),
    [state.agents],
  );
  const [selectedAgentId, setSelectedAgentId] = useState("");
  const selectedAgent =
    implementationAgents.find((agent) => agent.id === selectedAgentId) ?? implementationAgents[0] ?? null;
  const commandRunning = state.commandRuns.some((run) => run.status === "running");
  const allTodosDone = todos.length > 0 && todos.every((todo) => todo.status === "done");
  const canMarkReadyForTesting = task?.status === "reviewing" && allTodosDone;

  useEffect(() => {
    void loadAgents();
  }, [loadAgents]);

  useEffect(() => {
    if (!selectedAgentId && implementationAgents[0]) {
      setSelectedAgentId(implementationAgents[0].id);
    }
  }, [implementationAgents, selectedAgentId]);

  async function handleStartWithAgent(todo: PlanTodoItem, todoIndex: number) {
    if (!projectPath || !task || !taskId || !selectedAgent) {
      return;
    }

    const updatedTask = await startTodo(projectPath, taskId, todo.id, selectedAgent.id);
    const prompt = buildImplementationPrompt(updatedTask ?? task, todo, todoIndex);
    await startCommandRun({
      program: selectedAgent.command,
      args: buildAgentCommandArgs(selectedAgent, projectPath, prompt),
      cwd: projectPath,
      taskId,
    });
  }

  async function handleMarkReadyForTesting() {
    if (!projectPath || !taskId) {
      return;
    }

    await markReadyForTesting(projectPath, taskId);
  }

  return (
    <div className="pane-container">
      <div className="pane-header">
        <span className="pane-header-title">IMPLEMENTATION TODO</span>
        <label className="todo-agent-picker">
          <Bot size={14} />
          <select
            value={selectedAgent?.id ?? ""}
            disabled={implementationAgents.length === 0 || commandRunning}
            onChange={(event) => setSelectedAgentId(event.target.value)}
            title="Implementation Agent"
          >
            {implementationAgents.map((agent) => (
              <option key={agent.id} value={agent.id}>
                {agent.name}
              </option>
            ))}
          </select>
        </label>
      </div>

      <div className="todo-list-pane">
        {todos.length === 0 && (
          <div className="planning-empty">Confirm a plan to generate implementation todo items.</div>
        )}
        {todos.length > 0 && implementationAgents.length === 0 && (
          <div className="planning-empty">
            Enable or add an available implementation Agent in Settings before starting a todo.
          </div>
        )}

        {todos.map((todo, todoIndex) => {
          const active = todo.id === state.app.selectedTodoId || todo.status === "implementing";
          const done = todo.status === "done";
          const blocked = todo.status === "blocked";

          return (
            <div
              key={todo.id}
              className={`todo-row status-${todo.status}${active ? " active" : ""}`}
            >
              <button
                type="button"
                className="todo-start-button"
                disabled={done || !selectedAgent || commandRunning}
                title={
                  done
                    ? "Todo already completed"
                    : selectedAgent
                      ? `Start with ${selectedAgent.name}`
                      : "No implementation Agent is available"
                }
                onClick={() => {
                  if (!done) {
                    void handleStartWithAgent(todo, todoIndex);
                  }
                }}
              >
                <span className="todo-start-icon" aria-hidden="true">
                  {active ? <PlayCircle size={16} /> : <Play size={16} />}
                </span>
                <span className="todo-copy">
                  <span className="todo-title-line">
                    <span className="todo-title">{todo.title}</span>
                    <span className="todo-status-label">{TODO_STATUS_LABELS[todo.status]}</span>
                  </span>
                  <span className="todo-description">{todo.description}</span>
                </span>
              </button>
              <button
                type="button"
                className="todo-complete-button"
                title={
                  done
                    ? "Todo already completed"
                    : blocked
                      ? "Mark blocked todo done after resolving and verifying it"
                      : "Mark todo done after verification evidence is captured"
                }
                disabled={done}
                onClick={() => {
                  if (taskId) {
                    void completeTodo(projectPath, taskId, todo.id);
                  }
                }}
              >
                {done ? <CheckCircle2 size={15} /> : <Circle size={10} />}
              </button>
            </div>
          );
        })}
      </div>

      <div className="implementation-actions">
        <div>
          <div className="implementation-action-title">Testing handoff</div>
          <div className="implementation-action-copy">
            Complete every implementation todo before opening the debug validation stage.
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
    </div>
  );
}
