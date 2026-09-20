# Craft 启发的本机 Agent Chat — PRD（Phase 1 / M0）

- **Date**: 2026-09-17
- **Status**: Phase 1/2 历史设计；2026-09-20 起以 [现行需求](../requirements.md) 和 [Chat 契约](../architecture/chat-contracts.md) 为当前范围与实现依据。
- **Related plan**: [09:03-craft-inspired-local-agent-chat.md](../plans/2026-09-17/09:03-craft-inspired-local-agent-chat.md)
- **IA**: [craft-local-chat-ia.md](./craft-local-chat-ia.md)
- **Contracts**: [chat-contracts.md](../architecture/chat-contracts.md) · `src/domain/chat.ts` · `src-tauri/src/chat.rs`
- **Builds on**: [chat-first-prd.md](./chat-first-prd.md)（2026-09-14 chat-first M0–M5 已完成）

## 一句话

在已落地的 chat-first 壳之上，把 Loom 默认体验推进为 Craft Agents 风格的**本机 Agent Chat**：会话收件箱 + 转录 + composer + Agent / 三档权限，端到端 dogfood 调用本机已接线 CLI；**不**重写 Task 状态机，不引入云同步 / 市场 / Electron 服务端。

## 与 2026-09-14 chat-first 的关系

| | chat-first（已完成） | Craft Phase 1（本计划） |
|---|---|---|
| 默认路由 | 冷启动进 Chat | **保持**；IA 升级为 Inbox / Transcript / Composer |
| 会话模型 | `ChatSession` ≠ Task，`.loom/chat/` | **扩展**字段（三档权限、可选 status、消息 `parts`）；不换存储根 |
| 调用链 | `agent_adapter::prepare_invocation` | **强制复用**；权限档位 → stage / CLI flag 单一映射表 |
| 权限 UI | `read_only` / `read_write` checkbox | `explore` / `ask` / `auto`（旧值别名迁移） |
| Board | advanced 保留 | **不删除**；仍为 advanced |
| 升格 Task | stub 草稿 | **保持** stub；不自动跑阶段机 |

硬规则不变：Chat 回合不得隐式推进 `task_state`。

## 用户故事

1. 作为开发者，我打开已选项目的 Loom 后直接进入 Chat，而不是任务看板。
2. 我可以在左侧收件箱里新建 / 选中 / 归档会话，主区阅读完整 transcript，底部用 composer 发消息并中止当前回合。
3. 我可以在 Header 切换本机 Agent（Codex / Claude Code / Grok 及已配置者），并在探索 / 询问编辑 / 自动三档权限间切换；档位写入会话并影响真实 CLI 沙箱。
4. 流式回复中，我至少能看到文本；后续里程碑可见工具 / 命令类片段（`ChatMessagePart`），失败有可读错误气泡。
5. 我可以续聊同一 CLI 会话，或显式「开新 CLI 会话」；重启应用后 transcript 仍在。
6. 我仍可从 advanced 打开 Board / Planning / Testing 处理既有任务，且 Chat 不会偷偷推进 task stage。
7. 我可以把对话升格为草稿 Task（stub），但不自动开跑。

## 默认路径

冷启动（已选项目）→ **Chat**（Inbox + Transcript + Composer + Agent picker + 权限档位）。

无项目时：先 register / 选择项目，再进 Chat。

Board / Planning / Testing = advanced，侧栏或菜单可达，**不是**默认首页。

## 非目标（Phase 1）

- 不重写 `task_state` / 四阶段闭环 / Board 看板语义。
- 不做云同步、会话分享、插件 / Agent 市场、团队账号、远程 headless server。
- 不删除 Board / Planning；不迁移到 Electron / Bun。
- 不完整复刻 Craft Sources（MCP/REST/OAuth）、Automations、Background Tasks、Multi-file Diff。
- 不做 per-tool 审批弹窗（当前 Ask 的回合授权定义见 Chat 契约）。
- Beta 公证 / 公开分发另案；不依赖 Cursor Cloud Agents。

## 成功标准（产品）

- 用户能说清：Chat 是本机 Agent 对话工作台；Task / Board 是结构化开发闭环。
- 冷启动默认 Chat；IA 具备 Inbox / Transcript / Composer / Agent / 权限三档（见 IA）。
- 至少一条垂直切片（M2）：本机 Agent 新建会话 → 流式回复 → Abort → 再发 → 续聊或开新 CLI 会话 → 重启 transcript 仍在。
- 权限三档可解释并落到 `prepare_invocation`，不是仅前端文案。
- `pnpm check` 与触及单测通过；打开既有 Board 任务不回归。
- 文档标明：本 PRD 是 thorough refactor 的 Phase 1；Sources/MCP、完整 Ask 审批、更细 Inbox 状态属后续阶段。

## 已冻结决策（原计划待确认 → 拍板）

1. **Dogfood 首选 Agent**：用 `command -v` 探测，优先级 **grok → codex → claude**；以本机第一个命中者为 M2 垂直切片验收 Agent。探测结果与失败原因走既有 `agent_diagnostics`，并在 Chat / Settings 可读。
2. **Ask 档 Phase 2 语义**：映射为**可写 CLI**（同 Auto 的 Debugging / acceptEdits），但 **每回合发送前** 须在 Composer 确认「允许本回合写文件/跑可写工具」。**不做** stream 级 per-tool 审批弹窗（完整 tool-gate 仍可后续）。
3. **会话 status Phase 1**：仅 `active` | `archived`。不做 Craft 五态（todo / in_progress / needs_review / done / cancelled）；旗标 / needs_attention 可后续再加。

其它沿用 chat-first 拍板：持久化仍项目级 `.loom/chat/`；升格 stub 不自动开跑。

## 当前权限与迁移

当前 `ask` 是每回合发送前授权可写 CLI；逐工具审批尚未实现，须具备验收通过的双向协议后作为独立能力交付。参数、旧值迁移和实现限制统一维护在 [Chat 契约](../architecture/chat-contracts.md)，本历史 PRD 不再复制第二份权威映射表。

## ChatSession ≠ Task（重申）

| | ChatSession | Task |
|---|---|---|
| 目的 | 本机 Agent 多轮对话 | 四阶段开发闭环 |
| 状态 | `active` \| `archived` + turn 状态 | `task_state` |
| 持久化 | `.loom/chat/` | task store |
| Agent | `prepare_invocation`，intent=chat | 按 stage |
| 关系 | 可升格草稿 Task | 不反向吞并 Chat 历史（本期） |

## 继续实施

执行入口为 [本机 Chat 重构计划](../plans/2026-09-17/10:15-local-agent-chat-rebuild.md)。以上 Phase 1 故事是历史设计；当前实现与未完成目标以 Chat 契约的状态表区分。
