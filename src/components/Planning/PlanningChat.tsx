import { useEffect, useMemo, useState, type FormEvent } from "react";
import { AlertTriangle, AtSign, Bot, CheckCircle2, FileText, Send } from "lucide-react";
import type { AgentConfig, PlanReview, Task } from "../../domain";
import { useAgentBridge } from "../../hooks/useAgentBridge";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useAppState } from "../../state/AppStateContext";
import { Button } from "../common/Button";
import "./Planning.css";

function taskTitleFromRequirement(requirement: string) {
  const firstLine = requirement.trim().split(/\r?\n/)[0] ?? "Planning task";
  return firstLine.slice(0, 48) || "Planning task";
}

function extractMentionNames(value: string) {
  return Array.from(value.matchAll(/@([\w-]+)/g), (match) => match[1].toLowerCase());
}

function mentionAliases(agent: AgentConfig) {
  const normalizedName = agent.name.toLowerCase().replace(/\s+/g, "-");
  const aliases = new Set([
    agent.id.toLowerCase(),
    agent.name.toLowerCase(),
    normalizedName,
    agent.command.toLowerCase(),
  ]);

  if (agent.adapterType === "codex_cli") {
    aliases.add("codex");
  }
  if (agent.adapterType === "claude_code_cli") {
    aliases.add("claude");
    aliases.add("claude-code");
  }
  if (agent.adapterType === "amp_cli") {
    aliases.add("amp");
  }

  return aliases;
}

function planningCapable(agent: AgentConfig) {
  return agent.enabled && agent.available && agent.capabilities.includes("planning");
}

function latestPlanningReviews(task: Task | null) {
  if (!task) {
    return [];
  }

  const latestRun = task.planningRuns[task.planningRuns.length - 1];
  if (!latestRun) {
    return task.planReviews;
  }

  return task.planReviews.filter((review) => review.planningRunId === latestRun.id);
}

function severityLabel(review: PlanReview) {
  if (review.severity === "blocker") {
    return "Blocker";
  }
  if (review.severity === "risk") {
    return "Risk";
  }
  return "Info";
}

