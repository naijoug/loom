import type { AgentConfig, Task } from "../domain";

export function hasImplementationCapability(agent: AgentConfig) {
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
