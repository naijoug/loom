import type { AgentConfig, PlanTodoItem, Task } from "../domain";
import { parseCommandLine } from "./commandLine";

export function hasImplementationCapability(agent: AgentConfig) {
  return (
    agent.enabled &&
    agent.available &&
    agent.canWriteFiles &&
    agent.canRunCommands &&
    agent.adapterType !== "dummy" &&
    agent.capabilities.includes("implementation")
  );
}

function replaceRuntimePlaceholders(args: string[], projectPath: string, prompt: string) {
  return args.map((arg) =>
    arg.split("{projectPath}").join(projectPath).split("{prompt}").join(prompt),
  );
}

/** Build the argv for an implementation agent, mirroring SessionPane. */
export function buildAgentCommandArgs(agent: AgentConfig, projectPath: string, prompt: string) {
  if (agent.args.length > 0) {
    const args = replaceRuntimePlaceholders(agent.args, projectPath, prompt);
    return agent.args.some((arg) => arg.includes("{prompt}")) ? args : [...args, prompt];
  }

  switch (agent.adapterType) {
    case "codex_cli":
      return [
        "exec",
        "--json",
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
        "--verbose",
        "--output-format",
        "stream-json",
        "--include-partial-messages",
      ];
    default:
      return [prompt];
  }
}

export interface AgentCommandInvocation {
  program: string;
  args: string[];
  resumed: boolean;
}

function hasFlag(args: string[], names: string[]) {
  return args.some((arg) => names.includes(arg));
}

function buildCodexResumeArgs(resumeArgs: string[], prompt: string) {
  if (resumeArgs[0] !== "resume" || !resumeArgs[1]) {
    return null;
  }

  return ["exec", "resume", "--json", resumeArgs[1], prompt];
}

function buildClaudeResumeArgs(resumeArgs: string[], agent: AgentConfig, prompt: string) {
  const args = [...resumeArgs];
  if (!hasFlag(args, ["-p", "--print"])) {
    args.unshift("-p");
  }
  if (!hasFlag(args, ["--permission-mode"])) {
    args.push("--permission-mode", agent.canWriteFiles ? "acceptEdits" : "plan");
  }
  if (!hasFlag(args, ["--verbose"])) {
    args.push("--verbose");
  }
  if (!hasFlag(args, ["--output-format"])) {
    args.push("--output-format", "stream-json");
  }
  if (!hasFlag(args, ["--include-partial-messages"])) {
    args.push("--include-partial-messages");
  }
  args.push(prompt);
  return args;
}

export function buildAgentCommandInvocation(
  agent: AgentConfig,
  projectPath: string,
  prompt: string,
  resumeCommand?: string | null,
): AgentCommandInvocation {
  if (resumeCommand?.trim()) {
    try {
      const parsed = parseCommandLine(resumeCommand);
      if (parsed.program) {
        const resumedArgs =
          agent.adapterType === "codex_cli"
            ? buildCodexResumeArgs(parsed.args, prompt)
            : agent.adapterType === "claude_code_cli"
              ? buildClaudeResumeArgs(parsed.args, agent, prompt)
              : null;
        if (resumedArgs) {
          return { program: parsed.program, args: resumedArgs, resumed: true };
        }
      }
    } catch {
      // Fall back to a fresh invocation if the stored display command is stale.
    }
  }

  return {
    program: agent.command,
    args: buildAgentCommandArgs(agent, projectPath, prompt),
    resumed: false,
  };
}

export function buildImplementationPrompt(task: Task, todo: PlanTodoItem, todoIndex: number) {
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

/**
 * Persisted feedback content for a fix request: the human note plus the log
 * lines they quoted (fenced, attributed to the source command). Stored as a
 * `UserFeedback` entry so the conversation survives reloads.
 */
export function formatQuotedFeedback(note: string, quoteText: string, command: string) {
  if (!quoteText.trim()) {
    return note.trim();
  }
  const header = note.trim() ? `${note.trim()}\n\n` : "";
  return `${header}Quoted from \`${command}\`:\n\`\`\`\n${quoteText.trim()}\n\`\`\``;
}

/**
 * Repair prompt for a Testing-stage fix run: packages the repair context, the
 * human's note, and the log lines they highlighted as the failure.
 */
export function buildRepairPrompt(task: Task, quotedLogs: string, note: string) {
  return [
    "# Loom Repair Handoff",
    "",
    "You are fixing a failure found while testing a confirmed Loom plan.",
    "",
    `Project: ${task.projectPath}`,
    `Task: ${task.title}`,
    "",
    "Repair context:",
    task.repairContextPreview?.trim() || "(none captured yet)",
    "",
    "Human note:",
    note.trim() || "(none)",
    "",
    "Highlighted log lines (the human marked these as the problem):",
    quotedLogs.trim() ? "```\n" + quotedLogs.trim() + "\n```" : "(none)",
    "",
    "Execution rules:",
    "- Diagnose the highlighted failure and change only what is needed.",
    "- Preserve unrelated user changes.",
    "- Re-run the relevant validation command before reporting completion.",
    "- Report changed files, the fix, verification evidence, and remaining risk.",
    "",
    "Confirmed final plan:",
    task.finalPlan ?? "(none)",
  ].join("\n");
}

export function buildResumeRepairPrompt(task: Task, note: string) {
  return [
    "# Loom Incremental Repair",
    "",
    "Continue the existing implementation session and fix the latest validation failure.",
    "",
    `Project: ${task.projectPath}`,
    `Task: ${task.title}`,
    "",
    "New repair context:",
    task.repairContextPreview?.trim() || "(none captured yet)",
    "",
    "New instruction:",
    note.trim() || "(none)",
    "",
    "Execution rules:",
    "- Treat this as an incremental continuation of the existing session.",
    "- Change only what is needed for the current validation failure.",
    "- Preserve unrelated user changes.",
    "- Re-run the relevant validation command before reporting completion.",
    "- Report changed files, the fix, verification evidence, and remaining risk.",
  ].join("\n");
}
