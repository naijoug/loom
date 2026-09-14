const test = require("node:test");
const assert = require("node:assert/strict");
const { formatApprovalPrompt } = require("../../.tmp/test-build/src/utils/executionPolicy.js");

test("approval prompt explains dependency changes as environment risk", () => {
  const prompt = formatApprovalPrompt(
    {
      program: "pnpm",
      args: ["--filter", "web", "update"],
      cwd: "/repo/apps/web",
      projectPath: "/repo",
    },
    {
      decision: "approval_required",
      riskLevel: "medium",
      category: "dependency_install",
      detail: "command installs or changes project dependencies",
      normalizedProjectPath: "/repo",
      normalizedCwd: "/repo/apps/web",
    },
  );

  assert.match(prompt, /Loom requires approval/);
  assert.match(prompt, /pnpm --filter web update/);
  assert.match(prompt, /Working directory: \/repo\/apps\/web/);
  assert.match(prompt, /Risk: Dependency or environment change \(medium\)/);
  assert.match(prompt, /command installs or changes project dependencies/);
});

test("approval prompt labels destructive filesystem risk", () => {
  const prompt = formatApprovalPrompt(
    {
      program: "rm",
      args: ["-rf", "dist"],
      cwd: "/repo",
      projectPath: "/repo",
    },
    {
      decision: "approval_required",
      riskLevel: "high",
      category: "destructive_filesystem",
      detail: "command may permanently delete files",
      normalizedProjectPath: "/repo",
      normalizedCwd: "/repo",
    },
  );

  assert.match(prompt, /Risk: Destructive filesystem change \(high\)/);
});