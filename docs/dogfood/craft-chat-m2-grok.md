# Craft Chat M2 — Grok dogfood note

- **Date**: 2026-09-17
- **Default agent**: `agent-grok`（绝对路径优先：`command -v grok` → `~/.grok/bin/grok` → `/Users/guojian/.grok/bin/grok`）
- **Process**: `ProcessSupervisor` + `ProcessKind::Chat`（`task_id=chat:{sessionId}`）

## Mac 手工路径（验收）

1. 打开项目 → Chat → 新建会话（应默认选中 Grok，若可用）。
2. 权限保持「探索」；发送一句只读问题，确认流式文本出现。
3. 点「停止」：进程组应被 SIGTERM；气泡状态 `aborted` / 「已停止」。
4. 再发一条：若上一轮捕获到 session id，应带 `--resume` 续聊。
5. 切到「自动」再发：CLI 应带 `--permission-mode acceptEdits`（可用诊断/日志核对 argv）。
6. 若 grok 流里出现 tool_use / command 事件，气泡应出现「工具 · …」卡片；否则仅文本（best-effort）。

## 本执行环境

Linux box / CI 通常无用户 Mac 上的 `grok` 二进制；M2 以单测 + 代码路径完成，真实 dogfood 在开发者 Mac 勾选。

## 权限映射（Phase 1）

| UI | stage | Grok | Codex | Claude |
|---|---|---|---|---|
| explore / ask | Planning | `plan` | `read-only` | 不传 `--permission-mode` |
| auto | Debugging | `acceptEdits` | `workspace-write` | `acceptEdits` |
