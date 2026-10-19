import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent,
} from "react";
import { AtSign, FileText, Send } from "lucide-react";
import type { AgentConfig } from "../../domain";
import { useAgentBridge } from "../../hooks/useAgentBridge";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useAppState } from "../../state/AppStateContext";
import { Button } from "../common/Button";
import {
  agentIdsForMentions,
  canonicalMention,
  defaultPlanningAgentIds,
  extractMentionNames,
  mentionAliases,
  unknownMentionNames,
} from "./participantSelection";
import { PlanningTimeline } from "./PlanningTimeline";
import "../../features/planning/Planning.css";

function taskTitleFromRequirement(requirement: string) {
  const firstLine = requirement.trim().split(/\r?\n/)[0] ?? "规划任务";
  return firstLine.slice(0, 48) || "规划任务";
}

function agentBadgeClass(agent: AgentConfig) {
  if (agent.adapterType === "codex_cli" || agent.id.includes("codex")) {
    return "codex";
  }
  if (agent.adapterType === "claude_code_cli" || agent.id.includes("claude")) {
    return "claude";
  }
  return "hermes";
}

function agentInitials(agent: AgentConfig) {
  if (agent.adapterType === "codex_cli" || agent.id.includes("codex")) {
    return "Co";
  }
  if (agent.adapterType === "claude_code_cli" || agent.id.includes("claude")) {
    return "Cl";
  }
  return agent.name
    .split(/\s+/)
    .map((part) => part[0])
    .join("")
    .slice(0, 2);
}

function planningCapable(agent: AgentConfig) {
  return agent.enabled && agent.available && agent.capabilities.includes("planning");
}

function participantStatusLabel(status: string | undefined, available: boolean) {
  if (status === "succeeded") {
    return "已完成";
  }
  if (status === "failed") {
    return "失败";
  }
  if (status === "running" || status === "retrying") {
    return "运行中";
  }
  return available ? "已选择" : "当前不可用";
}

interface PlanningChatProps {
  readOnly?: boolean;
}

