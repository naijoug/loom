# Chat 契约草图（M0）

- **Related**: [chat-first-prd.md](../guides/chat-first-prd.md)
- **Frontend types**: `src/domain/chat.ts`
- **Status**: Phase 1 / M1 — chat commands registered；`parts` 可视化与 ProcessSupervisor 对齐属 M2+

## 持久化

```
<project>/.loom/chat/
  index.json          # { schemaVersion, sessionIds: string[] }
  sessions/<id>.json  # ChatSession
```

- `schemaVersion`: 1
- 不写入 task store；升格时 **复制** 描述到新 Task，不移动会话文件

## 领域模型（与 `src/domain/chat.ts` 对齐）

见 TypeScript：`ChatSession`、`ChatMessage`、`ChatTurnStatus`、`ChatPermissionMode`。

要点：

- `ChatSession` 含 `id`、`projectPath`、`agentId`、`title`、`permissionMode`、`messages`、`createdAtMs`、`updatedAtMs`、可选 `resumeCommand`、可选 `activeTurnId`
- **无** `taskId` 必填字段；可选 `promotedTaskId` 仅在升格后回写
- `ChatMessage.role`: `user` | `assistant` | `system`
- `ChatMessage.status`: `complete` | `streaming` | `aborted` | `error`

## Tauri commands（拟定，M2 实现）

| Command | Input | Output |
|---|---|---|
| `chat_list_sessions` | `{ projectPath: string }` | `ChatSessionSummary[]` |
| `chat_create` | `{ projectPath, agentId, title? }` | `ChatSession` |
| `chat_get` | `{ projectPath, sessionId }` | `ChatSession` |
| `chat_send` | `{ projectPath, sessionId, text, permissionMode? }` | `{ turnId: string }` |
| `chat_abort` | `{ projectPath, sessionId, turnId? }` | `{ ok: true }` |
| `chat_set_agent` | `{ projectPath, sessionId, agentId }` | `ChatSession` |
| `chat_update_meta` | `{ projectPath, sessionId, title?, permissionMode?, status? }` | `ChatSession` |
| `chat_promote_to_task` | `{ projectPath, sessionId }` | `{ taskId: string }` stub |

调用链（强制）：

`chat_send` → 组装 prompt（含历史裁剪）→ `agent_adapter::prepare`（stage 由 permissionMode 映射）→ `command_runner` → 解析 stream → 追加 `ChatMessage` → emit 事件。

### permissionMode → adapter stage（Craft Phase 1 / M0）

权威细节与 CLI flag 见 [craft-local-chat-prd.md](../guides/craft-local-chat-prd.md) 附录。

| permissionMode | stage | 说明 |
|---|---|---|
| `explore` | `planning` | 默认；只读 sandbox / plan |
| `ask` | `planning` | Phase 1 与 explore 同级保守 CLI；**无**审批 UI |
| `auto` | `debugging` | 可写 + 跑命令（仍受 agent flags / execution_policy） |

旧值：`read_only`→`explore`；`read_write`→`ask`（偏安全）。`ChatMessage.parts`（text/tool/error）可选；`ChatSession.status`：`active`|`archived`（缺省 active）。

## Events（拟定，M2 注册到 manifest）

命名对齐现有 `loom://…`：

| Event | Payload |
|---|---|
| `loom://chat-stream` | `{ sessionId, turnId, messageId, delta: string, done: boolean }` |
| `loom://chat-turn-finished` | `{ sessionId, turnId, messageId, status, errorSummary? }` |

## 明确不做（契约层）

- Chat 不复用 `run_planning_discussion` 作为传输。
- 不把 `taskId` 塞进 adapter 必填语义来「假装」有任务；若 prepare 签名仍要 taskId，M2 使用稳定占位如 `chat:<sessionId>` 并在门禁中识别 chat 前缀（实现时文档化）。
