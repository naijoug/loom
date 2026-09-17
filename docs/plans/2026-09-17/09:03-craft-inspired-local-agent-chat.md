# Loom Craft 启发的本机 Agent Chat — Plan（Phase 1）

- **Date**: 2026-09-17
- **Author**: Droplet
- **Status**: draft
- **Progress**: 未开工（仅计划）
- **Scope**: 在已完成的 chat-first（`docs/plans/2026-09-14/20:06-chat-first-refactor.md` M0–M5）之上，做**彻底重构的 Phase 1**：把产品主表面做成 Craft Agents 风格的**本机 Agent Chat**（会话收件箱 + 转录 + composer + Agent / 权限档位），端到端可 dogfood 调用本机已接线 Agent；**不**在本阶段重写 Task 状态机，不引入云同步 / 市场 / Electron 服务端架构。

参考：

- Craft Agents OSS：`https://github.com/craft-ai-agents/craft-agents-oss`（本地可读 `.ref/repos/craft-agents-oss`，见 `docs/ref.md`）
- 既有 chat-first 交付：`src-tauri/src/chat.rs`、`src/features/chat/`、`src/domain/chat.ts`、`docs/guides/chat-first-*.md`、`docs/architecture/chat-contracts.md`

## 目标

把 Loom 的默认体验从「能聊的 chat-first 壳」推进到「可日常 dogfood 的本机 Agent Chat 产品面」，对齐 Craft Agents 的信息架构与关键交互模式，同时坚持 Loom 的 Tauri + Rust + 本机 CLI adapter 栈：

- **本机 Agent 端到端**：同一 Chat UI 可切换 Codex / Claude Code / Grok（已接线者），完成流式回合、中止、续聊 / 开新 CLI 会话、失败可读。
- **Craft 启发的 IA**：会话列表/收件箱、主转录区、composer、Agent picker、权限档位（Explore / Ask / Auto 语义）、可选 Sources/工具可见性面板；视觉走 Loom `--loom-*` 令牌，**不**复制 Craft 专有资产。
- **显式复用**既有 `agent_adapter` / stream 解析 / chat 持久化 / Settings 诊断；进程编排以 wrap 或对齐 `command_runner` / `ProcessSupervisor` 为目标（见「显式复用」），不另起第二套产品语义。
- Board / Planning / Testing 仍作 advanced；升格为 Task 保持 stub / 草稿，不自动跑阶段机。

## 非目标

- 不在 Phase 1 重写 `task_state` / 四阶段闭环 / Board 看板语义。
- 不做云同步、会话分享、插件/Agent 市场、团队账号、远程 headless server（Craft 的 `packages/server` 薄客户端模式）。
- 不删除 Board / Planning；不迁移到 Electron/Bun。
- 不完整复刻 Craft Sources（MCP/REST/OAuth 全家桶）、Automations、Background Tasks 产品化、Multi-file Diff 窗口——仅留扩展点或最小可见性。
- Beta 公证 / 公开分发不在本计划（另见 beta-release-readiness）。
- 不依赖 Cursor Cloud Agents：一律本机 Mac clone + 本机 CLI。

## 成功标准

- 冷启动默认仍进 Chat；Chat IA 具备：左侧会话收件箱（可按状态/旗标过滤的最小集）、中央 transcript、底部 composer、顶部 Agent + 权限档位。
- 至少 **一条垂直切片** 用真实本机 Agent（优先 Codex，其次 Claude / Grok）完成：新建会话 → 流式回复（含工具/命令事件的最小可视化）→ Abort → 再发一条 → 续聊或「开新 CLI 会话」→ 重启应用后 transcript 仍在。
- 权限档位在 UI 与 adapter 映射上可解释：至少三档（只读 / 询问可写 / 自动可写），并落到现有 `prepare_invocation` 的 stage / permission-mode，而不是仅前端 checkbox。
- Chat 路径仍经 `agent_adapter::prepare_invocation`；进程生命周期可审计（abort、超时、退出码进气泡）；**不**把 Chat turn 绑进 Task stage。
- `pnpm check` 与触及的 Rust/前端单测通过；Board 打开既有任务不回归。
- 文档标明：本计划是 thorough refactor 的 Phase 1；后续 Phase 才考虑 Sources/MCP 深集成、后台任务、更完整权限规则引擎。

## 当前状态

基于仓库现状（承接 2026-09-14 chat-first **已完成** M0–M5，非绿野）：

