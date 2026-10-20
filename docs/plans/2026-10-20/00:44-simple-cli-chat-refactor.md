# 简化为 Claude Code 式本机 CLI Chat 的重构计划

- 状态：工程与资料收尾完成，待用户桌面验收（2026-10-20）。R0–R4 代码已在分支 `codex/simple-cli-chat` 完成（`7e1dfcb`、`3dd9a9f`，未合入 `main`），工程门禁通过；R5 文档已提交于 `0acbb10` 并继续补齐收尾记录；真实 CLI 进程验证有分项证据，**桌面验收由用户明确接手，未执行项仍保持 not-tested**。证据见 [验收记录](../../dogfood/2026-10-20-simple-chat-acceptance.md)，明细见 §10。执行拆解见 [实施计划](01:01-simple-chat-implementation.md)（R0→P0，R1→P1，R2→P2/P3，R3→P4–P7，R4→P8，R5→P9）。
- 代码基线：`main` @ `bed095c`（2026-10-18），工作区仅 `docs/reviews/` 未跟踪。
- 关联：本计划落地后取代 [Craft 完整重建计划](../2026-09-30/13:17-craft-parity-rebuild.md) 作为当前实施入口；与 [2026-09-14 chat-first 重构](../2026-09-14/20:06-chat-first-refactor.md) 方向一致，复用 [2026-09-17 Chat 重建](../2026-09-17/10:15-local-agent-chat-rebuild.md) 已落地的仓储与运行链。

## 1. 目标

把 Loom 收敛成一个简单的桌面 Chat：会话按项目分组管理；选好项目后，选择「调研」或「Code」模式和本机已安装的 Agent CLI（Claude Code、Codex、OpenCode 等），像在 Claude Code 里一样多轮对话，能看到流式回复与工具调用，能停止、续聊，重启后历史仍在。

### 非目标（本计划内不做）

- Craft v0.14.0 全功能复刻（Sources/MCP 管理、Skills、Browser、Pages、DAG、自动化、远程、渠道、分享）。
- 旧四阶段 Task（Planning → Implement → Review → Testing）、Board、多 Agent 讨论与互评。
- Loom 自建的逐工具审批协议。权限仍由各 CLI 自身的模式控制（见 §5 D4）。
- 内置模型 API / 凭据管理：登录与模型选择交给 CLI 本身。
- 同一会话中途切换到另一个 Agent 并共享原生上下文。

### 成功标准

1. 默认构建打开即为新 Chat 界面；不再挂载 Task/Board/Planning 代码，导航中没有入口。
2. 侧栏按项目分组列出会话（项目可折叠，可以在任一项目下新建会话）；每个会话显示模式（调研 / Code）和 Agent，模式在会话内可以切换。
3. 启动时自动探测本机 CLI，显示「已安装 / 版本 / 未安装」；探测不依赖写死的用户目录。
4. Claude Code、Codex 各自通过真实桌面验收：流式回复、连续三轮追问、工具调用卡片、停止后再发、重启后续聊；调研模式下确认没有任何文件写入。
5. OpenCode 至少通过流式回复 + 多轮 + 停止；续聊按 R0 实测结果标记支持或不支持，不臆测。
6. Grok 与自定义 CLI 保持现有能力，不回退（未重测的标为 `not-tested`，不标通过）。
7. 前端 `pnpm test`、Rust `cargo test`（src-tauri）、`pnpm build`、`pnpm tauri build --no-bundle` 通过；删除旧代码后无死引用。
8. 已有 `<project>/.loom/chat/v2` 会话可直接读取；`.loom/` 下的 Task 数据不被修改或删除。
9. 界面默认英文，可在设置 › Appearance › Language 切换为简体中文，切换立即生效并在重启后保留；新界面不出现硬编码的中英文案。
10. 新会话页只选择模式（Research / Code）；Agent 在输入框左下角的选择器中选择，新会话和已有会话位置一致。

## 2. 当前状态（事实）