export function PlanningChat({ readOnly = false }: PlanningChatProps) {
  const { state } = useAppState();
  const { loadAgents, runPlanningDiscussion } = useAgentBridge();
  const { createTask, loadTasks } = useTaskBridge();
  const project = state.projects.current;
  const task = state.tasks.find((candidate) => candidate.id === state.app.selectedTaskId) ?? null;
  const taskId = task?.id ?? null;
  const hasActivePlanning = Object.values(state.planningProgress).some(
    (event) =>
      event.taskId === taskId &&
      (event.status === "pending" ||
        event.status === "running" ||
        event.status === "retrying"),
  );
  // Seed the composer from a task's requirement only when it has not been
  // discussed yet; once a discussion exists (or for a brand-new task) start
  // empty. Keyed on the selected task id only, so user edits — including
  // clearing the field — are never overwritten on the next render.
  const [message, setMessage] = useState(
    task && !task.discussionSummary && !hasActivePlanning
      ? task.rawRequirement ?? ""
      : "",
  );
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  // When non-null, the `@`-autocomplete menu is open and this holds the partial
  // query typed after the `@`. Empty string means `@` with nothing yet.
  const [mentionQuery, setMentionQuery] = useState<string | null>(null);
  const [mentionIndex, setMentionIndex] = useState(0);
  const [submitting, setSubmitting] = useState(false);
  const [selectedAgentIds, setSelectedAgentIds] = useState<string[]>([]);
  // After a send, creating a task changes `taskId`, which would otherwise make
  // the reseed effect below refill the composer with the requirement we just
  // sent. This flag tells that effect to clear instead, for exactly one change.
  const skipReseedRef = useRef(false);
  const selectionTouchedRef = useRef(false);
  const selectedTaskRef = useRef(taskId);
  const lastMentionKeyRef = useRef("");
  const timelineScrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void loadAgents();
  }, [loadAgents]);

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
    setMessage(
      task && !task.discussionSummary && !hasActivePlanning
        ? task.rawRequirement ?? ""
        : "",
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hasActivePlanning, taskId]);

  const mentionedNames = useMemo(() => extractMentionNames(message), [message]);
  const configuredPlanningAgents = useMemo(
    () =>
      state.agents.filter(
        (agent) => agent.enabled && agent.capabilities.includes("planning"),
      ),
    [state.agents],
  );
  const availableAgents = useMemo(
    () => state.agents.filter(planningCapable),
    [state.agents],
  );
  const availableAgentKey = availableAgents.map((agent) => agent.id).join("|");
  const preferredAgentKey = task?.selectedPlanningAgentIds.join("|") ?? "";
  const selectedAgents = useMemo(
    () => availableAgents.filter((agent) => selectedAgentIds.includes(agent.id)),
    [availableAgents, selectedAgentIds],
  );
  const unknownMentions = useMemo(
    () => unknownMentionNames(message, configuredPlanningAgents),
    [configuredPlanningAgents, message],
  );
  const unavailableMentionAgents = useMemo(() => {
    const mentionedIds = new Set(agentIdsForMentions(message, configuredPlanningAgents));
    return configuredPlanningAgents.filter(
      (agent) => mentionedIds.has(agent.id) && !planningCapable(agent),
    );
  }, [configuredPlanningAgents, message]);
  const hasMentionError =
    unknownMentions.length > 0 || unavailableMentionAgents.length > 0;
  const summaryAgents = useMemo(() => {
    if (!task?.planningRuns.length || task.selectedPlanningAgentIds.length === 0) {
      return selectedAgents;
    }
    const byId = new Map(state.agents.map((agent) => [agent.id, agent]));
    return task.selectedPlanningAgentIds
      .map((agentId) => byId.get(agentId))
      .filter((agent): agent is AgentConfig => Boolean(agent));
  }, [selectedAgents, state.agents, task]);
  const latestDraftByAgent = useMemo(() => {
    const result = new Map<string, string>();
    for (const invocation of task?.agentInvocations ?? []) {
      if (invocation.promptSummary !== "Synthesize final plan") {
        result.set(invocation.agentId, invocation.status);
      }
    }
    return result;
  }, [task?.agentInvocations]);

  useEffect(() => {
    const taskChanged = selectedTaskRef.current !== taskId;
    if (taskChanged) {
      selectedTaskRef.current = taskId;
      selectionTouchedRef.current = false;
      lastMentionKeyRef.current = "";
    }

    setSelectedAgentIds((current) => {
      if (taskChanged || !selectionTouchedRef.current) {
        return defaultPlanningAgentIds(availableAgents, task?.selectedPlanningAgentIds);
      }
      const availableIds = new Set(availableAgents.map((agent) => agent.id));
      return current.filter((agentId) => availableIds.has(agentId));
    });
    // Keys make this effect react only to meaningful participant changes rather
    // than every agents/tasks array identity change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [availableAgentKey, preferredAgentKey, taskId]);

  useEffect(() => {
    if (timelineScrollRef.current) {
      timelineScrollRef.current.scrollTop = 0;
    }
  }, [taskId, task?.planningRuns.length]);

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

  function syncSelectedAgentsFromMentions(value: string) {
    const mentionKey = extractMentionNames(value).join("|");
    if (mentionKey === lastMentionKeyRef.current) {
      return;
    }
    lastMentionKeyRef.current = mentionKey;
    const ids = agentIdsForMentions(value, availableAgents);
    if (mentionKey && ids.length > 0) {
      selectionTouchedRef.current = true;
      setSelectedAgentIds(ids);
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
    const nextMessage = replaced + after;
    setMessage(nextMessage);
    syncSelectedAgentsFromMentions(nextMessage);
    setMentionQuery(null);
    requestAnimationFrame(() => {
      const node = textareaRef.current;
      if (node) {
        node.focus();
        node.setSelectionRange(replaced.length, replaced.length);
      }
    });
  }

  function toggleAgentSelection(agent: AgentConfig) {
    selectionTouchedRef.current = true;
    lastMentionKeyRef.current = mentionedNames.join("|");
    setSelectedAgentIds((current) =>
      current.includes(agent.id)
        ? current.filter((agentId) => agentId !== agent.id)
        : [...current, agent.id],
    );
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
  const discussionRunning = submitting || hasActivePlanning;

  async function handleDiscuss(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (
      !project ||
      !message.trim() ||
      selectedAgents.length === 0 ||
      hasMentionError ||
      discussionRunning
    ) {
      return;
    }

    const outgoing = message.trim();
    const agentIds = selectedAgents.map((agent) => agent.id);
    const creatingTask = task === null;

    setSubmitting(true);
    if (timelineScrollRef.current) {
      timelineScrollRef.current.scrollTop = 0;
    }
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

  if (!project) {
    return (
      <div className="planning-empty-route">
        <h2>先选择一个项目</h2>
        <p>选择项目后即可发起多 Agent 规划讨论。</p>
      </div>
    );
  }

  return (
    <div className="planning-chat">
      <section className="planning-chat-main">
        <div className="planning-chat-header">
          <div>
            <div className="planning-kicker">规划讨论室</div>
            <h1>{task?.title ?? "新的规划讨论"}</h1>
          </div>
          {task?.finalPlanPath && (
            <span className="planning-plan-ref" title={task.finalPlanPath}>
              <FileText size={14} />
              <span>{task.finalPlanPath.split("/").slice(-2).join("/")}</span>
            </span>
          )}
        </div>

        <div className="planning-timeline-scroll" ref={timelineScrollRef}>
          <PlanningTimeline projectPath={project.path} task={task} readOnly={readOnly} />
        </div>

        {!readOnly && (
        <form className="planning-composer-v2" onSubmit={handleDiscuss}>
          <div className="planning-input-wrap">
            <textarea
              ref={textareaRef}
              value={message}
              onChange={(event) => {
                setMessage(event.target.value);
                syncMentionMenu(event.target.value, event.target.selectionStart);
                syncSelectedAgentsFromMentions(event.target.value);
              }}
              onKeyDown={handleComposerKeyDown}
              onClick={(event) =>
                syncMentionMenu(event.currentTarget.value, event.currentTarget.selectionStart)
              }
              onBlur={() => setMentionQuery(null)}
              placeholder="描述需求。输入 @ 调用本地 Agent，例如 @codex 或 @claude。"
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
                    aria-pressed={active}
                    onClick={() => toggleAgentSelection(agent)}
                    title={
                      active
                        ? `不让 ${agent.name} 参与本轮讨论`
                        : `让 ${agent.name} 参与本轮讨论`
                    }
                  >
                    <span className={`loom-agent-avatar ${agentBadgeClass(agent)}`}>
                      {agentInitials(agent)}
                    </span>
                    {agent.name}
                  </button>
                );
              })}
              {availableAgents.length === 0 && (
                <span className="planning-chip-hint">暂无可用于规划的 Agent。</span>
              )}
            </div>
            <Button
              type="submit"
              variant="primary"
              iconRight={<Send size={14} />}
              disabled={
                !message.trim() ||
                selectedAgents.length === 0 ||
                hasMentionError ||
                discussionRunning
              }
            >
              {discussionRunning ? "讨论中…" : "发送"}
            </Button>
          </div>
          <div className={`planning-composer-meta${hasMentionError ? " error" : ""}`}>
            {unknownMentions.length > 0 ? (
              <span>
                未找到 {unknownMentions.map((name) => `@${name}`).join("、")}。请从上方 Agent 列表中选择。
              </span>
            ) : unavailableMentionAgents.length > 0 ? (
              <span>
                {unavailableMentionAgents.map((agent) => agent.name).join("、")} 当前不可用，请先到设置中检查命令。
              </span>
            ) : discussionRunning ? (
              <span>本轮讨论正在进行；你可以先写下一轮补充，待完成后发送。</span>
            ) : selectedAgents.length > 0 ? (
              <span>{selectedAgents.length} 个 Agent 将并行起草，并在成功后交叉评审与合成。</span>
            ) : (
              <span>至少选择 1 个可用 Agent 才能开始讨论。</span>
            )}
          </div>
          {state.app.taskError && <div className="planning-error">{state.app.taskError}</div>}
        </form>
        )}
      </section>
      <aside className="planning-summary-panel" aria-label="规划汇总">
        <section className="planning-summary-card">
          <div className="planning-summary-label">参与 Agent</div>
          <div className="planning-summary-agents">
            {summaryAgents.map((agent) => {
              const status = latestDraftByAgent.get(agent.id);
              return (
                <span className="planning-summary-agent" key={agent.id}>
                  <span className={`loom-agent-avatar ${agentBadgeClass(agent)}`}>
                    {agentInitials(agent)}
                  </span>
                  <span className="planning-summary-agent-name">{agent.name}</span>
                  <span className={`planning-summary-agent-state status-${status ?? "idle"}`}>
                    {participantStatusLabel(status, agent.available)}
                  </span>
                </span>
              );
            })}
            {summaryAgents.length === 0 && <span className="planning-summary-muted">暂无可用 Agent</span>}
          </div>
        </section>

        <section className="planning-summary-card">
          <div className="planning-summary-label">讨论摘要</div>
          <p>{task?.discussionSummary || "发送需求后，Loom 会在这里汇总多 Agent 的规划结论、风险和待确认项。"}</p>
        </section>

        <section className="planning-summary-card">
          <div className="planning-summary-label">最终计划</div>
          <p>{task?.finalPlanPath ? task.finalPlanPath.split("/").slice(-2).join("/") : "尚未生成最终计划文档"}</p>
        </section>

        <section className="planning-summary-card">
          <div className="planning-summary-label">决策记录</div>
          {task?.planningDecisions.length ? (
            <ul className="planning-summary-list">
              {task.planningDecisions.slice(-4).map((decision) => (
                <li key={decision.id}>
                  <strong>{decision.title}</strong>
                  <span>{decision.content}</span>
                </li>
              ))}
            </ul>
          ) : (
            <p>暂无人工决策。</p>
          )}
        </section>
      </aside>
    </div>
  );
}
