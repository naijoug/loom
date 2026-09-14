const test = require("node:test");
const assert = require("node:assert/strict");
const { detectDangerousCommand, parseCommandLine } = require("../../.tmp/test-build/src/utils/commandLine.js");

test("parses program and simple args", () => {
  assert.deepEqual(parseCommandLine("pnpm dev --host 127.0.0.1"), {
    program: "pnpm",
    args: ["dev", "--host", "127.0.0.1"],
  });
});

test("preserves quoted arguments and escaped spaces", () => {
  assert.deepEqual(parseCommandLine("claude -p \"fix the bug\" src/foo\\ bar.ts"), {
    program: "claude",
    args: ["-p", "fix the bug", "src/foo bar.ts"],
  });
});

test("single quotes keep backslashes literal", () => {
  assert.deepEqual(parseCommandLine("echo 'a\\ b'"), {
    program: "echo",
    args: ["a\\ b"],
  });
});

test("rejects unclosed quotes", () => {
  assert.throws(() => parseCommandLine("pnpm dev \"unterminated"), /Unclosed double quote/);
});

test("empty command returns an empty program", () => {
  assert.deepEqual(parseCommandLine("   "), {
    program: "",
    args: [],
  });
});

test("detects destructive and reset commands", () => {
  assert.equal(detectDangerousCommand("rm -rf dist").reason, "destructive-remove");
  assert.equal(detectDangerousCommand("git reset --hard HEAD").reason, "git-reset-hard");
});

test("detects dependency installs and package mutations", () => {
  assert.equal(detectDangerousCommand("pnpm install").reason, "dependency-install");
  assert.equal(detectDangerousCommand("npm ci").reason, "dependency-install");
  assert.equal(detectDangerousCommand("pnpm --filter web update").reason, "dependency-install");
  assert.equal(detectDangerousCommand("yarn add vite").reason, "dependency-install");
  assert.equal(detectDangerousCommand("bun remove left-pad").reason, "dependency-install");
  assert.equal(detectDangerousCommand("cargo add serde").reason, "dependency-install");
  assert.equal(detectDangerousCommand("python -m pip uninstall requests").reason, "dependency-install");
});

test("detects production-like targets", () => {
  assert.equal(detectDangerousCommand("curl https://api.production.example.com/deploy").reason, "production-target");
});

test("does not flag ordinary validation commands", () => {
  assert.equal(detectDangerousCommand("pnpm test"), null);
  assert.equal(detectDangerousCommand("cargo check --manifest-path src-tauri/Cargo.toml"), null);
  assert.equal(detectDangerousCommand("npm run add-fixture"), null);
  assert.equal(detectDangerousCommand("pnpm run update"), null);
});