| 领域 | 现状 | 证据 |
| --- | --- | --- |
| 默认入口 | `App` → `ChatShell` + `ChatPage`；Task/Board/Workspace 视图仍可切换 | `src/App.tsx`、`src/state/model.ts:90` |
| 新内核 | 仅在 `VITE_LOOM_WORKBENCH=1` 时启用；走 Claude/Pi SDK worker，**不调用已安装 CLI** | `src/main.tsx`、`src/workbench/`、`crates/`、`packages/provider-workers` |
| Chat 后端 | v2 journal、幂等回执、补流、回合日志、导出、停止与正常退出已实现 | `src-tauri/src/chat/*`、`docs/architecture/chat-contracts.md` |
| Agent 适配 | Codex、Claude、Grok、自定义 CLI 四种 adapter；React 不拼参数 | `src-tauri/src/agent_adapter.rs:93-96,156` |
| 输出解析 | `plain` / `claude_stream_json` / `codex_json`；Grok 用 `streaming_json` | `src-tauri/src/agents/stream.rs:106-131` |
| Agent 探测 | 默认 codex/claude/grok/dummy；候选路径含写死的 `/Users/guojian/...` | `src-tauri/src/agents/config.rs:38-49,208-215` |
| OpenCode | **无**任何适配代码；本机已安装于 `~/.opencode/bin/opencode`，非交互参数尚未实测 | `grep` 零命中 |
| 本机 CLI（2026-10-20） | claude 2.1.304、codex-cli 0.163.0、grok 1.0.62、opencode（版本未取到） | 本轮 `--version` |
| 权限 | `explore` / `ask` / `auto`；`ask` 是每回合授权，不是逐工具审批；Chat 不走 `execution_policy` | `chat-contracts.md`「权限与完成边界」 |
| 体量 | `src-tauri/src` 约 3.1 万行，其中 `tasks.rs`、`agents.rs`、`orchestrator.rs`、`command_runner.rs` 等主要服务 Task；前端 Task 组件集中在 `src/components/{Board,TaskDetail,Planning,Workspace}`、`src/features/{tasks,planning,testing}` | `wc -l` |

假设（R0 验证）：Claude `-p --output-format stream-json` 和 Codex `exec --json` 的事件里包含足够的工具调用信息，可以渲染工具卡片；OpenCode 提供 `run` 非交互模式和结构化输出。

## 3. 目标形态

```text
┌──────────────────┬────────────────────────────────────────────┐
│ Projects    [add] │ loom /                                     │
│ ▾ loom      4 [+] │ Refactor chat entry    [Research|Code]  ⇩  │
│   ‹›重构 Chat 入口 ├────────────────────────────────────────────┤
│   ⌕ OpenCode 调研 │ 用户消息                                    │
│   ‹›修复 CI 缓存  │ 助手 Markdown 流式输出                      │
│ ▸ hoxi-web    2   │ ▸ Read src/app.ts        (可折叠工具卡片)   │
│ ▸ notes-site  1   │ ▸ Edit src/app.ts  +3 −1 (diff 预览)        │
│                   ├────────────────────────────────────────────┤
│ ⚙ Settings        │ > Message…                                 │
│                   │ [Claude Code ▾] [ ] Auto-run    Stop · Esc  │
│                   │ ● Running · Code · confirm · resume ✓       │
└──────────────────┴────────────────────────────────────────────┘
```

- **按项目组织**：侧栏是「项目 → 会话」两级树。项目来自已登记的最近项目（`list_recent_projects`），每个项目的会话来自该项目自己的 `.loom/chat/v2`。项目可以折叠，每个项目旁有「+」在该项目下新建会话；侧栏顶部可以添加项目目录。当前项目默认展开，其他项目只显示会话数，展开时再加载。
- **新会话流程**：项目 → 模式（调研 / Code）→ Agent → 首条消息。
- **两种模式**（会话级，顶栏随时可切换，输入框边框颜色和提示随模式变化）：