export function PlanningChat() {
  const { state, dispatch } = useAppState();
  const { loadAgents, runPlanReviews, runPlanningDiscussion } = useAgentBridge();
  const { confirmPlan, createTask, loadTasks, recordPlanningDecision } = useTaskBridge();
  const project = state.projects.current;
  const task = state.tasks.find((candidate) => candidate.id === state.app.selectedTaskId) ?? null;
  const [message, setMessage] = useState(task?.rawRequirement ?? "");
  const [decision, setDecision] = useState("");

  useEffect(() => {
    void loadAgents();
  }, [loadAgents]);

  useEffect(() => {
    if (project) {
      void loadTasks(project.path);
    }
  }, [loadTasks, project]);

  useEffect(() => {
    if (!message && task?.rawRequirement) {
      setMessage(task.rawRequirement);
    }
  }, [message, task?.rawRequirement]);

  const mentionedNames = useMemo(() => extractMentionNames(message), [message]);
  const selectedAgents = useMemo(() => {
    if (mentionedNames.length === 0) {
      return state.agents.filter(planningCapable);
    }

    return state.agents.filter((agent) => {
      const aliases = mentionAliases(agent);
      return planningCapable(agent) && mentionedNames.some((name) => aliases.has(name));
    });
  }, [mentionedNames, state.agents]);
  const reviews = useMemo(() => latestPlanningReviews(task), [task]);
  const riskReviews = reviews.filter((review) => review.severity !== "info");
  const successfulInvocations =
    task?.agentInvocations.filter((invocation) => invocation.status === "succeeded") ?? [];
  const failedInvocations =
    task?.agentInvocations.filter((invocation) => invocation.status === "failed") ?? [];

  async function handleDiscuss(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (!project || !message.trim() || selectedAgents.length === 0) {
      return;
    }

    const activeTask =
      task ??
      (await createTask({
        projectPath: project.path,
        title: taskTitleFromRequirement(message),
        rawRequirement: message.trim(),
      }));

    if (!activeTask) {
      return;
    }

    await runPlanningDiscussion({
      projectPath: project.path,
      taskId: activeTask.id,
      requirement: message.trim(),
      agentIds: selectedAgents.map((agent) => agent.id),
    });
  }

  async function handleRunReviews() {
    if (!project || !task) {
      return;
    }

    await runPlanReviews(project.path, task.id);
  }

  async function handleRecordDecision(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (!project || !task || !decision.trim()) {
      return;
    }

    const updated = await recordPlanningDecision({
      projectPath: project.path,
      taskId: task.id,
      title: "Planning decision",
      content: decision.trim(),
    });

    if (updated) {
      setDecision("");
    }
  }

  async function handleCreateTasks() {
    if (!project || !task?.finalPlan) {
      return;
    }

    const updated = await confirmPlan(project.path, task.id);
    if (updated) {
      dispatch({ type: "app/viewSelected", view: "board" });
    }
  }

  if (!project) {
    return (
      <div className="planning-empty-route">
        <h2>Select a project</h2>
        <p>Choose a project before starting a planning discussion.</p>
      </div>
    );
  }

  return (
    <div className="planning-chat">
      <section className="planning-chat-main">
        <div className="planning-chat-header">
          <div>
            <div className="planning-kicker">Planning room</div>
            <h1>{task?.title ?? "New planning discussion"}</h1>
          </div>
          {task?.finalPlanPath && (
            <span className="planning-plan-ref">
              <FileText size={14} />
              <span>{task.finalPlanPath}</span>
            </span>
          )}
        </div>

        <div className="planning-thread">
          {(task || message.trim()) && (
            <article className="chat-message user-chat-message">
              <div className="chat-author">You</div>
              <p>{task?.rawRequirement ?? message.replace(/\s*@[\w-]+/g, "")}</p>
            </article>
          )}

          {task?.agentInvocations.map((invocation) => (
            <article className="chat-message agent-chat-message" key={invocation.id}>
              <div className="chat-author">
                <Bot size={14} />
                {invocation.agentName} · {invocation.status}
              </div>
              <p>{invocation.outputSummary}</p>
              {invocation.evidenceRef && <span className="chat-evidence">{invocation.evidenceRef}</span>}
              {invocation.status === "failed" && invocation.stderrTail.length > 0 && (
                <pre>{invocation.stderrTail.join("\n")}</pre>
              )}
            </article>
          ))}

          {task?.discussionSummary && (
            <article className="chat-message final-chat-message">
              <div className="chat-author">
                <CheckCircle2 size={14} />
                Summary
              </div>
              <p>{task.discussionSummary}</p>
              {task.finalPlan && <pre>{task.finalPlan}</pre>}
            </article>
          )}
        </div>

        <form className="planning-composer-v2" onSubmit={handleDiscuss}>
          <textarea
            value={message}
            onChange={(event) => setMessage(event.target.value)}
            placeholder="Describe the task. Mention @codex, @claude, or @amp to narrow participants."
          />
          <div className="planning-composer-bar">
            <div className="planning-agent-chips">
              {selectedAgents.map((agent) => (
                <span className="planning-agent-chip" key={agent.id}>
                  <AtSign size={12} />
                  {agent.name}
                </span>
              ))}
              {selectedAgents.length === 0 && (
                <span className="planning-chip-hint">No available planning Agent selected.</span>
              )}
            </div>
            <Button
              type="submit"
              variant="primary"
              iconRight={<Send size={14} />}
              disabled={!message.trim() || selectedAgents.length === 0 || state.app.isLoadingTasks}
            >
              Start discussion
            </Button>
          </div>
          {state.app.taskError && <div className="planning-error">{state.app.taskError}</div>}
        </form>
      </section>

      <aside className="planning-summary">
        <section className="summary-card">
          <div className="summary-card-title">Consensus</div>
          <p>{task?.discussionSummary ?? "Run a planning discussion to generate consensus."}</p>
          <div className="summary-stats">
            <span>{successfulInvocations.length} succeeded</span>
            <span>{failedInvocations.length} failed</span>
          </div>
        </section>

        <section className="summary-card">
          <div className="summary-card-title">
            <AlertTriangle size={15} />
            Risks
          </div>
          {riskReviews.length > 0 ? (
            <ul className="summary-list">
              {riskReviews.map((review) => (
                <li key={review.id}>
                  <strong>{severityLabel(review)}</strong>
                  <span>{review.finding}</span>
                </li>
              ))}
            </ul>
          ) : (
            <p>No review risks recorded yet.</p>
          )}
          <Button
            type="button"
            variant="ghost"
            disabled={!task || successfulInvocations.length < 1 || state.app.isLoadingTasks}
            onClick={handleRunReviews}
          >
            Run reviews
          </Button>
        </section>

        <section className="summary-card">
          <div className="summary-card-title">Human decisions</div>
          {task?.planningDecisions.length ? (
            <ul className="summary-list">
              {task.planningDecisions.map((item) => (
                <li key={item.id}>
                  <strong>{item.title}</strong>
                  <span>{item.content}</span>
                </li>
              ))}
            </ul>
          ) : (
            <p>No decisions recorded.</p>
          )}
          <form className="decision-form" onSubmit={handleRecordDecision}>
            <textarea
              value={decision}
              onChange={(event) => setDecision(event.target.value)}
              placeholder="Record a scope, tradeoff, or risk decision."
              disabled={!task}
            />
            <Button type="submit" variant="ghost" disabled={!task || !decision.trim()}>
              Record
            </Button>
          </form>
        </section>

        <section className="summary-card">
          <div className="summary-card-title">Create tasks</div>
          <p>Confirm the final plan to generate implementation todos and return to the board.</p>
          <Button
            type="button"
            variant="primary"
            disabled={!task?.finalPlan || state.app.isLoadingTasks}
            onClick={handleCreateTasks}
          >
            Create tasks from plan
          </Button>
        </section>
      </aside>
    </div>
  );
}
