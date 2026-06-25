const test = require("node:test");
const assert = require("node:assert/strict");
const {
  buildAgentCommandArgs,
  buildRepairPrompt,
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
  assert.deepEqual(args, ["exec", "--cd", "/tmp/project", "--sandbox", "workspace-write", "PROMPT"]);
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
