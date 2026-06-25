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
import { PlanningTimeline } from "./PlanningTimeline";
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
  return aliases;
}

function canonicalMention(agent: AgentConfig) {
  if (agent.adapterType === "codex_cli") {
    return "codex";
  }
  if (agent.adapterType === "claude_code_cli") {
    return "claude";
  }
  return agent.name.toLowerCase().replace(/\s+/g, "-");
}

function planningCapable(agent: AgentConfig) {
  return agent.enabled && agent.available && agent.capabilities.includes("planning");
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
  // Seed the composer from a task's requirement only when it has not been
  // discussed yet; once a discussion exists (or for a brand-new task) start
  // empty. Keyed on the selected task id only, so user edits — including
  // clearing the field — are never overwritten on the next render.
  const [message, setMessage] = useState(
    task && !task.discussionSummary ? task.rawRequirement ?? "" : "",
  );
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  // When non-null, the `@`-autocomplete menu is open and this holds the partial
  // query typed after the `@`. Empty string means `@` with nothing yet.
  const [mentionQuery, setMentionQuery] = useState<string | null>(null);
  const [mentionIndex, setMentionIndex] = useState(0);
  const [submitting, setSubmitting] = useState(false);
  // After a send, creating a task changes `taskId`, which would otherwise make
  // the reseed effect below refill the composer with the requirement we just
  // sent. This flag tells that effect to clear instead, for exactly one change.
  const skipReseedRef = useRef(false);

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
            <span className="planning-plan-ref" title={task.finalPlanPath}>
              <FileText size={14} />
              <span>{task.finalPlanPath.split("/").slice(-2).join("/")}</span>
            </span>
          )}
        </div>

        <div className="planning-timeline-scroll">
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
              }}
              onKeyDown={handleComposerKeyDown}
              onClick={(event) =>
                syncMentionMenu(event.currentTarget.value, event.currentTarget.selectionStart)
              }
              onBlur={() => setMentionQuery(null)}
              placeholder="Describe the task. Type @ to invoke a local agent (e.g. @codex, @claude)."
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
        )}
      </section>
    </div>
  );
}
