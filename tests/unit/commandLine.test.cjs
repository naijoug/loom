const test = require("node:test");
const assert = require("node:assert/strict");
const { parseCommandLine } = require("../../.tmp/test-build/src/utils/commandLine.js");

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
