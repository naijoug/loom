# 简化 CLI Chat 实施计划

- 状态：实施中，未关闭（2026-10-20 回填）。P0–P8 代码已完成（`7e1dfcb`、`3dd9a9f`，分支 `codex/simple-cli-chat`，未合入 `main`）；P9 只完成了文档改写和工程门禁，桌面验收未执行。明细见 §6 和 [验收记录](../../dogfood/2026-10-20-simple-chat-acceptance.md)。
- 上位计划：[简化 CLI Chat 重构计划](00:44-simple-cli-chat-refactor.md)。目标、非目标、决策 D1–D8 和成功标准以那份计划为准，本文只写怎么做。
- UI 设计：[Loom Chat 初版 UI（Claude Design 画布）](https://claude.ai/artifact/MrT5QtAcWQJVt3tdTWodx4)，包含主会话（运行中）、新会话空状态、Composer 命令与 Agent 切换、设置页本机 Agent 四个画板。画布是私有链接，未分享前只有作者能打开。
- 代码基线：`main` @ `bed095c`。

## 1. 交付方式

- 每个工作包（P）是一个可单独 Review 和 revert 的 PR，从 `main` 拉分支 `refactor/simple-chat-pN-<slug>`。
- 依赖关系：

```text
P0 ─┬─> P3 ──> P4 ─┐
    │              ├─> P8 ──> P9
P1 ─┼─> P5 ──> P6 ─┤
    │              │
P2 ─┴─> P7 ────────┘
```

  P0、P1、P2 互不依赖，可以并行。P8（删除旧代码）必须等 P1–P7 都合入。
- 每个 PR 至少跑通 `pnpm test`；改了 Rust 的 PR 加跑 `cargo test --manifest-path src-tauri/Cargo.toml`；改了契约的 PR 加跑契约测试。P8 和 P9 跑完整的 `pnpm check` 和 `pnpm tauri build --no-bundle`。
- 预览只用 `scripts/debug.sh`：启动前和结束后都执行 `scripts/debug.sh stop`，端口固定 1420。

## 2. 设计令牌（从画布落到代码）

在 `src/styles/theme.css` 中新增 `--chat-*` 变量。旧的 `--loom-*` 在 P8 清理，这里不改。

| 令牌 | 深色 | 用途 |
| --- | --- | --- |
| `--chat-bg` | `#111214` | 主区底色 |
| `--chat-sidebar` | `#0C0D0F` | 侧栏 |
| `--chat-surface` | `#17181B` / `#15161A` | 输入框、卡片 |
| `--chat-border` | `#23252A` / `#2E3036` | 分隔、描边 |
| `--chat-text` / `--chat-muted` / `--chat-dim` | `#E6E6E3` / `#9A9DA4` / `#8B8E95` | 正文、次要文字、元信息（均达到 4.5:1） |
| `--chat-accent` | `#7AA2F7` | 选中、主按钮、运行中图标 |
| `--chat-ok` / `--chat-warn` / `--chat-danger` | `#8FD6AE` / `#E5B26B` / `#F0948B` | 成功或新增行、待验证、删除行或停止 |

- 字体：界面用 Geist，代码、路径和命令用 Geist Mono，两者已通过 `@fontsource` 自托管。中文回退到 `PingFang SC`。
- 浅色主题：P4 定一套对应的浅色值，与深色共用同一批变量名。

## 3. 工作包

### P0 — CLI 实测与样本（只读）

- **做什么**：用临时空项目和无害提示词，各调用一次 claude、codex、opencode、grok。每个 CLI 覆盖三种情况：纯问答、触发一次文件读取、触发一次命令执行。另外尝试原生续聊。
- **产出**：
  - `src-tauri/tests/fixtures/cli-streams/{claude-2.1.304,codex-0.163.0,opencode-<ver>,grok-1.0.62}/*.jsonl`，已脱敏。
  - `docs/agent-adapter.md` 新增「CLI 实测矩阵（2026-10-20）」：非交互参数、输出格式、工具事件字段（调用 id、名称、输入、结果、退出码）、续聊参数、权限参数、对 SIGTERM 的反应。
- **OpenCode 的出路**：如果没有 JSON 流，就记作纯文本 adapter，并在矩阵中标明。
- **验收**：样本可以原样喂给 P3 的解析测试。

### P1 — 入口收敛与错误兜底（前端）

- **修改**：
  - `src/main.tsx`：去掉 `VITE_LOOM_WORKBENCH` 和 `/preview/planning` 分支，根组件固定为 `App`。
  - `src/App.tsx`：`AppContent` 只渲染 `ChatShell + ChatPage` 或 `SettingsPage`，删除 `WorkflowShell`、`Board`、`WorkspaceSplit` 的懒加载。
  - `src/state/model.ts`：`AppView` 改为 `"chat" | "settings"`，`app/viewSelected` 的类型同步收窄。
  - `src/features/chat/ChatShell.tsx`、`src/components/Sidebar/Navigation.tsx`：移除「高级工作流」入口和 Task 计数。
  - `src/features/chat/ChatPage.tsx`：隐藏升格为 Task 的按钮。
- **新增**：`src/components/common/ErrorBoundary.tsx`，带降级 UI 和「重新加载」按钮。包住 `App` 根部，并在 `ChatTranscript` 子树单独包一层，这样某条消息渲染出错时整页不会白屏。
- **新增 i18n 层**（上位计划 D7，不引入依赖）：
  - `src/i18n/en.ts` 为源字典（`as const`），`src/i18n/zh-CN.ts` 以 `Messages` 类型约束，缺 key 时编译报错。
  - `src/i18n/index.tsx` 提供 `I18nProvider`、`useT()`、`formatRelativeTime()`（基于 `Intl`），插值只做 `{name}` 替换，不渲染 HTML。
  - Rust `AppSettings` 增加 `locale: "en" | "zh-CN"`，`#[serde(default)]` 为 `en`，复用现有 `theme_mode` 的读写路径；前端启动时读取，切换后立即重渲染并写回。`<html lang>` 同步。
  - P1 只迁移 P1 碰到的文案；P4–P7 新组件一律通过 `useT()` 取文案，`src/copy/workflow.ts` 在 P8 随旧工作流删除。
- **测试**：更新 `tests/unit/chatShell.test.cjs`；新增 `errorBoundary.test.cjs`，模拟子组件抛错后显示降级 UI；新增 `i18n.test.cjs`：两份字典 key 集合一致、插值、未知 locale 回落 `en`；Rust 测试旧设置文件（无 `locale`）读成 `en`。
- **验收**：`pnpm test`、`pnpm build`；`scripts/debug.sh web` 中只有 Chat 和设置两个入口。

### P2 — Agent 探测与能力声明（Rust）

- **修改** `src-tauri/src/agents/config.rs`：
  - 删除 `default_command_candidates` 里写死的 `/Users/guojian/...`（208–215 行），改成 `$HOME` 下的通用候选（`.local/bin`、`.npm-global/bin`、`.opencode/bin`、`.grok/bin`）加上 `/opt/homebrew/bin`、`/usr/local/bin`，最后再查 `$PATH`。
  - `discover_default_agents` 增加 `agent-opencode`；`agent-dummy` 只在 `cfg(debug_assertions)` 或测试中注册。
  - 版本探测：直接执行 `<cmd> --version`，不经过 shell，2 秒超时；拿不到版本时记为 `unknown`，不报错。
- **新增**：`AgentCapabilities { structured_stream, tool_events, native_resume, modes }`，由 adapter 声明，在诊断结果中返回。
- **契约**：同步更新 `contracts/tauri-contract.json`、`src-tauri/src/contracts.rs`、`src/domain` 中 Agent 诊断的类型，以及 `tests/unit/contract.test.cjs`。
- **按项目聚合会话**：新增只读命令 `chat_list_project_groups { projectPaths?: string[] }`，返回 `[{ project: ProjectSummary, sessions: summary[], storageError? }]`，项目顺序与 `list_recent_projects` 一致。单个项目读取失败只标在该项目的 `storageError` 上，不影响其他项目。前端展开某个项目时才传它的路径；不传参数时只返回项目及会话数，不读取会话正文。复用现有 `chat_list_sessions` 的仓储读取，不新增存储格式。
- **测试**：候选路径不含任何用户名；`$PATH` 优先；探测超时；OpenCode 默认项存在；聚合命令在某个项目目录损坏或不存在时，其余项目照常返回。
- **验收**：`cargo test`；设置页拿到四个 CLI 的真实路径和版本。

### P3 — OpenCode 适配与工具事件解析（Rust，依赖 P0）

- **工具片段扩展**（向后兼容，新字段都可选）：
  - `ChatMessagePart::Tool` 增加 `call_id`、`kind`（`read | edit | write | bash | search | web | other`）、`target`（路径或命令）、`exit_code`、`diff_stats { added, removed }`、`diff_preview`（有界，最多 40 行、8 KiB）。
  - 改动文件：`src-tauri/src/chat/models.rs`、`src/domain/chat.ts`、契约样本。旧会话缺少这些字段时按 `other` 渲染。
- **调用与结果配对**：`src-tauri/src/chat/mod.rs` 现有的 tool 解析（约 240–380 行）改为按 `call_id` 把 tool_use 和 tool_result 配成同一个片段，状态依次为 `running → done/error`。Claude 用 `tool_use.id` / `tool_use_id`，Codex 用 item id，具体字段按 P0 矩阵。
- **diff 来源**：Claude 的 Edit / Write 输入里有 old/new，Codex 有 patch 事件。在 Rust 侧算出统计和截断后的预览；前端不解析 CLI 原始 JSON。
- **OpenCode**：在 `agent_adapter.rs` 新增 `OpencodeAdapter` 和 `ADAPTER_OPENCODE`，在 `adapter_for` 中注册。模式、cwd、续聊参数按 P0 映射。有结构化输出的话，在 `agents/stream.rs` 增加对应的 `CliOutputMode` 和解析器。
- **测试**：每个样本对应一个解析测试（片段数量、配对结果、diff 统计）；adapter 参数映射和 resume 解析的单测。
- **验收**：`cargo test`；用 Claude 和 Codex 的样本回放，生成的片段与预期快照一致。

### P4 — 会话界面重构（前端，依赖 P1）

把 `ChatPage.tsx`（610 行）拆成以下组件，状态仍在 `useChatBridge` 和 `ChatProjection` 中，不新增全局 store：

| 新组件 | 对应画板 | 说明 |
| --- | --- | --- |
| `ProjectTree` | 主会话左栏 | 「项目 → 会话」两级树：项目行只显示名称（路径放在悬停提示里），可折叠，有「+」在该项目下新建会话；会话行显示模式图标（调研或 Code）、标题、运行中标记和 Agent。顶部有「添加项目」（复用 `register_project` 和目录选择）。折叠状态按项目记在 localStorage。数据来自 P2 的 `chat_list_project_groups`；由 `ChatInbox` 改造 |
| `SessionHeader` | 主会话顶栏 | 面包屑 `project /`、标题、`ModeSwitch`（Research / Code）、导出；不放 Agent 选择（见 P6）；由 `ChatSessionHeader` 改造 |
| `Transcript` | 主会话中栏 | 单列，最大宽度 860px；用户消息右对齐成气泡，助手消息不加框 |
| `ToolCallCard` | 工具卡片 | 按 `kind` 显示图标、名称、目标、元信息（行数、匹配数、+/−、退出码、耗时）；Edit / Write 展开显示 diff，Bash 展开显示输出尾部；运行中的卡片默认展开，完成后默认收起 |
| `StatusLine` | 输入框下方 | 只显示运行状态和续聊状态两项；模式、Agent 版本、模型、token 放在悬停提示里（有才显示）。执行日志从工具卡片和错误提示进入（复用 `ChatRunDetails`），状态行不再放入口 |
| `EmptySession` | 新会话画板 | 只选择模式：Research / Code 两张卡片（radiogroup，一句话说明能否改文件；CLI 参数不在这里展示），不出现 Agent 列表。下面是首条消息输入框，Agent 在输入框左下角的 `AgentPicker` 中选择（默认上次使用的 Agent）。不放快捷提问。发送时用所选模式对应的 `permissionMode` 和所选 Agent 调用 `chat_create` |

- **依赖说明**：`ProjectTree` 要用 P2 的 `chat_list_project_groups`。P2 没合入时，先在前端用 `list_recent_projects` 加逐个项目调用 `chat_list_sessions` 顶上，P2 合入后再替换，所以 P4 不用等 P2。
- **Markdown**：助手正文先只做最小渲染（段落、行内代码、代码块、列表），在 `src/features/chat/markdown.ts` 中手写转义。是否引入 `react-markdown`，在 P4 Review 时按体积决定。
- **样式**：新建 `src/features/chat/chat.css`，只用 §2 的令牌；`ChatPage.css` 中不再使用的规则在 P8 删除。
- **测试**：`ToolCallCard` 各 `kind` 的渲染、展开状态、旧片段降级；`Transcript` 中纯文本不执行 HTML；`EmptySession` 中未安装项不可选。
- **验收**：`scripts/debug.sh web` 配合 mock store，对照画板检查深浅两种主题；窗口缩窄到 760px 以下时侧栏折叠。

### P5 — Composer 与斜杠命令（前端，依赖 P1）

- **改造** `ChatComposer.tsx`：
  - Enter 发送，Shift+Enter 换行，Esc 停止（只在运行中且焦点在 Composer 内时生效），保留 IME 保护。
  - 新增 `src/features/chat/workMode.ts`，提供纯函数 `toWorkMode(permissionMode)` 和 `toPermissionMode(workMode, auto)`：`explore ↔ 调研`、`ask ↔ Code`、`auto ↔ Code + 自动执行`。替换现有 `chatPermission.ts` 中的三档循环，Shift+Tab 仍不修改模式。
  - 输入框按模式变化：调研模式显示「只读」说明；Code 模式显示「自动执行」开关，第一次打开时弹出警告（画板「Composer」右下），每个会话只提示一次。边框颜色和占位文案也随模式变化。
  - 顶栏的 `ModeSwitch` 和 `/mode` 命令都调用 `chat_update_meta { permissionMode }`；运行中禁止切换（后端本来就拒绝，前端也要禁用）。
  - 输入框右下角只放一个 32px 圆形图标按钮：空闲时为发送（向上箭头），运行中为停止（方块）；不显示文字和快捷键，文字放在 `aria-label`，快捷键放在 tooltip（Enter / Esc）。新会话页同样使用这个按钮，不另设「Start」。
- **新增** `slashCommands.ts`：纯函数，负责解析和过滤命令。命令有 `/new`、`/agent <id>`、`/mode <research|code|auto>`、`/clear-resume`、`/export`，执行时调用现有的 bridge 方法。
- **新增** `SlashMenu`：listbox 语义，↑↓ 选择，Enter 执行，Esc 关闭。
- **测试**：命令解析与过滤、键盘导航、Esc 只在运行中触发停止、IME 组合期间不提交；`workMode` 双向映射（含旧值和未知值回落到 `explore`）；运行中禁止切换模式。
- **验收**：组件测试；web 预览中的操作与画板一致。

### P6 — Agent 切换与能力降级（前端，依赖 P2、P5）

- Composer 左下角 `AgentPicker`（按钮 + 向上弹出的 menu，`menuitemradio` 语义）：显示已启用的 Agent、能力标签和版本；未安装的项置灰。新会话中只改本地草稿；已有会话切换前提示「会清除原生续聊」，确认后调用 `chat_set_agent`。`/agent <id>` 命令走同一路径。
- 默认 Agent：上次成功创建会话时用的 Agent，记在 localStorage；不可用时回落到第一个已安装的 T1 Agent。
- 按 `capabilities` 降级展示：没有 `tool_events` 时不渲染工具卡片区；没有 `native_resume` 时状态行显示「历史重放」。
- **测试**：切换确认流程；新会话中切换不弹确认；运行中禁止切换（后端本来就拒绝，前端也要禁用）；降级显示；默认 Agent 回落。

### P7 — 设置页精简（前端，依赖 P2）

- `src/features/settings/SettingsPage.tsx`（999 行）只保留四个分区：Agents、Appearance、Diagnostics & export、About & updates。Agents 页增加只读的 Work modes 表，列出每种模式对应的 Claude / Codex 权限参数（数据来自适配器，不在前端写死）。Appearance 包含 Language（English / 简体中文，radiogroup）和 Theme（System / Dark / Light）。删除 Task、Planning 和项目命令相关分区。
- 本机 Agent 页：「重新探测」按钮，表格列为 Agent、状态、路径、能力、启用、测试。「测试」按钮发起一次无副作用的探测调用（`--version`，或者 P0 确认过的只读调用）。
- 自定义 CLI 表单：名称、可执行文件、参数模板、输出格式。后端沿用现有 shell 包装拒绝规则，前端只显示规则说明。
- **测试**：更新 `settingsAgentDraft.test.cjs`；表格渲染各种状态。

### P8 — 删除旧工作流（依赖 P1–P7 全部合入）

- **步骤 1**：用依赖扫描生成删除清单，`scripts/` 下写一次性脚本，从 `src/main.tsx` 和 `src-tauri/src/lib.rs` 出发统计可达模块，清单写入 PR 描述。
- **前端预计删除**：`src/components/{Board,TaskDetail,Planning,Workspace,Header}`、`src/features/{tasks,planning,testing}`、`src/preview/`、`src/workbench/`、`src/state/reducers` 中与 Task 相关的部分（`tasks`、`commandTrace`、`planning`、`taskCollections`、`commands`）、对应 hooks、utils 和测试。
- **后端预计删除**：Task 命令和独占模块，包括 `tasks.rs`、`tasks/`、`agents/orchestrator.rs`、`implementation_review.rs`、`task_summary.rs`、`task_state.rs`、`task_repository.rs`、`plan_html.rs` 等。`src-tauri/src/workbench/` 从 `lib.rs` 注册中摘除。`command_runner.rs` 和 `pty.rs` 只有在没有其他引用时才删。
- **保留**：`chat/`、`agent_adapter.rs`、`agents/{config,stream}.rs`、`process_supervisor.rs`、`storage.rs`、`projects.rs`、`diagnostics.rs`、`app_update.rs`、`attachments.rs`（如果 Chat 用到）。
- **其他清理**：契约 JSON 删掉已移除的命令和事件；`ChatPage.css` 和 `--loom-*` 中的无用样式；`package.json` 中不再使用的依赖（例如 xterm）。
- **数据**：不触碰 `.loom/` 下的 Task 数据。
- **验收**：完整 `pnpm check`、`pnpm tauri build --no-bundle`、`cargo clippy` 无新增警告；`rg` 扫描已删除模块名零命中。

### P9 — 文档、CI 与真实验收（依赖 P8）

- **CI**：`.github/workflows/ci.yml` 把 `crates/`、`packages/provider-workers` 改为可选 job，或者从主门禁移出；`scripts/check.sh` 同步调整。
- **文档**：更新 `README.md`、`AGENTS.md`、`docs/requirements.md` 当前目标节、`docs/architecture.md`、`docs/architecture/chat-contracts.md`（tool 片段新字段）、`docs/agent-adapter.md`、`docs/testing.md` 验证矩阵；Craft 重建计划标为「已暂停，被取代」。
- **真实桌面验收**（`scripts/debug.sh desktop`），记入 `docs/dogfood/2026-10-xx-simple-chat-acceptance.md`：

| 场景 | Claude Code | Codex | OpenCode | Grok |
| --- | --- | --- | --- | --- |
| 流式回复 | 必须 | 必须 | 必须 | 回归 |
| 连续三轮追问 | 必须 | 必须 | 必须 | 回归 |
| 工具卡片（读文件、执行命令、编辑带 diff） | 必须 | 必须 | 按能力 | 按能力 |
| Esc 停止后再发 | 必须 | 必须 | 必须 | 回归 |
| 重启后续聊 | 必须 | 必须 | 按能力 | 回归 |
| 调研模式下无文件写入（让 Agent 尝试改文件，前后 `git status` 不变） | 必须 | 必须 | 必须 | 回归 |
| 会话中途从调研切到 Code 后能改文件 | 必须 | 必须 | 按能力 | — |
| 切换 Agent 后历史重放 | 必须（任选一对） | | | |
| 两个项目各建会话，侧栏分组正确，重启后分组和折叠状态保留 | 必须（与 Agent 无关） | | | |
| 新会话页只出现模式卡片；在输入框中选 Agent 后创建的会话使用该 Agent | 必须（与 Agent 无关） | | | |
| 默认英文；切到简体中文后全部界面文案切换，重启后保留 | 必须（与 Agent 无关） | | | |

- 每项记为 `passed`、`partial`、`blocked` 或 `not-tested`，附脱敏日志路径。回填上位计划 §10 和 `docs/PLANS.md`。

## 4. 风险与应对

- **P3 的 schema 扩展影响旧会话读取**：新字段全部可选；增加旧 v2 样本读取回归测试；不提升 `schemaVersion`。
- **P4 和 P5 同时修改 `ChatPage.tsx`**：P4 先拆出组件骨架并合入，P5 只改 `ChatComposer` 和新文件；如果两边并行，P5 需要在 P4 合入后 rebase。
- **文案遗漏翻译**：P8 验收时 `rg` 扫描 `src/features`、`src/components` 中的 CJK 字符（测试与字典文件除外），命中即视为硬编码文案。
- **P8 删除范围过大**：拆成两个提交，先删前端再删后端，每个提交都能单独通过测试；PR 描述附上可达性清单。
- **CLI 版本漂移**：样本目录带版本号；解析器遇到未知事件时降级为文本，并在执行日志中记一条 warning。

## 5. 待确认问题

无阻塞问题。沿用上位计划 §9 的默认值：新内核冻结后移出构建；保留导出、诊断包和更新；不纳入 Gemini。

## 6. 实施记录

### 2026-10-20 回填

没有按 §1 拆成单独的 PR 和 `refactor/simple-chat-pN-*` 分支：P0 是 `7e1dfcb`，P1–P8 合在 `3dd9a9f`，都在分支 `codex/simple-cli-chat` 上，还没有 PR。验证命令在 HEAD + 当前工作区复跑：`pnpm check`（82 个前端测试；148 个 Rust 测试，16 个真实 CLI 测试按设计忽略；strict Clippy；release 文档检查）通过，`pnpm tauri build --no-bundle` 通过。

| P | 状态 | 实现要点 | 未通过 / 偏差 |
|---|---|---|---|
| P0 | 完成（有缺口） | 4 个 CLI 的样本已脱敏；矩阵写入 `docs/agent-adapter.md` | 没有分别录纯问答、读文件、执行命令三种场景；OpenCode 没有命令样本；Grok 只有 402 错误样本；矩阵没有 SIGTERM 列（停止行为见真实 CLI stop 测试）；Codex 样本里有本机全局指令痕迹 |
| P1 | 完成 | ErrorBoundary、i18n（`en` / `zh-CN`，`AppSettings.locale`）、入口收敛 | — |
| P2 | 完成 | 通用候选路径，`$PATH` 优先；OpenCode 默认项；Dummy 只在 debug/test 构建出现；capabilities；`chat_list_project_groups` | 本机 OpenCode 原安装签名无效，设置页只能显示为不可启动 |
| P3 | 完成 | 工具片段新字段；按 `callId` 配对；Claude 的 Edit/Write 输入生成 diff；Codex 用文本基线计算 diff；`OpencodeAdapter` | — |
| P4 | 完成（合成验证） | ProjectTree、SessionHeader、Transcript、ToolCallCard、StatusLine、EmptySession；`chat.css` + `--chat-*` 令牌 | 工具卡片没有行数、匹配数、耗时；每张卡片都会重复渲染执行日志入口；只做了合成网页截图 |
| P5 | 完成（合成验证） | `workMode.ts`、`slashCommands.ts`、`SlashMenu`、圆形发送/停止按钮、Esc 停止 | Auto-run 警告用 `window.confirm`，不是画板中的弹层 |
| P6 | 完成（合成验证） | `AgentPicker`、默认 Agent 回落、能力降级 | 切换确认用 `window.confirm`，Tauri 下的表现未验证 |
| P7 | 完成 | 设置页只剩 4 个分区；Work modes 表的数据来自适配器 | Codex 的 Code 与 Auto-run 显示为同一组参数 |
| P8 | 完成 | 删除旧前后端代码；契约同步；旧符号零引用 | 前端和后端删除没有分成两个提交；`src/workbench/` 按上位计划 Q1 保留冻结，没有按本节清单删除 |
| P9 | 部分完成 | 文档改写（未提交）；CI 与 `check.sh` 移出冻结内核；验收记录 | 桌面验收 not-tested；真实 CLI 测试没有在 HEAD 上完整重跑；Grok blocked（402） |

P9 场景矩阵见 [验收记录](../../dogfood/2026-10-20-simple-chat-acceptance.md#p9-场景矩阵)。真实 CLI 进程层面，Claude / Codex / OpenCode 的必测项都有通过记录，但开发过程中出现过失败后又通过的运行，原因没有保存，记录里已列出。
