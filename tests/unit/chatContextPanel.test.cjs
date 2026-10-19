const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "../..");

test("ChatContextPanel empty-state defers MCP/Sources and shows loom/chat + diagnostics", () => {
  const src = fs.readFileSync(path.join(root, "src/features/chat/ChatContextPanel.tsx"), "utf8");
  assert.match(src, /\.loom\/chat\//);
  assert.match(src, /Sources \/ MCP/);
  assert.match(src, /后续阶段/);
  assert.match(src, /agent_diagnostics/);
  assert.doesNotMatch(src, /mcp_connect|connectMcp|MCP 连接入口/i);
});

test("Session menu promote stub stresses draft-only no auto run", () => {
  const src = fs.readFileSync(path.join(root, "src/features/chat/ChatSessionHeader.tsx"), "utf8");
  assert.match(src, /升格为任务（仅草稿，不自动开跑）/);
  assert.match(src, /不会自动推进任务状态机/);
});
