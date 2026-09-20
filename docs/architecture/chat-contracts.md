# Chat 当前契约与重构目标

- **Related**: [craft-local-chat-prd.md](../guides/craft-local-chat-prd.md) · [craft-local-chat-ia.md](../guides/craft-local-chat-ia.md) · [chat-first-prd.md](../guides/chat-first-prd.md)
- **Frontend types**: `src/domain/chat.ts`
- **Backend**: `src-tauri/src/chat.rs`
- **核对日期**：2026-09-20；当前 schema v1，以生产类型、IPC wrapper 和 Rust 实现为现状依据。历史 Phase 1 完成记录不代表 v2 目标或所有 CLI 已验收。
- **范围来源**：[现行需求](../requirements.md)；[重构计划](../plans/2026-09-17/10:15-local-agent-chat-rebuild.md) 管理后续工作。

## 持久化

```
<project>/.loom/chat/
  index.json          # { schemaVersion, sessionIds: string[] }
  sessions/<id>.json  # ChatSession
```

- `schemaVersion`: 1
- 不写入 task store；升格时 **复制** 描述到新 Task 草稿，不移动会话文件，**不**自动开跑状态机

## 领域模型（与 `src/domain/chat.ts` 对齐）

见 TypeScript：`ChatSession`、`ChatSessionSummary`、`ChatMessage`、`ChatTurnStatus`、`ChatPermissionMode`。

要点：

- `ChatSession`：`id`、`projectPath`、`agentId`、`title`、`permissionMode`、`messages`、`createdAtMs`、`updatedAtMs`、可选 `resumeCommand`、`activeTurnId`、`turnStatus`、`status`（`active`|`archived`）、`flagged`、可选 `promotedTaskId`
- **无** `taskId` 必填字段；`promotedTaskId` 仅在升格 stub 后回写
- `ChatSessionSummary` 可含派生 `needsAttention`（旗标或最近错误）
- `ChatMessage.role`: `user` | `assistant` | `system`
- `ChatMessage.status`: `complete` | `streaming` | `aborted` | `error`
- `ChatMessage.parts?`: `text` / `tool` / `error`（stream best-effort；无 parts 回退 `content`）

## Tauri commands

| Command | Input | Output |
|---|---|---|
| `chat_list_sessions` | `{ projectPath }` | `ChatSessionSummary[]` |
| `chat_create` | `{ input: { projectPath, agentId, title?, permissionMode? } }` | `ChatSession` |
| `chat_get` | `{ projectPath, sessionId }` | `ChatSession`（load 时 reconcile 中断的 streaming → aborted） |
| `chat_send` | `{ input: { projectPath, sessionId, text, permissionMode? } }` | `{ turnId, session }` |
| `chat_abort` | `{ input: { projectPath, sessionId, turnId? } }` | `null`（Rust `Result<(), String>`） |
| `chat_set_agent` | `{ projectPath, sessionId, agentId }` | `ChatSession` |
| `chat_update_meta` | `{ input: { projectPath, sessionId, title?, permissionMode?, status?, flagged?, titleFromFirstMessage? } }` | `ChatSession` |
| `chat_clear_resume` | `{ projectPath, sessionId }` | `ChatSession`（开新 CLI 会话；失败不丢旧 handle） |
| `chat_promote_to_task` | `{ input: { projectPath, sessionId } }` | `{ taskId, task, session }` **草稿 stub** |

当前调用链：

`chat_send` → `chat_context::prepare_chat_invocation` → `agent_adapter::prepare_invocation`（stage 由 permissionMode 映射）→ `ProcessSupervisor`（`ProcessKind::Chat`，`task_id=chat:{sessionId}`，`run_id=turnId`）→ 解析 stream → 追加 `ChatMessage` / `parts` → emit 事件。

仅在 turn **complete** 时写入新的 resume handle；abort/error/timeout 保留旧 handle。

Turn timeout：`CHAT_TURN_TIMEOUT_MS`（默认 10 分钟）到期时 `ProcessSupervisor::request_stop(turnId, "chat_timeout")`；消息 `status=aborted`，`errorSummary` 为可读超时文案；会话 `turnStatus` 回到 idle，可再发送。

### 当前 permissionMode → adapter stage

