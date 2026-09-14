# Chat-first PRD（M0）

- **Date**: 2026-09-14
- **Status**: draft / M0
- **Related plan**: [20:06-chat-first-refactor.md](../plans/2026-09-14/20:06-chat-first-refactor.md)
- **Contracts**: [chat-contracts.md](../architecture/chat-contracts.md)
- **IA**: [chat-first-ia.md](./chat-first-ia.md)

## 一句话

Loom 默认变成「打开项目 → 选本机 Agent → 自由对话」；Board / Planning / Testing 降为 advanced。Chat 调用本机 Codex CLI、Claude Code CLI、Grok CLI（经现有 adapter），**不**另起进程层。

## 用户故事

1. 作为开发者，我打开 Loom 后直接进入 Chat，而不是任务看板。
2. 我可以新建会话、在会话里发消息，并看到本机 Agent 的流式回复。
3. 我可以在 Codex / Claude Code / Grok（及已配置 Agent）之间切换。
4. 我可以随时打开 advanced 工作流处理既有任务，且不会因为 Chat 回合推进 task stage。
5. 我可以把当前对话「升格」为草稿任务（stub）：带上最后一条用户消息作描述，但不自动跑状态机。

## 默认路径

冷启动（已选项目）→ **Chat**（会话列表 + 消息流 + composer + Agent picker）。

无项目时：先 register / 选择项目，再进 Chat。

## ChatSession ≠ Task

| | ChatSession | Task |
|---|---|---|
| 目的 | 自由多轮对话 | 四阶段开发闭环 |
| 状态机 | 无 stage；仅 turn 状态（idle/streaming/error） | `task_state` 权威状态机 |
| 持久化 | `.loom/chat/sessions/*.json` | 现有 task store |
| Agent 调用 | `agent_adapter` + `command_runner`，chat 权限策略 | 按 stage 权限 |
| 与对方关系 | 可升格为草稿 Task（M5 stub） | 不反向吞并 Chat 历史（第一期） |

**硬规则**：Chat 回合不得隐式调用 `confirm_plan` / `mark_ready_for_testing` / 其它 task 推进命令。

## 非目标（phase 1）

- 不重写任务状态机。
- 不做云同步 / 插件 / 团队协作。
- 不删除 Board / Planning。
- 不依赖 Cursor Cloud Agents。
- 不在 M0 接通真实 CLI（M2+）。

## 已拍板的默认（原计划待确认）

1. **Chat 默认权限**：**只读默认**（映射 adapter 的 `planning`/`review` 类沙箱）。会话级开关「允许写文件/跑命令」打开后，才允许 implementation/debugging 能力；关闭时拒绝可写 prepare。
2. **持久化**：项目级 `.loom/chat/`（见 contracts）。用户级全局会话本期不做。
3. **Grok CLI**：以用户本机已安装二进制为准；经 `agent_diagnostics` 探测；最低版本等 M4 spike 再锁。
4. **升格 stub**：创建草稿 Task，`description` = 最后一条 user 消息（可截断）；**不**自动开 planning/implement。

## 成功标准（产品）

- 用户能说清：Chat 是对话，Task 是工作流。
- M1 起 UI 按 [IA](./chat-first-ia.md) 可导航。
- M2 起 Codex 流式对话可 dogfood，且仍走 `agent_adapter` → `command_runner`。
