# Loom Chat-first 重构 — Plan

- **Date**: 2026-09-14
- **Author**: Droplet
- **Status**: completed
- **Progress**: M0–M5 完成（chat-first 主路径落地）
- **Scope**: 把 Loom 从 Board/Planning 主导的任务工作流，重构为可调用本地 Codex CLI、Claude Code CLI 与 Grok CLI 的 Grok-Bot 式 chat-first UX，同时保留现有 adapter / runner / PTY / stream 能力；Board 与 Planning 降级为高级入口，不在本轮重写任务状态机。

## 目标

把 Loom 的默认体验收敛到「打开项目 → 选 Agent → 自由对话 → 需要时再升格为任务」的 chat-first 路径，对齐 Grok Bot / Cursor Agent 一类本机助手交互：

- 一等公民的自由对话会话（ChatSession），可流式调用本地 CLI Agent。
- 内建或等价一等支持 Codex CLI、Claude Code CLI、Grok CLI。
- 复用现有 `agent_adapter`、`command_runner`、`pty`、`stream`、Settings 诊断，而不是另起一套进程编排。
- 保留 Board / Planning / Testing 作为 advanced 工作流，而不是删除。

## 非目标

- 不在 phase 1 重写 `task_state` / 任务状态机与四阶段闭环语义。
- 不做云同步、插件市场、团队账号或远程协作。
- 不在本轮移除 Board / Planning；只将其从默认主路径降级为高级入口。
- Beta 签名 / 公证 / 公开分发不在本计划范围（另见 `docs/plans/2026-08-07/10:00-beta-release-readiness.md`）。
- 不依赖 Cursor Cloud Agents：当前计划场景下 Cloud Agents 不可用，一律走本机 CLI 实现路径。

## 成功标准

- 默认启动路由进入 Chat（会话列表 + 消息流 + composer），而不是 Board。
- 用户可在同一 Chat UI 中切换 Codex / Claude Code / Grok（或经验证提升的 custom CLI 路径）并发起至少一轮流式对话。
- Chat 调用路径复用 `agent_adapter` → `command_runner`（及既有 stream / 诊断），不新增第二套进程启动层。
- Board / Planning / Testing 仍可通过 advanced 入口打开，既有任务状态机行为不被破坏（回归：`pnpm check` 与相关单元测试通过）。
- 存在可点击的「将当前 chat 升格为 task」stub（可只落契约与 UI，不要求完整状态机迁移）。
- 文档与代码中明确标注：ChatSession ≠ Task；chat 轮次不隐式推进 task stage。

## 当前状态

基于仓库现状（非愿景）：

- **Agent 适配**：`src-tauri/src/agent_adapter.rs` 已有 `CodexAdapter`、`ClaudeAdapter`、`CustomCliAdapter`；按 `adapter_type` 分发；支持 stage 权限、JSON / stream-json 输出模式与有限 resume。
- **运行与流**：已有 `command_runner`、`pty`、`agents/stream`、Settings / `agent_diagnostics` 诊断闭环。
- **UI IA**：`App.tsx` 主视图为 `board` ↔ `WorkspaceSplit`（任务详情含 Planning / Session / Testing 等）；无独立的一等公民自由对话 Chat 路由或会话列表。
- **现有「chat」片段**：Planning / Testing 内嵌对话或反馈 composer，绑定任务阶段与 feedback / repair run，不是自由 ChatSession。
- **Grok**：无内建 Grok adapter；仅能经 custom CLI 占位符路径间接尝试，契约未验证。
- **Cloud Agents**：本计划场景不可用 → 实施一律本地 CLI。

## 显式复用

| 现有能力 | 复用方式 |
|---|---|
| `agent_adapter` | Chat 调用走同一 `prepare`；新增 chat 阶段或映射到只读/可写策略，避免分叉参数拼装 |
| `command_runner` | Chat turn 的进程生命周期、超时、退出码、证据路径与现有 run 记录对齐 |
| `pty` | 仅在需要真终端交互时复用；默认 chat 优先结构化 stream，不强制 PTY |
| `agents/stream` | Codex JSON / Claude stream-json 事件解析与前端增量渲染 |
| Settings / `agent_diagnostics` | Agent picker 与可用性探测复用诊断结果；Grok 探测挂同一诊断面板 |

## 里程碑

### M0 — PRD + IA + ChatSession 模型 + Tauri 命令草图

**Outcome**: 产品边界、信息架构与后端契约写清，后续 UI / 接线有单一真相源。