以下是当前参数映射，不是 CLI 安全能力验收结果。Ask 仅 Composer 每回合确认，Rust 当前不校验独立授权凭证，也无逐工具审批传输。

| permissionMode | stage | 说明 |
|---|---|---|
| `explore` | `planning` | 默认；请求只读/plan，保障取决于已验收的 CLI 能力 |
| `ask` | `debugging` | 可写 CLI；**每回合发送前** Composer 确认「允许本回合写文件/跑可写工具」 |
| `auto` | `debugging` | 请求可写；无 Composer 确认，受 adapter 配置及 CLI 自身策略限制 |

旧值：`read_only`→`explore`；`read_write`→`ask`（偏安全：可写但需确认）。

| UI | Grok | Codex | Claude |
|---|---|---|---|
| explore | `--permission-mode plan` | `--sandbox read-only` | 不传 `--permission-mode` |
| ask / auto | `acceptEdits` | `workspace-write` | `acceptEdits` |

## Events

| Event | Payload |
|---|---|
| `loom://chat-stream` | `{ sessionId, turnId, messageId, delta, done, part? }` |
| `loom://chat-turn-finished` | `{ sessionId, turnId, messageId, status, errorSummary? }` |

## UI 边界（Phase 1）

- Inbox：`active` | `needs_attention`（过滤）| `archived`；status 持久化仅两态
- 可选右侧「上下文」空态：项目路径、`.loom/chat` 提示、当前权限、复用 `agent_diagnostics`；**不**实现 MCP/Sources 连接
- 升格入口在 Session 菜单；文案强调仅草稿、不自动开跑

## 明确不做（契约层 / Phase 2+）

- Chat 不复用 `run_planning_discussion` 作为传输
- 不把 Chat turn 绑进 Task stage
- MCP / Sources 连接、Ask **per-tool** 运行时审批（当前为发送前确认 stub）、Craft 五态 Inbox（P2-M4 可选）
- 后台回合指示 / 超时已在 Phase 2 P2-M3 落地（非完整 Background tasks 产品）

## 上下文与续聊

`chat_context.rs` 使用 adapter 返回的 `resumed` 判断传输方式，而不是仅看磁盘中是否有 resume 字符串。原生续聊只传最新输入；无句柄或 adapter 明确返回非 resume 调用时，使用受预算约束的历史 JSON；适配器明确拒绝的句柄/协议直接报错，不绕过拒绝。CLI 已开始执行后失败不会自动重试。

历史最多 12 条、每条正文最多 2000 个 Unicode 字符、序列化后总预算 12000 字符，保留消息角色和状态并明确标记截断；最新输入完整保留，超过 32000 字符在启动前报错。原始转录不随 prompt 裁剪。当前不生成语义摘要，不能宣称已保留全部早期约束。换 Agent/权限会清除不相容句柄，活动回合拒绝这类配置变更。

## 当前限制与 v2 目标（尚未完成）

| 方面 | 当前实现 | 重构目标 |
|---|---|---|
| 进程归属 | supervisor 使用 `task_id=chat:{sessionId}` 兼容键；没有创建领域 Task | 独立 `ProcessOwner::Chat`，移除兼容假 taskId |
| 回合状态 | session `idle/streaming`；消息 `complete/streaming/aborted/error`，崩溃恢复为 aborted | starting/running/cancelling 与独立 completed/failed/cancelled/timed_out/interrupted |
| 持久化 | `.loom/chat/` v1 JSON，结束时保存结果 | v2 journal、原子快照、增量持久化、单 writer 与重放 |
| 事件 | 两类现有事件，含 sessionId/turnId | 带 projectKey 与单调 seq 的统一事件和补流 |
| 权限 | stage 参数映射；Ask UI 回合授权；部分 CLI/resume 限制待实测 | 能力驱动的有效权限；有双向协议才能提供逐工具审批 |

现有路径检查不能替代计划中的 canonical projectKey、所有 realpath/symlink 边界验收。Chat 直接启动 adapter 进程，不经过 Task command runner 的完整 execution_policy 门禁；不得将 Task 的逐命令策略宣称为 Chat 内部工具执行的保障。详细边界见 [安全策略](../security-policy.md)。

前端 IPC 唯一封装为 `src/api/chatClient.ts`，具体信封见上表。推进 v2 时同步修改 producer、consumer 和契约样本；更新现状表后才能将目标标为完成。