| 能力 | 现状 | 相对 Craft 的缺口 |
|---|---|---|
| ChatSession 模型与 `.loom/chat/` 持久化 | 有；summary + messages + resume | 无 inbox 工作流状态 / flag / archive；标题仅截断首条消息 |
| Tauri chat commands | `list/create/get/set_agent/send/abort/clear_resume/promote` | 缺 rename、status、flag、权限档位持久化 API 完善 |
| 流式 UX | `loom://chat-stream` 文本 delta + 气泡 | 无 Turn 级工具卡 / 命令执行行 / 差异摘要（Craft `TurnCard` / `InlineExecution`） |
| 权限 | `read_only` \| `read_write` checkbox → Planning / Debugging stage | 非 Craft 三档 Explore/Ask/Auto；Ask 无审批 UI；Grok 已有 plan/acceptEdits 但 Chat UI 未暴露 |
| Agent 切换 | `<select>` picker | 无诊断状态内联（不可用原因）、无快捷键循环 |
| 进程 | `chat.rs` 内 `tokio::process::Command` 直启（刻意不绑 Task） | 未走 `command_runner` / `ProcessSupervisor`；abort/超时/证据路径与任务路径不完全对齐 |
| 会话列表 | 简单列表 + 新建 | 无 Inbox/Archive、Todo→Done 状态、旗标、未读 |
| Sources / MCP | 无 | Craft 一等 Sources；Phase 1 仅预留面板空态或「项目 cwd 即上下文」说明 |
| 设置诊断 | Agent Settings + `agent_diagnostics` | Chat 内缺少「本机会话健康」入口（二进制/登录/最近失败） |
| Board | advanced 入口仍在 | 保持；不删 |

Craft 侧已阅读要点（README + `docs/cli.md` + shared session/permission + Electron ChatPage / UI chat 组件）：

- **Session inbox**：status 工作流（todo / in_progress / needs_review / done / cancelled）、flag、archive、AI/手动命名、磁盘持久化。
- **Permission modes**：`safe`（Explore）/ `ask`（Ask to Edit）/ `allow-all`（Auto）；Shift+Tab 循环。
- **Sources**：MCP（stdio/http）、REST、本地文件系统；工具名前缀隔离。
- **Chat 表面**：SessionList + ChatDisplay + composer；Turn 级工具/执行可视化；后台任务条。
- **架构差异**：Craft = Electron + Bun server-core；Loom = Tauri + Rust。只借 IA/数据边界与权限产品语义，不搬运行时。

## 显式复用

| 现有能力 | Phase 1 策略 |
|---|---|
| `agent_adapter`（Codex / Claude / Grok / custom） | **复用**：Chat 一律 `prepare_invocation`；扩展权限档位 → stage / CLI permission-mode 映射表 |
| `agents/stream`（codex_json / claude_stream_json / grok） | **复用并增强**：除文本 delta 外，解析 tool/command 事件供 UI 卡片；不重写 parser 主干 |
| `session_capture` | **复用**：resume 句柄写入 `ChatSession` |
| `.loom/chat/` 持久化 + index | **扩展字段**（status/flag/permission 三档）；保持原子写；必要时 schemaVersion bump |
| Settings / `agent_diagnostics` | **复用**：Agent picker 与 Chat 诊断抽屉读同一结果 |
| `command_runner` / `ProcessSupervisor` | **对齐 wrap**：Chat turn 优先挂到 supervisor（intent=`chat`，无 taskId），以统一 abort/超时/证据；若短期成本过高，M2 先保留直启但抽出共享 spawn/abort 辅助，避免第三套逻辑 |
| `pty` | **不默认**：Chat 仍优先结构化 stream；PTY 仅 advanced |
| Board / `task_state` | **不动**；升格 stub 保持 |
| Craft 代码 | **只读参考** IA/权限/Turn 可视化；禁止拷贝专有资源与 Electron 壳 |

**替换 vs 包装：**

- **包装/演进**：`ChatPage` 壳 → 拆成 Inbox / Transcript / Composer / ModeSwitcher；`ChatPermissionMode` 二档 → 三档；stream 事件载荷加 tool 片段。
- **替换（产品语义）**：「允许写入」checkbox → Craft 式权限档位控件；扁平会话列表 → 带状态过滤的收件箱。
- **不替换**：adapter 体系、Task 状态机、项目 `.loom/` 任务布局。

## Craft → Loom 映射（Phase 1）

| Craft | Loom Phase 1 |
|---|---|
| Workspace | 已打开的 Project（`projectPath`） |
| Session + Inbox status | `ChatSession` + `inboxStatus`（最小：active / needs_attention / done；可映射 todo/in_progress/done） |
| Flag / Archive | `flagged` bool；archive = `inboxStatus=done` 或 `archivedAt`（二选一，M0 定稿） |
| Permission `safe/ask/allow-all` | `explore` / `ask` / `auto`；映射 adapter：explore→只读 stage；ask→可写但需 Loom 确认门（最小：发送前确认或回合内拦截写工具——Open Question）；auto→debugging/acceptEdits |
| Sources 面板 | 可选右侧「上下文」：先展示项目路径 + 当前 Agent 诊断；MCP 接入列为 Phase 2+ |
| TurnCard / tool viz | `ChatMessage` 增加 `parts[]`（text / tool / command）或并行 `ChatTurnEvent`；UI 用 Loom 卡片样式 |
| LLM Connection picker | Agent picker（本机 CLI 配置） |
| Background tasks | 非目标；最多显示当前 turn 的 spinner/abort |
| Headless server | 非目标 |