| 模式 | 用途 | 对应现有权限值 | Claude | Codex |
| --- | --- | --- | --- | --- |
| 调研 | 读代码、查资料、分析方案；不改文件 | `explore` | `--permission-mode plan` | `read-only` sandbox |
| Code（确认） | 改代码、跑命令 | `ask` | `acceptEdits` | `workspace-write` |
| Code + 自动执行 | 同上，CLI 不再停下确认 | `auto` | 按 R0 实测映射 | 按 R0 实测映射 |

  模式直接复用现有 `permissionMode` 字段，不改数据格式、不升级 schema：新增模式只是界面上的叫法，旧会话按原值显示。调研模式是否追加一段面向调研的提示词（要求给出结论和依据），在 R2 用真实对比决定，不默认加。
- 单列转录，风格接近 Claude Code：等宽工具卡片、退出码、diff 摘要，长输出折叠。
- Composer 支持少量斜杠命令：`/new`、`/agent <id>`、`/mode <research|code|auto>`、`/clear-resume`、`/export`。
- 设置页只保留：Agent 列表（探测结果、启用、命令路径覆盖、自定义 CLI）、主题、关于、更新。
- 后端链路不变：command → context/adapter → runtime → journal → UI 投影。

## 4. Agent 支持分级

| 等级 | Agent | 要求 |
| --- | --- | --- |
| T1 | Claude Code、Codex | 结构化流、工具卡片、原生续聊、三种模式映射，真实桌面验收 |
| T2 | OpenCode、Grok | 结构化或纯文本流、多轮、停止；续聊按实测 |
| T3 | 自定义 CLI | 纯文本流、多轮靠有界历史重放；不承诺工具卡片 |

能力以 adapter 声明的 `capabilities`（`structuredStream`、`toolEvents`、`nativeResume`、`modes`）呈现，UI 按能力降级，不按 Agent 名字写分支。

## 5. 关键决策

- **D1 以旧 Chat 运行链为基础，不以新内核为基础。** 用户要的是「调用已安装的 CLI」，这正是 `src-tauri/src/chat` + `agent_adapter.rs` 已在做的事；新内核依赖 SDK worker 与 Bun 资源，M2 尚未到出口。
- **D2 冻结 Craft 重建。** `crates/`、`packages/`、`src/workbench/`、`src-tauri/src/workbench/` 本计划内不再扩张。R5 只把它们移出默认构建与 CI 主门禁；是否物理删除，在 R5 结束时单独决定（§9 Q1）。
- **D3 Task 工作流退出产品。** R1 移除入口，R4 删除代码。磁盘上的 Task 数据原样保留，不做迁移。
- **D4 权限交给 CLI，界面上只呈现两种模式。** 底层保持现有 `explore` / `ask` / `auto` 映射（Claude：plan/acceptEdits 等；Codex：sandbox + approval never），界面上叫「调研」和「Code（可开自动执行）」，对应关系见 §3。UI 文案写明这是 CLI 侧模式，不是 Loom 的逐工具审批。
- **D6 会话按项目组织。** 存储位置不变（每个项目自己的 `.loom/chat`），侧栏按项目聚合；Loom 不做跨项目的会话移动。逐工具审批作为后续独立计划，候选做法是 Claude `--permission-prompt-tool`、Codex app-server 协议。
- **D7 英文优先，支持简体中文。** 界面文案集中在 `src/i18n/`，默认 `en`，可切换 `zh-CN`；语言保存在应用设置（`AppSettings.locale`，serde 默认 `en`，旧设置文件可直接读取）。Agent 回复语言由用户提示词决定，不受界面语言影响。
- **D8 Agent 在输入框中选择。** 新会话页只做模式选择；Agent 选择器固定在 Composer 左下角，新会话与已有会话共用同一组件。顶栏只保留模式切换和导出。
- **D5 Agent 不可中途替换上下文。** 换 Agent 会清除续聊句柄（现有行为），新回合使用有界历史重放。

## 6. 里程碑与任务

### R0 — CLI 能力实测（只读，无代码改动）

