import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent,
} from "react";
import { createPortal } from "react-dom";
import { HEADER_ACTIONS_SLOT_ID } from "../../layouts/AppLayout";
import {
  AlertTriangle,
  AtSign,
  Bot,
  CheckCircle2,
  FileText,
  PanelRightClose,
  PanelRightOpen,
  Send,
} from "lucide-react";
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

function canonicalMention(agent: AgentConfig) {
  if (agent.adapterType === "codex_cli") {
    return "codex";
  }
  if (agent.adapterType === "claude_code_cli") {
    return "claude";
  }
  if (agent.adapterType === "amp_cli") {
    return "amp";
  }
  return agent.name.toLowerCase().replace(/\s+/g, "-");
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
  const taskId = task?.id ?? null;
  // Seed the composer from a task's requirement only when it has not been
  // discussed yet; once a discussion exists (or for a brand-new task) start
  // empty. Keyed on the selected task id only, so user edits — including
  // clearing the field — are never overwritten on the next render.
  const [message, setMessage] = useState(
    task && !task.discussionSummary ? task.rawRequirement ?? "" : "",
  );
  const [decision, setDecision] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  // When non-null, the `@`-autocomplete menu is open and this holds the partial
  // query typed after the `@`. Empty string means `@` with nothing yet.
  const [mentionQuery, setMentionQuery] = useState<string | null>(null);
  const [mentionIndex, setMentionIndex] = useState(0);
  const chatRef = useRef<HTMLDivElement>(null);
  const [summaryCollapsed, setSummaryCollapsed] = useState(false);
  const [headerSlot, setHeaderSlot] = useState<HTMLElement | null>(null);
  const [submitting, setSubmitting] = useState(false);
  // After a send, creating a task changes `taskId`, which would otherwise make
  // the reseed effect below refill the composer with the requirement we just
  // sent. This flag tells that effect to clear instead, for exactly one change.
  const skipReseedRef = useRef(false);

  useEffect(() => {
    void loadAgents();
  }, [loadAgents]);

  // Resolve the header slot once mounted so the panel toggle can be portalled
  // onto the header line, aligned with the sidebar toggle.
  useEffect(() => {
    setHeaderSlot(document.getElementById(HEADER_ACTIONS_SLOT_ID));
  }, []);

  // Auto-collapse the summary panel when the planning area gets too narrow to
  // hold both columns (e.g. the window shrinks, or the sidebar expands). We key
  // off the container width — not the viewport — and only react on threshold
  // crossings so manual toggles in the middle ground are preserved.
  useEffect(() => {
    const el = chatRef.current;
    if (!el || typeof ResizeObserver === "undefined") {
      return;
    }

    const NARROW = 900;
    let wasNarrow = el.clientWidth < NARROW;
    const observer = new ResizeObserver((entries) => {
      const width = entries[0]?.contentRect.width ?? el.clientWidth;
      const narrow = width < NARROW;
      if (narrow !== wasNarrow) {
        setSummaryCollapsed(narrow);
        wasNarrow = narrow;
      }
    });

    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (project) {
      void loadTasks(project.path);
    }
  }, [loadTasks, project]);

  useEffect(() => {
    if (skipReseedRef.current) {
      skipReseedRef.current = false;
      setMessage("");
      return;
    }
    setMessage(task && !task.discussionSummary ? task.rawRequirement ?? "" : "");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [taskId]);

  const mentionedNames = useMemo(() => extractMentionNames(message), [message]);
  const availableAgents = useMemo(
    () => state.agents.filter(planningCapable),
    [state.agents],
  );
  const selectedAgents = useMemo(() => {
    if (mentionedNames.length === 0) {
      return availableAgents;
    }

    const matched = availableAgents.filter((agent) => {
      const aliases = mentionAliases(agent);
      return mentionedNames.some((name) => aliases.has(name));
    });

    // A typo'd or unknown @mention should never block the send — fall back to
    // every available planning Agent rather than an empty selection.
    return matched.length > 0 ? matched : availableAgents;
  }, [mentionedNames, availableAgents]);

  const mentionSuggestions = useMemo(() => {
    if (mentionQuery === null) {
      return [];
    }
    if (mentionQuery === "") {
      return availableAgents;
    }
    return availableAgents.filter((agent) => {
      if (agent.name.toLowerCase().includes(mentionQuery)) {
        return true;
      }
      return Array.from(mentionAliases(agent)).some((alias) =>
        alias.startsWith(mentionQuery),
      );
    });
  }, [mentionQuery, availableAgents]);

  function syncMentionMenu(value: string, cursor: number) {
    const before = value.slice(0, cursor);
    const match = before.match(/(?:^|\s)@([\w-]*)$/);
    if (match) {
      setMentionQuery(match[1].toLowerCase());
      setMentionIndex(0);
    } else {
      setMentionQuery(null);
    }
  }

  function applyMention(agent: AgentConfig) {
    const el = textareaRef.current;
    const cursor = el ? el.selectionStart : message.length;
    const before = message.slice(0, cursor);
    const after = message.slice(cursor);
    const replaced = before.replace(
      /(^|\s)@([\w-]*)$/,
      (_full, prefix: string) => `${prefix}@${canonicalMention(agent)} `,
    );
    setMessage(replaced + after);
    setMentionQuery(null);
    requestAnimationFrame(() => {
      const node = textareaRef.current;
      if (node) {
        node.focus();
        node.setSelectionRange(replaced.length, replaced.length);
      }
    });
  }

  function toggleAgentMention(agent: AgentConfig) {
    const aliases = mentionAliases(agent);
    const mentioned = mentionedNames.some((name) => aliases.has(name));
    if (mentioned) {
      const next = message
        .replace(/@([\w-]+)/g, (full, name: string) =>
          aliases.has(name.toLowerCase()) ? "" : full,
        )
        .replace(/[ \t]{2,}/g, " ")
        .replace(/[ \t]+\n/g, "\n");
      setMessage(next.trimStart());
    } else {
      const needsSpace = message.length > 0 && !/\s$/.test(message);
      setMessage(`${message}${needsSpace ? " " : ""}@${canonicalMention(agent)} `);
    }
    textareaRef.current?.focus();
  }

  function handleComposerKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (mentionQuery === null || mentionSuggestions.length === 0) {
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setMentionIndex((index) => (index + 1) % mentionSuggestions.length);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setMentionIndex(
        (index) => (index - 1 + mentionSuggestions.length) % mentionSuggestions.length,
      );
    } else if (event.key === "Enter" || event.key === "Tab") {
      event.preventDefault();
      applyMention(mentionSuggestions[mentionIndex] ?? mentionSuggestions[0]);
    } else if (event.key === "Escape") {
      setMentionQuery(null);
    }
  }
  // Drive the composer's running state from the in-flight send only — not the
  // shared `isLoadingTasks` flag, which also flips during unrelated task-list
  // loads and would otherwise disable Send for no reason.
  const discussionRunning = submitting;
  const reviews = useMemo(() => latestPlanningReviews(task), [task]);
  const riskReviews = reviews.filter((review) => review.severity !== "info");
  const successfulInvocations =
    task?.agentInvocations.filter((invocation) => invocation.status === "succeeded") ?? [];
  const failedInvocations =
    task?.agentInvocations.filter((invocation) => invocation.status === "failed") ?? [];

  async function handleDiscuss(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (!project || !message.trim() || selectedAgents.length === 0 || submitting) {
      return;
    }

    const outgoing = message.trim();
    const agentIds = selectedAgents.map((agent) => agent.id);
    const creatingTask = task === null;

    setSubmitting(true);
    // Clear the composer as soon as the message is sent. When a new task is
    // created the taskId change would refill it, so suppress that one reseed.
    setMessage("");
    if (creatingTask) {
      skipReseedRef.current = true;
    }

    try {
      const activeTask =
        task ??
        (await createTask({
          projectPath: project.path,
          title: taskTitleFromRequirement(outgoing),
          rawRequirement: outgoing,
        }));

      if (!activeTask) {
        setMessage(outgoing); // restore so the user doesn't lose their input
        return;
      }

      const result = await runPlanningDiscussion({
        projectPath: project.path,
        taskId: activeTask.id,
        requirement: outgoing,
        agentIds,
      });

      if (!result) {
        setMessage(outgoing);
      }
    } finally {
      setSubmitting(false);
    }
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
    <div
      className={`planning-chat${summaryCollapsed ? " summary-collapsed" : ""}`}
      ref={chatRef}
    >
      {headerSlot &&
        createPortal(
          <button
            type="button"
            className="panel-toggle-button"
            title={summaryCollapsed ? "Show planning panel" : "Hide planning panel"}
            aria-label={summaryCollapsed ? "Show planning panel" : "Hide planning panel"}
            onClick={() => setSummaryCollapsed((value) => !value)}
          >
            {summaryCollapsed ? <PanelRightOpen size={16} /> : <PanelRightClose size={16} />}
          </button>,
          headerSlot,
        )}
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
          {task?.rawRequirement && (
            <article className="chat-message user-chat-message">
              <div className="chat-author">You</div>
              <p>{task.rawRequirement}</p>
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

          {discussionRunning && (
            <article className="chat-message agent-chat-message planning-running">
              <div className="chat-author">
                <Bot size={14} />
                Running planning discussion
              </div>
              <p>
                {selectedAgents.length > 0
                  ? `${selectedAgents.map((agent) => agent.name).join(", ")} are drafting plans. Real agents run sequentially and can take a few minutes each.`
                  : "Agents are working. This can take a few minutes."}
              </p>
            </article>
          )}

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
          <div className="planning-input-wrap">
            <textarea
              ref={textareaRef}
              value={message}
              onChange={(event) => {
                setMessage(event.target.value);
                syncMentionMenu(event.target.value, event.target.selectionStart);
              }}
              onKeyDown={handleComposerKeyDown}
              onClick={(event) =>
                syncMentionMenu(event.currentTarget.value, event.currentTarget.selectionStart)
              }
              onBlur={() => setMentionQuery(null)}
              placeholder="Describe the task. Type @ to invoke a local agent (e.g. @codex, @claude, @amp)."
            />
            {mentionQuery !== null && mentionSuggestions.length > 0 && (
              <ul className="mention-menu" role="listbox">
                {mentionSuggestions.map((agent, index) => (
                  <li key={agent.id}>
                    <button
                      type="button"
                      className={`mention-option${index === mentionIndex ? " active" : ""}`}
                      // mousedown (not click) so the textarea keeps focus/caret
                      onMouseDown={(event) => {
                        event.preventDefault();
                        applyMention(agent);
                      }}
                    >
                      <AtSign size={12} />
                      <span className="mention-option-name">{agent.name}</span>
                      <span className="mention-option-alias">@{canonicalMention(agent)}</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
          <div className="planning-composer-bar">
            <div className="planning-agent-chips">
              {availableAgents.map((agent) => {
                const active = selectedAgents.includes(agent);
                return (
                  <button
                    type="button"
                    key={agent.id}
                    className={`planning-agent-chip${active ? " active" : ""}`}
                    onClick={() => toggleAgentMention(agent)}
                    title={
                      active
                        ? `Remove @${canonicalMention(agent)} from this discussion`
                        : `Invoke @${canonicalMention(agent)} in this discussion`
                    }
                  >
                    <AtSign size={12} />
                    {agent.name}
                  </button>
                );
              })}
              {availableAgents.length === 0 && (
                <span className="planning-chip-hint">No planning-capable Agent available.</span>
              )}
            </div>
            <Button
              type="submit"
              variant="primary"
              iconRight={<Send size={14} />}
              disabled={!message.trim() || selectedAgents.length === 0 || discussionRunning}
            >
              {discussionRunning ? "Sending…" : "Send"}
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