## 里程碑

### M0 — 差距冻结 + IA/契约修订 + 权限映射表

**Outcome**: 在既有 chat-first 文档上修订 Phase 1 边界；前后端契约可评审；不写大块 UI。

| # | Task | Files / Output | Verification |
|---|---|---|---|
| 0.1 | 修订 PRD/IA：Inbox + 三档权限 + Turn 可视化；标明「build on M0–M5」 | `docs/guides/chat-first-prd.md`、`chat-first-ia.md` 增补或 `docs/guides/craft-chat-phase1.md` | 中文 IA 含线框级区域：inbox / transcript / composer / agent / mode |
| 0.2 | 扩展领域类型：`inboxStatus`、`flagged`、`ChatPermissionMode` 三档、消息 `parts` 草案 | `src/domain/chat.ts` + `docs/architecture/chat-contracts.md` | ChatSession ≠ Task 仍成立；schemaVersion 策略写清 |
| 0.3 | 权限映射表：Loom 档位 → `AgentStage` + 各 adapter CLI flag（Codex sandbox / Claude permission-mode / Grok plan\|acceptEdits） | 契约附录或 `agent-adapter.md` 小节 | 表可被 M2 单测引用 |
| 0.4 | 决定 Chat 进程：supervisor wrap vs 共享 spawn 辅助（写进本计划 Progress） | 短决策笔记 | 选定一条；禁止再增加第三路径 |

### M1 — Craft 风格 Chat IA 壳（可仍接 mock / 现有后端）

**Outcome**: UI 信息架构接近 Craft，默认仍可用现有 send 路径发纯文本。

| # | Task | Files / Output | Verification |
|---|---|---|---|
| 1.1 | 拆分 `ChatPage`：`ChatInbox`、`ChatTranscript`、`ChatComposer`、`ChatSessionHeader` | `src/features/chat/*` | 组件可独立渲染；`pnpm check` |
| 1.2 | Inbox：列表项显示标题/预览/Agent/时间；过滤 active vs done；flag 切换；重命名 | UI + `chat_update_meta`（或扩展现有 command） | 刷新/重进应用状态保留 |
| 1.3 | Header：Agent picker + 三档权限控件（可 Shift+Tab 循环，若快捷键框架允许） | Header 组件 | 档位变更写入 session；文案中文：探索 / 询问编辑 / 自动 |
| 1.4 | Transcript 空态与加载/失败态按 Loom 令牌重做；去掉「仅 checkbox」旧交互 | CSS / a11y | 视觉与键盘可达；单测或组件测 |

### M2 — 垂直切片：真实本机 Agent + 流式 Turn 可视化 + 进程对齐

**Outcome**: 选定一个已接线 Agent（**优先 Codex**）完成可 dogfood 的端到端 Chat。

| # | Task | Files / Output | Verification |
|---|---|---|---|
| 2.1 | 按 M0 决策把 `run_chat_turn` 挂到 `ProcessSupervisor` 或共享 spawn/abort | `chat.rs` + 必要 runner 辅助 | abort 能杀掉进程组；超时进 error 气泡 |
| 2.2 | Stream：解析并 emit tool/command 片段；前端 Turn 卡片渲染 | `agents/stream` 消费者 + Chat UI | 一次真实 Codex（或 fixture）回合可见文本 + 至少一种工具/命令行 |
| 2.3 | 三档权限真正传入 `prepare_invocation`；去掉仅前端的二档 checkbox | `chat.rs` `permission_to_stage` 演进 | 单测覆盖三档映射；explore 不走 workspace-write |
| 2.4 | Dogfood 笔记：本机 Codex（或 Claude/Grok）一轮完整路径 | `docs/` 短笔记或本计划 Progress | 人工勾选成功标准条目 |

### M3 — 多 Agent 对等体验 + 诊断内联

**Outcome**: Claude / Grok 与 Codex 在同一 IA 下体验对等；不可用 Agent 有可读诊断。

| # | Task | Files / Output | Verification |
|---|---|---|---|
| 3.1 | Claude stream-json 工具事件进同一 `parts` 模型 | stream + UI | 切换 Claude 可见对等卡片 |
| 3.2 | Grok：UI 档位与 `plan` / `acceptEdits` 对齐（见 grok spike） | adapter + Chat header | 无二进制时 picker 显示诊断摘要 |
| 3.3 | Chat 内「Agent 状态」抽屉/弹出：复用 `agent_diagnostics` | UI + invoke | 不打开 Settings 也能看到路径/版本/失败提示 |
| 3.4 | 门禁 | CI 本地 | `pnpm check`；相关单测 |

