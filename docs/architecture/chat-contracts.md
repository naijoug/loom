# Chat 契约（Phase 1）

- **Related**: [craft-local-chat-prd.md](../guides/craft-local-chat-prd.md) · [craft-local-chat-ia.md](../guides/craft-local-chat-ia.md) · [chat-first-prd.md](../guides/chat-first-prd.md)
- **Frontend types**: `src/domain/chat.ts`
- **Backend**: `src-tauri/src/chat.rs`
- **Status**: Phase 1 **completed**（M0–M5）— commands / events / ProcessSupervisor(Chat) / Inbox meta / 上下文空态已落地；MCP Sources 属 Phase 2+

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
| `chat_create` | `{ projectPath, agentId, title? }` | `ChatSession` |
| `chat_get` | `{ projectPath, sessionId }` | `ChatSession`（load 时 reconcile 中断的 streaming → aborted） |
| `chat_send` | `{ projectPath, sessionId, text, permissionMode? }` | `{ turnId, session }` |
| `chat_abort` | `{ projectPath, sessionId, turnId? }` | `{ ok: true }` |
| `chat_set_agent` | `{ projectPath, sessionId, agentId }` | `ChatSession` |
| `chat_update_meta` | `{ projectPath, sessionId, title?, permissionMode?, status?, flagged?, titleFromFirstMessage? }` | `ChatSession` |
| `chat_clear_resume` | `{ projectPath, sessionId }` | `ChatSession`（开新 CLI 会话；失败不丢旧 handle） |
| `chat_promote_to_task` | `{ projectPath, sessionId }` | `{ taskId, task, session }` **草稿 stub** |

调用链（强制）：

`chat_send` → 组装 prompt → `agent_adapter::prepare_invocation`（stage 由 permissionMode 映射）→ `ProcessSupervisor`（`ProcessKind::Chat`，`task_id=chat:{sessionId}`，`run_id=turnId`）→ 解析 stream → 追加 `ChatMessage` / `parts` → emit 事件。

仅在 turn **complete** 时写入新的 resume handle；abort/error/timeout 保留旧 handle。

Turn timeout：`CHAT_TURN_TIMEOUT_MS`（默认 10 分钟）到期时 `ProcessSupervisor::request_stop(turnId, "chat_timeout")`；消息 `status=aborted`，`errorSummary` 为可读超时文案；会话 `turnStatus` 回到 idle，可再发送。

### permissionMode → adapter stage（Phase 2）

权威 CLI flag 见 [craft-local-chat-prd.md](../guides/craft-local-chat-prd.md) 附录。Ask 门禁见 Phase 2 计划 `docs/plans/2026-09-17/10:55-craft-chat-phase2.md`。

| permissionMode | stage | 说明 |
|---|---|---|
| `explore` | `planning` | 默认；只读 sandbox / plan |
| `ask` | `debugging` | 可写 CLI；**每回合发送前** Composer 确认「允许本回合写文件/跑可写工具」 |
| `auto` | `debugging` | 可写 + 跑命令（无确认；仍受 agent flags / execution_policy） |

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

## v2 冻结补充（10:15 rebuild）

执行入口：`docs/plans/2026-09-17/10:15-local-agent-chat-rebuild.md`。

### IPC 信封
- **`{ input: {...} }`**：`chat_create`、`chat_update_meta`、`chat_send`、`chat_abort`、`chat_promote_to_task`
- **平铺字段**：`chat_list_sessions`、`chat_get`、`chat_set_agent`、`chat_clear_resume`
- 前端唯一封装：`src/api/chatClient.ts`

### 回合状态
`idle → streaming → (complete | aborted | error)`；同会话同时仅一个 streaming 回合；abort/timeout 均走 ProcessSupervisor。

### 权限
`explore` / `ask` / `auto`（旧 `read_only`→explore，`read_write`→ask）。Ask 发送前确认门已在 Phase 2 落地。

### 存储与隔离
`.loom/chat/`；ChatSession ≠ Task；禁止为 Chat 伪造 taskId。路径 canonical，拒绝 `..` 逃逸。崩溃后 streaming → interrupted，由用户手动继续。