| # | Task | Files / Output | Verification |
|---|---|---|---|
| 0.1 | 写 Chat-first PRD：用户故事、默认路径、与 Task 的边界 | `docs/` 下 PRD 切片或本计划附录定稿 | 明确 ChatSession ≠ Task；列出非目标 |
| 0.2 | 画 IA：侧栏会话列表、主栏消息、composer、Agent picker、advanced 入口 | IA 草图 / 路由表 | 默认路由为 Chat；Board/Planning 标为 advanced |
| 0.3 | 定义 `ChatSession` / `ChatMessage` 模型与持久化位置草案 | `src-tauri` 模型草图 + 前端类型草案 | 字段含 sessionId、agentId、projectPath、messages、createdAt；不绑定 task stage |
| 0.4 | 草拟 Tauri commands：`chat_list_sessions`、`chat_create`、`chat_send`、`chat_abort`、stream 事件名 | `contracts` / API 草案 | 命令签名可被前后端评审；标明复用 adapter/runner |

### M1 — Chat UI shell

**Outcome**: 可导航的 Chat 壳层可跑，尚可不接真实 Agent。

| # | Task | Files / Output | Verification |
|---|---|---|---|
| 1.1 | 新增 Chat 路由与懒加载页面壳 | `src/App.tsx`、`src/features/chat/` 或等价 | 默认 `currentView` 可进 Chat；Esc / 导航不破坏 Board |
| 1.2 | 会话列表 + 空态 + 新建会话 | Chat sidebar 组件 | 本地 mock 或空仓储可渲染列表 |
| 1.3 | 消息列表 + composer + Agent picker UI | Chat main + composer | picker 读取已保存 Agent 配置；无 Agent 时有引导 |
| 1.4 | 视觉与无障碍：加载/失败/空消息态 | 样式与测试 | 相关前端单测或组件测通过；`pnpm check` |

### M2 — 接通 Codex 端到端流式 Chat

**Outcome**: 选定 Codex Agent 后，用户可在 Chat 中发送消息并看到流式回复。

| # | Task | Files / Output | Verification |
|---|---|---|---|
| 2.1 | 实现 chat 后端：create/list/send，经 `agent_adapter` + `command_runner` 启动 Codex | Rust chat 模块 + commands | 单元测覆盖 prepare 与权限门禁 |
| 2.2 | 订阅 stream 事件并写入 ChatMessage | stream bridge + 前端 hook | 一次真实或 fixture 流式回合可增量渲染 |
| 2.3 | abort / 超时 / 失败摘要进会话 | runner 错误映射 | 失败气泡含退出码或超时原因；可再发下一条 |
| 2.4 | 门禁 | CI 本地 | `pnpm check`；相关 Rust/前端单测通过 |

### M3 — Claude Code 对等 + 支持处的 session resume

**Outcome**: Claude Code 在 Chat 中达到与 Codex 对等的流式体验；在适配器已支持处提供 resume。

| # | Task | Files / Output | Verification |
|---|---|---|---|
| 3.1 | Chat 路径注册 Claude adapter 与 stream-json 消费 | adapter + stream | 与 Codex 同一 Chat UI 可切换 Claude |
| 3.2 | 记录可安全解析的 resume 句柄（沿用现有 resume 契约） | ChatSession 元数据 | 仅接受内建可解析 resume；拒绝任意 shell 字符串 |
| 3.3 | UI：继续会话 / 新会话入口 | Chat chrome | resume 失败有明确错误，不污染 task 状态机 |
| 3.4 | 门禁 | CI 本地 | `pnpm check`；Claude/Codex adapter 相关单测通过 |

### M4 — Grok CLI 一等 adapter（或经验证的 custom CLI 提升）

**Outcome**: Grok 成为可选一等 Agent，或 custom CLI 路径经 spike 验证后提升并文档化。

| # | Task | Files / Output | Verification |
|---|---|---|---|
| 4.1 | Spike：探测本机 Grok CLI 的 argv、auth、输出模式、是否支持 resume | `docs/` spike 笔记 | 产出「可一等适配」或「保持 custom + 模板」决策 |
| 4.2 | 若可一等：实现 `GrokAdapter` + 诊断 + 单测 | `agent_adapter.rs`、diagnostics | 阶段权限与参数不经 shell；单测覆盖 |
| 4.3 | 若不可：提供官方推荐的 custom CLI 模板并在 Settings 引导 | Settings copy + docs | 用户按模板可完成一轮 Chat；限制写清 |
| 4.4 | Chat Agent picker 暴露 Grok（或模板 Agent） | UI + 配置 | 无二进制时诊断说明可读；`pnpm check` |

### M5 — 默认路由到 Chat；Board/Planning 降为 advanced；升格 stub

**Outcome**: 产品默认路径是 Chat；旧工作流仍在；升格为任务仅 stub。

| # | Task | Files / Output | Verification |
|---|---|---|---|
| 5.1 | 默认 `currentView` / 侧栏主入口改为 Chat | App shell、导航文案 | 冷启动进入 Chat |
| 5.2 | Board / Planning 移入 advanced 或次级入口，文案标明 | Sidebar / IA | 旧路径仍可打开既有任务 |
| 5.3 | 「将 chat 升格为 task」按钮 + 契约 stub（可创建草稿 task，不自动跑阶段机） | UI + command stub | 点击有明确 no-op 或草稿创建；不推进 stage |
| 5.4 | 回归与文档收口 | dogfood 笔记 / 本计划 Progress | `pnpm check`；冒烟：Chat 一轮 + 打开 Board 不回归 |

