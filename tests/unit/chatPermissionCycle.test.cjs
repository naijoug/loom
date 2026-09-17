const test = require("node:test");
const assert = require("node:assert/strict");

/** Mirror of src/features/chat/chatPermission.ts helpers. */
const CHAT_PERMISSION_CYCLE = ["explore", "ask", "auto"];
const ASK_TURN_CONFIRM_LABEL = "允许本回合写文件/跑可写工具";

function normalizeChatPermissionMode(value) {
  switch (value) {
    case "explore":
    case "ask":
    case "auto":
      return value;
    case "read_only":
      return "explore";
    case "read_write":
      return "ask";
    default:
      return "explore";
  }
}

function chatPermissionAllowsWrite(mode) {
  const normalized = normalizeChatPermissionMode(mode);
  return normalized === "ask" || normalized === "auto";
}

function cyclePermissionMode(mode) {
  const current = normalizeChatPermissionMode(mode);
  const index = CHAT_PERMISSION_CYCLE.indexOf(current);
  return CHAT_PERMISSION_CYCLE[(index + 1) % CHAT_PERMISSION_CYCLE.length];
}

const LABELS = {
  explore: "探索",
  ask: "询问编辑",
  auto: "自动",
};

function permissionModeLabel(mode) {
  return LABELS[normalizeChatPermissionMode(mode)];
}

function askTurnRequiresConfirm(mode) {
  return normalizeChatPermissionMode(mode) === "ask";
}

function permissionCliHint(adapterType, mode) {
  const write = chatPermissionAllowsWrite(mode);
  const askGate = askTurnRequiresConfirm(mode);
  const gateSuffix = askGate ? " · 发送前确认" : "";
  switch (adapterType) {
    case "grok_cli":
      return (write ? "CLI · --permission-mode acceptEdits" : "CLI · --permission-mode plan") + gateSuffix;
    case "codex_cli":
      return (write ? "CLI · --sandbox workspace-write" : "CLI · --sandbox read-only") + gateSuffix;
    case "claude_code_cli":
      return (
        (write ? "CLI · --permission-mode acceptEdits" : "CLI · 默认只读（不传 permission-mode）") +
        gateSuffix
      );
    default:
      return (write ? "CLI · 可写 stage" : "CLI · 只读 stage") + gateSuffix;
  }
}

function diagnosticStatusLabel(status) {
  switch (status) {
    case "ready":
      return "就绪";
    case "disabled":
      return "已停用";
    case "missing":
      return "缺失";
    default:
      return "未知";
  }
}

test("cyclePermissionMode walks explore→ask→auto→explore", () => {
  assert.equal(cyclePermissionMode("explore"), "ask");
  assert.equal(cyclePermissionMode("ask"), "auto");
  assert.equal(cyclePermissionMode("auto"), "explore");
  assert.equal(cyclePermissionMode("read_only"), "ask");
  assert.equal(cyclePermissionMode("read_write"), "auto");
});

test("permissionModeLabel uses Chinese Craft-style copy", () => {
  assert.equal(permissionModeLabel("explore"), "探索");
  assert.equal(permissionModeLabel("ask"), "询问编辑");
  assert.equal(permissionModeLabel("auto"), "自动");
});

test("askTurnRequiresConfirm only for ask (Phase 2 gate)", () => {
  assert.equal(askTurnRequiresConfirm("explore"), false);
  assert.equal(askTurnRequiresConfirm("ask"), true);
  assert.equal(askTurnRequiresConfirm("auto"), false);
  assert.equal(askTurnRequiresConfirm("read_write"), true);
  assert.equal(ASK_TURN_CONFIRM_LABEL.includes("本回合"), true);
});

test("permissionCliHint: ask uses writable flags + 发送前确认", () => {
  assert.equal(permissionCliHint("grok_cli", "explore"), "CLI · --permission-mode plan");
  assert.equal(
    permissionCliHint("grok_cli", "ask"),
    "CLI · --permission-mode acceptEdits · 发送前确认",
  );
  assert.equal(permissionCliHint("grok_cli", "auto"), "CLI · --permission-mode acceptEdits");
  assert.equal(permissionCliHint("codex_cli", "explore"), "CLI · --sandbox read-only");
  assert.equal(
    permissionCliHint("codex_cli", "ask"),
    "CLI · --sandbox workspace-write · 发送前确认",
  );
  assert.equal(permissionCliHint("codex_cli", "auto"), "CLI · --sandbox workspace-write");
  assert.equal(
    permissionCliHint("claude_code_cli", "explore"),
    "CLI · 默认只读（不传 permission-mode）",
  );
  assert.equal(
    permissionCliHint("claude_code_cli", "ask"),
    "CLI · --permission-mode acceptEdits · 发送前确认",
  );
  assert.equal(permissionCliHint("claude_code_cli", "auto"), "CLI · --permission-mode acceptEdits");
});

test("diagnosticStatusLabel covers ready/disabled/missing", () => {
  assert.equal(diagnosticStatusLabel("ready"), "就绪");
  assert.equal(diagnosticStatusLabel("disabled"), "已停用");
  assert.equal(diagnosticStatusLabel("missing"), "缺失");
  assert.equal(diagnosticStatusLabel(undefined), "未知");
});