### M4 — Inbox 工作流打磨 + transcript 恢复韧性

**Outcome**: 会话管理接近「可当主工作台」；重启与失败不丢话。

| # | Task | Files / Output | Verification |
|---|---|---|---|
| 4.1 | Inbox：needs_attention（回合 error / 被 flag）自动归类；done/归档过滤 | chat index 字段 | 过滤切换正确 |
| 4.2 | 重启后：streaming 中断标记为 aborted/interrupted；保留 partial 文本 | `chat.rs` 启动对账或懒修复 | 杀进程再开，会话可读且可再发送 |
| 4.3 | 标题：手动重命名 + 可选「用首条消息」；避免无脑截断 | command + UI | 标题持久化 |
| 4.4 | 续聊 / 开新 CLI 会话入口保留并放入 Session 菜单（对齐 Craft SessionMenu 密度，不抄样式） | Header 菜单 | resume 失败不丢旧 handle |

### M5 — 可选 Sources 空态 + 文档收口 + dogfood 清单

**Outcome**: Phase 1 可宣布完成；Phase 2 入口清晰。

| # | Task | Files / Output | Verification |
|---|---|---|---|
| 5.1 | 可选右侧「上下文」面板：项目路径、`.loom`、当前 permission、Agent 诊断链接；MCP 标注「后续阶段」 | UI 空态 | 不实现 MCP 连接也能理解边界 |
| 5.2 | 更新 `docs/architecture.md` / chat 契约 / 本计划 Progress=completed 条件 | docs | 与代码一致 |
| 5.3 | 回归：Chat dogfood 清单 + 打开 Board 既有任务 | 手动/冒烟 | `pnpm check`；无 Task 状态机回归 |
| 5.4 | 升格 stub 入口保留在 Session 菜单；文案强调不自动开跑 | UI | 点击仍只建草稿 Task |

## 风险

| Risk | Impact | Mitigation |
|---|---|---|
| 把 Craft 整仓架构（server-core / Sources）搬进 Phase 1 | 高 | 非目标写死；只借 IA 与权限语义 |
| `ask` 档无真实审批则名不副实 | 高 | M0 选定最小 Ask：发送前确认 **或** 映射为只读+显式升档；完整 tool-gate 可进 Phase 2 |
| Chat 直启进程与 Task runner 双轨 | 中 | M0 决策 + M2 必须收敛到 supervisor 或共享辅助 |
| Turn 可视化拖垮性能 / 事件噪声 | 中 | 先支持子集事件；卡片虚拟列表可后续 |
| 权限三档与既有 stage 能力矩阵冲突 | 中 | 单一映射表 + adapter 单测；禁止 UI 私自拼 CLI flag |
| Inbox 状态与 Task 看板概念混淆 | 中 | 命名用「会话收件箱」；文档强调 ≠ Board columns |
| 过度承诺 MCP Sources | 中 | M5 仅空态；Open Question 交给用户 |

## 验证门禁

每个里程碑结束至少：

- `pnpm check`（或仓库现行总检）通过。
- 本里程碑触及的 Rust / 前端单元测试通过。
- Code review：Chat 仍经 `agent_adapter`；无第二套 argv 拼装；ChatSession 不推进 Task stage。
- M2+：至少一次本机真实 Agent dogfood（或 CI fixture + 本地一次人工）。

## 待确认问题

1. **Ask 档最小语义**：Phase 1 用「发送前确认可写」还是做到「工具/写文件级审批」（接近 Craft）？
2. **Inbox 状态集合**：精简三态（active / needs_attention / done）是否足够，还是要对齐 Craft 五态？
3. **垂直切片首选 Agent**：本机 dogfood 默认 Codex，若本机无 Codex 是否接受 Claude / Grok 作为 M2 验收 Agent？
4. **Chat 进程是否必须在 M2 接入 `ProcessSupervisor`**，还是允许先共享 spawn/abort 辅助、Supervisor 放到 Phase 1.1？
5. **Sources / MCP**：是否同意 Phase 1 只做上下文空态，MCP stdio 明确放到下一阶段？
6. **会话存储位置**：继续项目级 `.loom/chat/`，还是部分元数据升到用户级 app data（跨项目 Inbox）？

## 下一步建议

计划评审通过后从 **M0** 开工：冻结 IA/契约与权限映射、选定 Chat 进程策略，再进入 M1 壳层；**M2 Codex（或指定本机 Agent）垂直切片** 作为第一条可 dogfood 的完成定义。不要并行铺 MCP / 后台任务 / Task 状态机改造。