1. 对 claude / codex / opencode / grok 各跑一次非交互调用，用临时项目和无害提示词，保存原始 stdout 作为 fixture，存到 `src-tauri/tests/fixtures/cli-streams/<agent>-<version>.jsonl`（先脱敏）。
2. 确认每个 CLI 的：非交互参数、输出格式、工具调用事件字段、会话 ID 来源与续聊参数、权限/沙箱参数、停止信号行为。
3. OpenCode 重点确认 `opencode run` 是否支持 JSON 输出与 `--session` / `--continue`；如只有 `opencode serve` HTTP 接口，记录下来，并在 R2 二选一。

产物：`docs/agent-adapter.md` 新增「CLI 实测矩阵（2026-10-20）」小节，加上 fixture 文件。
验证：fixture 能被现有 `parse_cli_stdout` 解析，Claude/Codex 的测试先跑起来（这时 OpenCode 预期失败）。

### R1 — 收敛入口与导航（依赖：无）

1. `src/App.tsx`、`src/state/model.ts`：`AppView` 只保留 `chat` 和 `settings`；删除 `board` / `workspace` 分支和 `WorkflowShell` 懒加载。
2. `src/features/chat/ChatShell.tsx`、`src/components/Sidebar/Navigation.tsx`：去掉「高级工作流」入口和 Task 相关计数。
3. `src/main.tsx`：去掉 `VITE_LOOM_WORKBENCH` 与 `PlanningPreviewApp` 分支（文件在 R4/R5 处理）。
4. 新增全局 `ErrorBoundary`，并在 `ChatTranscript` 子树单独包一层（对应 10-19 评审 S6）。
5. `chat_promote_to_task` 在 UI 中隐藏；后端命令在 R4 删除。

验证：`pnpm test`、`pnpm build`；`scripts/debug.sh web` 打开，确认只有 Chat 和设置两个入口。

### R2 — Agent 探测与 OpenCode 适配器（依赖：R0）

1. `src-tauri/src/agents/config.rs`：删除写死的 `/Users/guojian/...` 候选路径，改为 `$PATH` 加通用候选（`~/.local/bin`、`~/.npm-global/bin`、`~/.opencode/bin`、`~/.grok/bin`、`/opt/homebrew/bin`、`/usr/local/bin`）。探测结果包含解析后的路径与版本；探测命令有超时，且不经过 shell。
2. 默认 Agent 增加 `agent-opencode`；`agent-dummy` 只在测试/开发构建中出现。
3. `src-tauri/src/agent_adapter.rs`：新增 `OpencodeAdapter` 与 `ADAPTER_OPENCODE`，按 R0 结果映射模式、cwd、续聊；为参数映射与 resume 解析补单测。
4. `src-tauri/src/agents/stream.rs`：如果 OpenCode 有专用 JSON，就新增对应 `CliOutputMode` 与解析器，并用 R0 fixture 做解析测试。
5. adapter 返回 `capabilities`；同步 `contracts/tauri-contract.json` 与 `src/domain/chat.ts`。

验证：`cargo test`（src-tauri，含 fixture 解析测试与契约测试）；设置页显示 4 个 CLI 的真实探测结果。

### R3 — Claude Code 式会话界面（依赖：R1；工具卡片依赖 R2 的 capabilities）

1. 拆分 `src/features/chat/ChatPage.tsx`（610 行）：拆成 `SessionSidebar`、`Transcript`、`ToolCallCard`、`Composer`、`StatusLine`；把状态与桥接留在 `useChatBridge` / `ChatProjection`。
2. `ToolCallCard`：按 message.parts 的 tool 片段渲染名称、参数摘要、退出码和输出折叠；Edit/Write 类显示 diff 摘要。只做纯文本渲染，不执行 HTML。
3. 助手正文按 Markdown 渲染。如需新增依赖，先评估体积，并保持代码块转义安全。
4. Composer：斜杠命令（§3）、Enter 发送 / Shift+Enter 换行、Esc 停止、IME 保护（沿用现有逻辑）、模式切换。
5. 顶栏的 Agent 选择器显示能力徽标（结构化、工具、续聊）；不可用的 Agent 置灰并显示原因。
6. 清理 `ChatPage.css`（26.6K）里不再使用的样式。