## 风险

| Risk | Impact | Mitigation |
|---|---|---|
| 把 Chat 回合与 Task stage 混为一谈 | 高 | 模型与命令分离；文档与代码注释强制 ChatSession ≠ Task；M5 升格仅 stub |
| Chat 权限模型弱于现有 stage 门禁 | 高 | 复用 adapter 阶段权限或显式 `chat` 只读默认 + 用户确认可写 |
| Grok CLI 契约未知 | 中 | M4 先 spike；无法一等则走 custom CLI 模板，不阻塞 M0–M3 |
| 双 UI（Chat + Board）造成导航混乱 | 中 | M0 IA 定默认路径；M5 降级 advanced 并统一文案 |
| Stream / 多 Agent 并发拖垮 UX | 中 | 单会话单 in-flight turn；abort 优先；复用现有超时 |

## 验证门禁

每个里程碑结束至少：

- `pnpm check`（或仓库现行等价总检）通过。
- 本里程碑触及的 Rust / 前端单元测试通过。
- 不引入第二套进程启动路径（code review 检查 chat 是否仍经 `agent_adapter` + `command_runner`）。

## 待确认问题

- Chat 默认权限：全局只读，还是按 Agent 配置继承 implementation 写权限？
- ChatSession 持久化放在项目 `.loom/` 还是用户级 app data？
- Grok CLI 以用户本机已安装二进制为准，还是需要 Loom 文档锁定最低版本？
- 「升格为 task」stub 在 M5 只建草稿，还是允许带上最后一条 prompt 作为任务描述？

## 下一步建议

先做 M0：定 PRD/IA、`ChatSession` 模型与 Tauri 命令草图，并在评审中冻结「不重写任务状态机、不依赖 Cloud Agents」。M0 通过后再开 M1 UI shell；Codex 端到端（M2）应是第一条可 dogfood 的垂直切片。


## M0 交付物（2026-09-14）

- PRD：`docs/guides/chat-first-prd.md`
- IA：`docs/guides/chat-first-ia.md`
- 契约：`docs/architecture/chat-contracts.md`
- 前端类型：`src/domain/chat.ts`（已从 `src/domain/index.ts` 导出）
- 待确认问题已在 PRD 拍板；Tauri commands 延后到 M2 再写入 `contracts.rs` manifest，避免空实现撑破契约测试。


## M1 交付物（2026-09-14）

- `src/features/chat/`：ChatPage 壳（会话列表、消息、composer、Agent picker、允许写入开关）
- `AppView` 增加 `chat`；侧栏项目下增加「对话」入口
- M1 使用 `mockStore` 模拟回复，不调用本机 CLI（M2 再接）
- 单测：`tests/unit/chatMockStore.test.cjs`


## M2 交付物（2026-09-14）

- Rust `src-tauri/src/chat.rs`：`chat_list/create/get/set_agent/send/abort`，持久化 `.loom/chat/`，经 `agent_adapter::prepare_invocation` 启动本机 CLI，事件 `loom://chat-stream` / `loom://chat-turn-finished`
- **不**走 `start_command_run`（避免绑定 Task）
- 前端 ChatPage 在 Tauri 下接真实命令；浏览器预览仍用 mock
- contracts / `TAURI_COMMANDS` / `TAURI_EVENTS` 已登记


## M3 交付物（2026-09-14）

- `chat_send` 回合结束后用 `session_capture` 解析 Codex/Claude session id，写入 `ChatSession.resumeCommand`
- 下一轮 `prepare_invocation` 自动带 resume（仅内建可解析命令）
- `chat_clear_resume` + UI「开新 CLI 会话」；续聊失败保留旧 resume，错误进气泡


## M4 交付物（2026-09-14）

- 一等 `grok_cli` adapter（`-p` + `streaming-json` + permission-mode plan/acceptEdits）
- 默认 Agent `agent-grok`；`session_capture` 支持 `grok --resume`
- Spike 笔记：`docs/guides/grok-cli-spike.md`

## M5 交付物（2026-09-14）

- 默认 `currentView = chat`；选项目后进 Chat
- 侧栏「任务看板（高级）」
- `chat_promote_to_task` stub：用最后一条用户消息创建草稿 Task，不推进状态机


## Follow-up（2026-09-15）

- `chat_promote_to_task` 写入含会话摘要的 `raw_requirement`，返回完整 Task；Chat UI 升格后 `tasks/upserted` + `tasks/selected` 跳任务详情（仍不自动开跑状态机）。
