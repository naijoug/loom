# Craft 本机 Agent Chat — Phase 1 dogfood 清单

- **Date**: 2026-09-17
- **Plan**: [09:03-craft-inspired-local-agent-chat.md](../plans/2026-09-17/09:03-craft-inspired-local-agent-chat.md)
- **合并自**: [craft-chat-m2-grok.md](./craft-chat-m2-grok.md) · [craft-chat-m3-parity.md](./craft-chat-m3-parity.md) · M4 Inbox/Session 行为
- **默认 Agent 偏好**: grok → codex → claude（绝对路径探测）

> Linux box / CI 通常无本机 CLI；下列 **Mac 手工** 项在开发者机器勾选。自动化门禁：`pnpm exec tsc --noEmit`、相关 unit、`cargo test chat::tests`（若环境有 Rust）。

## 成功标准对照（Phase 1）

| # | 标准 | 勾选 |
|---|---|---|
| S1 | 冷启动默认进 Chat；Inbox + Transcript + Composer + Agent/权限三档 | [ ] |
| S2 | 垂直切片：新建 → 流式 → Abort → 再发 → 续聊/开新 CLI → 重启后 transcript 仍在 | [ ] |
| S3 | 权限三档可解释并落到 `prepare_invocation`（非仅前端） | [ ] |
| S4 | Chat 经 `agent_adapter` + `ProcessSupervisor(Chat)`；不绑 Task stage | [ ] |
| S5 | `pnpm check` / 相关单测；打开 Board 既有任务不回归 | [ ] |
| S6 | 文档标明 Phase 1 完成；Sources/MCP / Ask 审批 / 后台任务为 Phase 2+ | [ ] |

## M2 — Grok 垂直切片

| # | 步骤 | 勾选 |
|---|---|---|
| M2.1 | 新建会话默认选中 Grok（若可用） | [ ] |
| M2.2 | 「探索」发只读问题 → 流式文本 | [ ] |
| M2.3 | 「停止」→ 进程组 SIGTERM；气泡 `aborted` | [ ] |
| M2.4 | 再发 → 有 session id 时 `--resume` 续聊 | [ ] |
| M2.5 | 「自动」→ Grok `acceptEdits` / Codex `workspace-write` / Claude `acceptEdits` | [ ] |
| M2.6 | tool/command 事件 →「工具 · …」卡片（best-effort） | [ ] |

**权限映射（Phase 1）**

| UI | stage | Grok | Codex | Claude |
|---|---|---|---|---|
> **Phase 2 更新（2026-09-17）**：`ask` 改为 Debugging / 可写 CLI + Composer 每回合确认；见 `docs/plans/2026-09-17/10:55-craft-chat-phase2.md`。下表保留 Phase 1 历史映射。

| explore / ask | Planning | `plan` | `read-only` | 不传 `--permission-mode` |
| auto | Debugging | `acceptEdits` | `workspace-write` | `acceptEdits` |

## M3 — 多 Agent + 诊断

| # | 步骤 | 勾选 |
|---|---|---|
| M3.1 | 切 Claude；工具触发问题 → 同款 `parts` 工具卡 | [ ] |
| M3.2 | Claude 探索不带 permission-mode；自动才 `acceptEdits` | [ ] |
| M3.3 | Composer「状态 · …」显示路径/版本/说明（复用 `diagnose_agents`） | [ ] |
| M3.4 | 缺失二进制时 picker/状态显示「缺失」 | [ ] |

## M4 — Inbox / 恢复 / Session 菜单

| # | 步骤 | 勾选 |
|---|---|---|
| M4.1 | 旗标或回合 error →「需关注」过滤可见 | [ ] |
| M4.2 | 进行中 / 需关注 / 已归档过滤正确 | [ ] |
| M4.3 | 杀进程再开：streaming → aborted/interrupted；可再发送 | [ ] |
| M4.4 | 重命名 +「用首条消息作标题」持久化 | [ ] |
| M4.5 | Session 菜单：开新 CLI 会话（清 resume）；resume 失败不丢旧 handle | [ ] |

## M5 — 上下文空态 + 升格 stub

| # | 步骤 | 勾选 |
|---|---|---|
| M5.1 | 「上下文」面板：项目路径、`.loom/chat/` 提示、当前权限、Agent 诊断刷新 | [ ] |
| M5.2 | Sources/MCP 文案标明后续阶段；**无**连接入口 | [ ] |
| M5.3 | 升格菜单文案强调「仅草稿，不自动开跑」；点击只建草稿 Task | [ ] |
| M5.4 | 打开 Board 既有任务：阶段机行为无回归 | [ ] |

## 明确不测（Phase 2+）

- MCP / Sources 连接、OAuth、工具名前缀隔离
- Ask 档 per-tool / 写文件审批弹窗
- Board / `task_state` 重写、后台任务产品化
- Beta 公证 / 公开分发

## 记录

| 项 | 值 |
|---|---|
| 机器 / OS | |
| 可用 CLI | grok / codex / claude（删未装） |
| 执行人 | |
| 日期 | |
| 备注 | |