验证：组件单测（工具卡片、斜杠命令解析、Esc 停止、能力降级）；`scripts/debug.sh web` 用 mock 截图检查深浅两种主题。

### R4 — 删除 Task 工作流（依赖：R1–R3 合入）

1. 前端删除 `src/components/{Board,TaskDetail,Planning,Workspace,Header}`、`src/features/{tasks,planning,testing}`、`src/preview/`、`src/state/reducers/{tasks,commandTrace,planning,taskCollections,commands}.ts` 及相关 hooks/utils/测试（具体清单在实施时用依赖扫描生成）。
2. 后端删除 Task 命令及其独占模块：`tasks.rs`、`tasks/`、`agents/orchestrator.rs`、`implementation_review.rs`、`task_summary.rs`、`task_state.rs`、`task_repository.rs`、`plan_html.rs`、`loop`/`validation` 相关代码等；`command_runner.rs`、`pty.rs` 若只被 Task 使用，一并删除。共享模块（`process_supervisor`、`agent_adapter`、`agents/stream`、`storage`、`projects`、`diagnostics`、`app_update`）保留。
3. 更新 `contracts/tauri-contract.json` 和 `src-tauri/src/contracts.rs`；`lib.rs` 只注册保留的命令。
4. 删除清单单独提交，Review 时逐项核对「无引用」。

验证：`pnpm test`、`cargo test`、`pnpm build`、`pnpm tauri build --no-bundle`、`cargo clippy` 无新增警告。

### R5 — 文档、构建与真实验收（依赖：R4）

1. 根 `Cargo.toml` 与 CI：把 `crates/`、`packages/provider-workers` 从默认门禁中移出（或标为可选 job）；`scripts/check.sh` 与 `package.json` 同步调整。
2. 改写 `docs/requirements.md` 当前目标节、`README.md`、`docs/architecture.md`、`docs/agent-adapter.md`、`docs/architecture/chat-contracts.md`、`AGENTS.md` 的范围描述；Craft 重建计划标为「已暂停，被本计划取代」。
3. 真实桌面验收（`scripts/debug.sh desktop`，结束后执行 `scripts/debug.sh stop`）：Claude Code、Codex 跑完 §1 的标准 3；OpenCode 跑标准 4。结果记入 `docs/dogfood/2026-10-xx-simple-chat-acceptance.md`，附脱敏日志。
4. 回填本计划状态与 `docs/PLANS.md`。

验证：完整 `pnpm check`；验收记录中每项都标注 `passed` / `partial` / `blocked` / `not-tested`。

## 7. 风险与缓解

| 风险 | 影响 | 缓解 |
| --- | --- | --- |
| CLI 升级导致输出格式变化 | 解析失败、工具卡片缺失 | 按版本保留 fixture；无法识别的事件降级为纯文本，不丢正文 |
| OpenCode 无稳定的非交互结构化输出 | T2 目标无法达成 | R0 先确认；退化为纯文本 adapter，或另开计划接 `opencode serve` |
| 删除 Task 代码时误删共享模块 | 构建或运行回归 | R4 依赖扫描 + 单独提交；先删前端，再删后端 |
| 权限语义被误解为逐工具审批 | 安全误导 | UI 与文档明确写「CLI 侧模式」；`auto` 需要显式选择 |
| Chat 未走 `execution_policy`（评审 S1） | cwd 越界 | R2 在 `chat_send` 入口补 cwd canonicalize 与项目内校验 |
| 冻结新内核造成沉没成本 | 过去投入暂时不用 | 不删代码，只移出默认构建；逐工具审批等经验在后续计划中复用 |

回滚：每个里程碑一个或多个独立 PR。R1–R3 只改前端和 adapter，可单独 revert；R4 的删除提交可整体 revert。Chat v2 数据格式不变，回滚不涉及数据迁移。

## 8. 验证策略

