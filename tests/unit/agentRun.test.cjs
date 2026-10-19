const test = require("node:test");
const assert = require("node:assert/strict");
const {
  buildAgentCommandArgs,
  buildAgentCommandInvocation,
  buildImplementationPrompt,
  buildRepairPrompt,
  buildResumeRepairPrompt,
  formatQuotedFeedback,
} = require("../../.tmp/test-build/src/utils/agentRun.js");

function task(overrides = {}) {
  return {
    projectPath: "/tmp/project",
    title: "Fix the build",
    repairContextPreview: "captured logs + todo context",
    finalPlan: "# Plan\n- step one",
    ...overrides,
  };
}

function todo(overrides = {}) {
  return {
    title: "Wire intent metadata",
    description: "Add command run intent fields.",
    ...overrides,
  };
}

test("formatQuotedFeedback returns just the note when nothing is quoted", () => {
  assert.equal(formatQuotedFeedback("  please fix  ", "", ""), "please fix");
});

test("formatQuotedFeedback fences the quoted log lines and attributes the command", () => {
  const content = formatQuotedFeedback("look here", "TypeError: x is undefined", "pnpm test");
  assert.match(content, /^look here\n\n/);
  assert.match(content, /Quoted from `pnpm test`:/);
  assert.match(content, /```\nTypeError: x is undefined\n```/);
});

test("formatQuotedFeedback works with a quote and no note", () => {
  const content = formatQuotedFeedback("", "boom", "cargo test");
  assert.match(content, /^Quoted from `cargo test`:/);
  assert.match(content, /```\nboom\n```/);
});

test("buildRepairPrompt embeds repair context, note, quoted logs, and plan", () => {
  const prompt = buildRepairPrompt(task(), "error on line 5", "the test fails");
  assert.match(prompt, /# Loom Repair Handoff/);
  assert.match(prompt, /captured logs \+ todo context/);
  assert.match(prompt, /Human note:\nthe test fails/);
  assert.match(prompt, /```\nerror on line 5\n```/);
  assert.match(prompt, /# Plan\n- step one/);
});

test("buildRepairPrompt degrades gracefully with no repair context or plan", () => {
  const prompt = buildRepairPrompt(task({ repairContextPreview: undefined, finalPlan: null }), "", "");
  assert.match(prompt, /\(none captured yet\)/);
  assert.match(prompt, /Human note:\n\(none\)/);
  assert.match(prompt, /Highlighted log lines[\s\S]*\(none\)/);
});

test("buildAgentCommandArgs uses codex exec form with write sandbox", () => {
  const args = buildAgentCommandArgs(
    { args: [], adapterType: "codex_cli", canWriteFiles: true },
    "/tmp/project",
    "PROMPT",
  );
  assert.deepEqual(args, [
    "exec",
    "--json",
    "--cd",
    "/tmp/project",
    "--sandbox",
    "workspace-write",
    "PROMPT",
  ]);
});

test("buildAgentCommandArgs uses claude stream-json form", () => {
  const args = buildAgentCommandArgs(
    { args: [], adapterType: "claude_code_cli", canWriteFiles: true },
    "/tmp/project",
    "PROMPT",
  );
  assert.deepEqual(args, [
    "-p",
    "PROMPT",
    "--permission-mode",
    "acceptEdits",
    "--verbose",
    "--output-format",
    "stream-json",
    "--include-partial-messages",
  ]);
});

test("buildAgentCommandArgs substitutes placeholders in custom args", () => {
  const args = buildAgentCommandArgs(
    { args: ["--cwd", "{projectPath}", "--task", "{prompt}"], adapterType: "cli", canWriteFiles: false },
    "/tmp/project",
    "PROMPT",
  );
  assert.deepEqual(args, ["--cwd", "/tmp/project", "--task", "PROMPT"]);
});

test("buildAgentCommandArgs appends the prompt when custom args omit the placeholder", () => {
  const args = buildAgentCommandArgs(
    { args: ["run"], adapterType: "cli", canWriteFiles: false },
    "/tmp/project",
    "PROMPT",
  );
  assert.deepEqual(args, ["run", "PROMPT"]);
});

test("buildAgentCommandInvocation resumes codex sessions through headless exec", () => {
  const invocation = buildAgentCommandInvocation(
    { command: "codex", args: [], adapterType: "codex_cli", canWriteFiles: true },
    "/tmp/project",
    "FIX",
    "codex resume session-1",
  );

  assert.equal(invocation.program, "codex");
  assert.equal(invocation.resumed, true);
  assert.deepEqual(invocation.args, ["exec", "resume", "--json", "session-1", "FIX"]);
});

test("buildAgentCommandInvocation resumes claude sessions in print mode", () => {
  const invocation = buildAgentCommandInvocation(
    { command: "claude", args: [], adapterType: "claude_code_cli", canWriteFiles: false },
    "/tmp/project",
    "FIX",
    "claude --resume session-1",
  );

  assert.equal(invocation.program, "claude");
  assert.equal(invocation.resumed, true);
  assert.deepEqual(invocation.args, [
    "-p",
    "--resume",
    "session-1",
    "--permission-mode",
    "plan",
    "--verbose",
    "--output-format",
    "stream-json",
    "--include-partial-messages",
    "FIX",
  ]);
});

test("buildAgentCommandInvocation falls back when resume command is unsupported", () => {
  const invocation = buildAgentCommandInvocation(
    { command: "custom", args: ["run"], adapterType: "cli", canWriteFiles: true },
    "/tmp/project",
    "PROMPT",
    "custom resume session-1",
  );

  assert.equal(invocation.resumed, false);
  assert.equal(invocation.program, "custom");
  assert.deepEqual(invocation.args, ["run", "PROMPT"]);
});

test("buildImplementationPrompt embeds scoped todo, rules, and final plan", () => {
  const prompt = buildImplementationPrompt(task({ title: "Refactor loops" }), todo(), 2);

  assert.match(prompt, /# Loom Implementation Handoff/);
  assert.match(prompt, /Task: Refactor loops/);
  assert.match(prompt, /Todo 3: Wire intent metadata/);
  assert.match(prompt, /Todo description:\nAdd command run intent fields\./);
  assert.match(prompt, /- Preserve unrelated user changes\./);
  assert.match(prompt, /# Plan\n- step one/);
});

test("buildResumeRepairPrompt uses incremental context without embedding the full plan", () => {
  const prompt = buildResumeRepairPrompt(task(), "validation failed again");

  assert.match(prompt, /# Loom Incremental Repair/);
  assert.match(prompt, /captured logs \+ todo context/);
  assert.match(prompt, /validation failed again/);
  assert.doesNotMatch(prompt, /# Plan\n- step one/);
});