- 单元：adapter 参数映射、resume 解析、fixture 流解析、前端组件与斜杠命令。
- 契约：`contracts/tauri-contract.json` 的 Rust/TS 双向校验。
- 集成：现有 chat 仓储、补流、停止的进程回归保持通过。
- 真实：§1 标准 3/4 的桌面验收；网页 mock 和 fixture 不能代替。
- 未运行的检查不写成通过。

## 9. 待确认问题

- **Q1** 新内核（`crates/`、`packages/`、`src/workbench/`、`src-tauri/src/workbench/`）在 R5 后是保留冻结还是删除？默认：保留冻结，移出默认构建。
- **Q2** 「设置」之外是否还需要保留 Chat 导出、项目切换以外的旧功能（例如诊断包、应用内更新）？默认：保留导出、诊断包、更新。
- **Q3** 首批 T2 是否需要 Gemini CLI 等其他 Agent？本机未安装 Gemini，默认不纳入。

以上问题都有默认值，不阻塞 R0–R3 开工。

## 10. 实施记录

### 2026-10-20 回填

| 里程碑 | 状态 | 提交 | 验证与缺口 |
|---|---|---|---|
| R0 CLI 实测 | 完成（有缺口） | `7e1dfcb` | 样本在 `src-tauri/tests/fixtures/cli-streams/`，矩阵在 [适配器文档](../../agent-adapter.md)。每个 CLI 只录了一个合并场景；OpenCode 没有命令执行样本；Grok 只有 402 错误样本 |
| R1 入口收敛 | 完成 | `3dd9a9f` | `AppView` 只剩 chat/settings；根部和 Transcript 各有 ErrorBoundary；`VITE_LOOM_WORKBENCH` 分支已删除 |
| R2 探测与 OpenCode | 完成 | `3dd9a9f` | `$PATH` 优先，然后查通用候选；版本探测 2 秒超时；adapter 返回 capabilities；`chat_send` 补了 cwd 校验（§7 S1）。OpenCode 通过 `run --format json` + `--session` 实现原生续聊 |
| R3 会话界面 | 完成 | `3dd9a9f` | 组件已拆分，Markdown 手写最小解析，没有新增依赖；只有合成网页验证 |
| R4 删除 Task 工作流 | 完成 | `3dd9a9f` | 删除约 4.3 万行；旧符号零引用；Task 数据未触碰。**删除没有单独提交**（见下文“偏差”） |
| R5 文档、构建、验收 | 工程交付完成，用户验收待执行 | `0acbb10` + 本轮文档收尾 | CI 与 `check.sh` 已移出冻结内核；`pnpm check`、`pnpm tauri build --no-bundle` 已通过；真实桌面验收 not-tested |

成功标准逐项状态见 [验收记录](../../dogfood/2026-10-20-simple-chat-acceptance.md#成功标准重构计划-1)。目前只有 1、7、8 为 passed；2–5、9、10 为 partial（真实 CLI 进程或合成网页已通过，缺桌面验收）；6 中 Grok blocked（额度耗尽）。

**偏差**

- 回滚颗粒度：计划要求每个里程碑一个或多个独立 PR，R4 删除单独提交；实际 R1–R4 合在一个提交 `3dd9a9f`（240 个文件，+4k / −43k 行），无法按里程碑单独 revert。
- Q1：按默认值执行，`crates/`、`packages/`、`src/workbench/`、`src-tauri/src/workbench/` 保留冻结，并移出默认门禁。
- Codex 的 `ask` 与 `auto` 映射为同一组参数（`workspace-write; approval_policy=never`），Auto-run 只跳过 Loom 的回合确认。
- `3dd9a9f` 使用 `window.confirm`；独立 UI 打磨已在工作区改成应用内提示条，但尚未提交和桌面验收，不计入此基线。

**交付与验收边界**：本轮代码、工程检查、验收资料及阶段性 commit 已交付。用户已明确接手桌面验收；上述产品验收项保持开放，不能因工程交付完成而自动改成 passed。合入 `main` 由后续验收决定，本轮不自动合并。
