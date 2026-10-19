# Loom 2026-06 重构落地实施计划（前端 IA 重构 + 换肤）

> 关联设计：`designs/redesign-2026-06/`（`README.md` 为权威索引，`html/app.css` 为设计 token 与组件样式权威来源，`shots/` 为 30 张视觉基准，`html/loom.html` 为可交互原型）。
> 关联需求：`docs/requirements.md`（产品范围依据）。

## 1. 目标

把 `designs/redesign-2026-06` 的新信息架构与视觉，落到现有 Tauri + React 应用上，并能**端到端跑通整个开发闭环**：
`新建任务 → ① 计划聊天室 → ② 任务看板 → ③ 执行中(Agent session) → ④ 测试(多终端 + Debug 循环) → ⑤ 完成总结 →（Follow-up）`。
同时建立**可自我验收**的测试体系（单元 / 组件 / Rust / 类型 / 端到端 / 视觉回归），让实施者或 Agent 在每个里程碑结束时能自证"做对了"。

## 2. 非目标

- 不重写后端**执行内核与适配器**：`command_runner.rs`（进程/日志/生命周期）与 `agents.rs` 适配逻辑保持不动。
- 后端**仅补 2 个状态转换命令**（见 M0.5）：`mark_ready_for_testing`、`complete_task`——这是闭环闭合所必需的最小改动，不属于"重写"。其余 Rust commands、domain 模型、state reducer、bridge hooks 已覆盖整个闭环（见 §4），默认复用。
- 主 Agent 选择不新增独立后端命令：通过扩展 `CreateTaskInput.primary_agent_id` 与 `start_todo(..., primary_agent_id?)` 写回任务，避免为 Assignee 切换引入额外命令面。

> 术语注脚：设计稿中的 **"Testing"** 不是一个独立任务状态；`TaskStatus` 无 `testing`。UI 的 Testing 详情对应后端状态 **`debugging ∪ verifying ∪ fixing`**（命令跑完后由 `finish_command_run` 自动置 verifying/debugging，修复时 `generate_repair_context` 置 fixing）。下文出现 "Testing" 均按此映射理解。
- 不引入新产品能力（插件市场、云同步、团队协作、浏览器自动验收）——`requirements.md §10` 明确这些不在 MVP。
- 不改 Agent 适配器协议与命令执行内核（`command_runner.rs` / `agents.rs` 逻辑保持）。
- 不删除 Hermes 等核心 Agent；设计稿中移除的 `vibelive`/`hermes-engine` 仅为演示项目数据，与产品 Agent 无关。

## 3. 成功标准

1. 应用 IA 与设计一致：左侧项目列表、计划聊天室、Linear 看板、按状态分化的任务详情、6 页设置。
2. 主题 token 与 `html/app.css` 实测值一致（深/浅双主题），关键页面与 `shots/` 视觉对齐。
3. 整个闭环可在 `scripts/start-local.sh web` 下手动走通一遍（见 §7 验收清单），无控制台报错。
4. `pnpm test`（新增）、`tsc --noEmit`、`cargo test`、`pnpm build` 全绿。
5. `requirements.md §11` 验收标准逐条可对应到 UI 操作（§7 映射表）。

## 4. 当前状态（现状盘点）

### 4.1 后端（已就绪，复用）
已注册 Tauri 命令（`src-tauri/src/lib.rs` invoke_handler）：
- 项目：`list_recent_projects` / `register_project`
- Agent：`list_agents` / `create_agent` / `update_agent` / `delete_agent` / `set_agent_enabled` / `run_planning_discussion` / `run_plan_reviews`
- 任务：`create_task` / `list_tasks` / `record_planning_decision` / `confirm_plan` / `start_todo` / `complete_todo` / `append_feedback` / `generate_repair_context`
- 命令执行：`start_command_run` / `stop_command_run` / `command_runner_ready`
- 健康检查：`health_check`

Rust 各模块均带 `#[cfg(test)]` 单测（agents/projects/tasks/command_runner/storage）。

### 4.2 前端（部分复用，UI 需重构）
- 状态层完整：`src/state/reducer.ts`（`AppState = {app, projects, agents, tasks, commandRuns, commandLogs}`）、`AppStateContext`、`selectors.ts`。
- bridge hooks 完整：`useProjectBridge` / `useAgentBridge` / `useTaskBridge` / `useCommandBridge`，已覆盖上面所有命令。
- domain 类型完整：`task.ts` 含 12 态状态机、`PlanningRun`/`AgentInvocation`/`PlanReview`/`PlanTodoItem`/`CommandRun`/`UserFeedback`。
- **待重构 UI**：`App.tsx`（仅 workspace/settings 两视图）、`AppLayout`、`Sidebar`（固定导航，非项目列表）、`Header`（四阶段 stepper）、`Workspace/*`（左右分栏）、`Settings/SettingsPage`。
- **待迁移 token**：`src/styles/theme.css` 现为旧的 teal/cream `--theme-*` 命名与色值，与设计稿的 `--bg/--sidebar/--surface/--accent...`（neutral gray + blue）不一致。
- **缺测试工具链**：`package.json` 无 vitest/testing-library/test 脚本。

### 4.4 后端缺口（评审核实，必须补）
- **无 `completed` 转换**：Rust 中无任何地方把 `task.status` 置为 `completed`（`grep '"completed"' src-tauri/src` 零命中）。→ Done 详情进不去，闭环闭不上。
- **无 `reviewing → debugging` 命令**：`debugging`/`verifying` 仅由 `command_runner::finish_command_run`（命令跑完回调）自动置位；从 reviewing 进入"测试"目前只能靠"启动测试命令的副作用"，没有可点的显式转换。
- **日志为单一全局流**：`useCommandBridge` 仅 `listen("loom://command-log")` 一条；reducer 的 `commandLogs` 是**全局、截断到 299 行、所有 run 共享**，多终端会互相挤掉日志。需按 `runId` 分桶。
- **启动命令会全局清日志**：`startCommandRun` 目前先 dispatch `commands/cleared`；多终端时启动第二个 run 会清掉第一个 run 的日志。日志分桶时必须同时移除/改造这个全局清空行为。
→ 以上由 **M0.5**（后端）+ **M5**（reducer 分桶）解决。

### 4.3 差距小结（本计划的工作面）
| 维度 | 现状 | 目标（设计稿） |
|---|---|---|
| 侧栏 | 固定功能导航 | 项目列表 + Add project + hover ＋New task |
| 主导航 | workspace / settings 两视图 | board / planning / task-detail(按状态) / settings(6页) |
| 计划 | 向导式 PlanningWizard | 聊天室（多 Agent，大输入框，@邀请） |
| 任务列表 | 无 Linear 看板 | 按状态分组的 Linear 看板（每项目独立） |
| 任务详情 | 左右分栏 | 按状态分化：执行中=Agent session / 测试=多终端 / 完成=总结 |
| 设置 | 单页 | 6 页（General/Appearance/Agents/Commands&Safety/Notifications/About），右栏可滚动 |
| token | 旧 teal/cream | neutral gray + blue，双主题，实测值 |

## 5. 架构决策

1. **复用 state + bridge，只换 UI 层**：所有新组件通过现有 `useAppState()` + bridge hooks 读写，不新增全局状态库。
2. **路由用枚举驱动**，不引入 react-router：扩展 `AppSlice.currentView` 为
   `"board" | "planning" | "task-detail" | "settings"`（保留向后兼容的 `workspace` 过渡期别名），任务详情视图由 `selectedTaskId` 对应任务的 `status` 决定渲染哪个 pane（与设计的"状态决定详情视图"一致）。
3. **token 单一来源**：把 `html/app.css` 的 `:root` / `[data-theme=dark]` 变量整理进 `src/styles/theme.css`，旧 `--theme-*` 用别名过渡，组件逐步切到新变量后再删除旧变量。
4. **组件目录按屏组织**：`src/components/Board`、`src/components/Planning`、`src/components/TaskDetail/{SessionPane,TestingPane,DonePane}`、`src/components/Settings/pages/*`、`src/components/Sidebar`（改造）。
5. **多终端测试**复用 `start_command_run`：前端为前端/后端各起一个 command run，分别订阅日志事件流；Debug agent 走 `generate_repair_context` + `append_feedback`，修复后重启对应 run，形成 fix→restart→re-test。
6. **可逆小步**：每个里程碑独立可合入、可回滚，先骨架后细节（`AGENTS.md` 开发约定）。

## 6. 里程碑与具体任务

> 每个里程碑都给出：涉及文件、对应后端命令、**验收点**（实施者/Agent 可自查）。

### M0 · 基础设施（token + 测试工具链 + 路由模型）
- 任务
  - 迁移设计 token 到 `src/styles/theme.css`：`--bg/--sidebar/--surface/--surface-2/--inset/--selected/--border/--text/--text-2/--accent/--accent-fill` + 状态色 + 字号变量；旧 `--theme-*` 设为新变量别名。
  - 加测试工具链：`vitest` + `@testing-library/react` + `@testing-library/jest-dom` + `jsdom`；`package.json` 加 `"test": "vitest run"`、`"test:watch": "vitest"`；加 `vitest.config.ts`（jsdom 环境）。
  - 扩展 `AppSlice.currentView` 路由枚举与对应 action；为旧值加过渡别名，保证现有页面不崩。
- 文件：`src/styles/theme.css`、`package.json`、`vitest.config.ts`、`src/state/reducer.ts`、`src/state/reducer.test.ts`（新）
- 后端命令：无
- 验收点
  - `pnpm test` 能跑通至少 1 个 reducer 单测（视图切换）。
  - 切 `data-theme` 后，取色与设计稿一致（抽查 `--bg`/`--accent`）。
  - `tsc --noEmit` 绿。

### M0.5 · 后端状态补口（闭环闭合的最小后端改动）
> 评审发现的硬缺口：没有这两个命令，整个流程无法走到 Testing/Done（§4.4）。改动小但必须先于 M4–M6。
- 任务
  - 新增并注册 `mark_ready_for_testing(project_path, task_id)`：`reviewing → debugging`，写入 `events`（actor=user）。让"Mark ready for testing"按钮有真实语义。
  - 新增并注册 `complete_task(project_path, task_id)`：`verifying → completed`，写入 `events`（人工验收触发，**手动**而非自动，保留人工把关）。
  - 两命令补 Rust `#[cfg(test)]`（状态前置校验：仅允许合法前驱态转换）。
  - 前端：`useTaskBridge` 加两个 bridge 方法；reducer 走现有 `tasks/upserted` 回写权威 status（无需新 action）。
- 文件：`src-tauri/src/tasks.rs`、`src-tauri/src/lib.rs`（invoke_handler 注册）、`src/hooks/useTaskBridge.ts`、`src/domain/task.ts`（输入类型）
- 后端命令：**新增** `mark_ready_for_testing` / `complete_task`
- 验收点
  - `cargo test` 覆盖：reviewing→debugging、verifying→completed 成功；非法前驱态被拒。
  - 前端调用后 `list_tasks`/`upserted` 回来的 `task.status` 正确变化。
  - 决策落定：**任务 status 以后端为准**；前端 `tasks/todoCompleted` 等本地分支仅作即时反馈，后端回写后以后端为准（见 §8 决策）。

### M1 · 项目侧栏 + 路由骨架
- 任务
  - `Sidebar` 改为项目列表：渲染 `projects.recent`，当前项目高亮并展开其任务（`tasks` 按 `status` 显示 Linear 图标）。
  - 顶部 **＋ Add project**（`register_project`，走 `@tauri-apps/plugin-dialog` 选目录），位于列表上方。
  - 每个项目 hover 显示 **＋**（新建任务，跳新建任务弹窗，project 由该项决定）。
  - 底部固定 **Settings** 入口。
  - 点项目名 → 该项目 board；点任务 → 按状态打开任务详情。
- 文件：`src/components/Sidebar/*`、`src/layouts/AppLayout.tsx`、`src/App.tsx`、`src/hooks/useProjectBridge.ts`（复用）
- 后端命令：`list_recent_projects` / `register_project` / `list_tasks`
- 验收点
  - 启动后侧栏显示最近项目；新增项目后出现在列表并被选中。
  - hover 项目出现 ＋；点击进入新建任务弹窗且 project 正确。
  - 组件测试：给定 mock projects/tasks，渲染出正确条目与高亮。

### M2 · 任务看板（Linear，按项目）
- 任务
  - `Board` 按 `task.status` 分组（In Progress / Todo / Testing / Blocked / Done），渲染 ID、优先级、标签、执行 Agent。
  - Todo 行 **▶ Run**：`start_todo`（状态 Todo→实施）→ 打开执行中详情。
  - **＋ New task** 弹窗：`create_task`；含"Suggested primary"推荐主 Agent（依据 Agent 能力标签，Implement 优先 Claude Code）。扩展 `CreateTaskInput.primary_agent_id?: string`，用户接受推荐或手选后随任务持久化。
  - List/Board 视图切换（先实现 List）。
- 文件：`src/components/Board/*`、`src/components/Board/NewTaskModal.tsx`、`src/hooks/useTaskBridge.ts`（复用）
- 后端命令：`list_tasks` / `create_task` / `start_todo`
- 验收点
  - 看板按状态正确分组；改任务状态后分组随之变化。
  - ＋New task 走通 `create_task` 并落到聊天室。
  - 组件测试：分组渲染、▶ Run 触发 `start_todo`（mock bridge 断言调用）。

### M3 · 计划聊天室（多 Agent 讨论）
- 任务
  - `PlanningChat`：单条讨论流（用户 + 各 Agent 发言来自 `agentInvocations`/`planningRuns`），底部大输入框，`@` 邀请多 Agent。
  - 发起讨论：`run_planning_discussion`（多 Agent 并行计划）。
  - 右栏汇总：共识 / 冲突 / 风险（来自 `planningDecisions` + reviews），可 `record_planning_decision`。
  - Review：`run_plan_reviews`（协作 Agent 审查，结果按 info/risk/blocker 分类）。
  - **Create tasks from plan**：`confirm_plan` → 生成 `planTodos` → 跳看板。
- 文件：`src/components/Planning/*`（替换 `PlanningWizard`/`PlanningPane`）、`src/hooks/useTaskBridge.ts`/`useAgentBridge.ts`（复用）
- 后端命令：`run_planning_discussion` / `record_planning_decision` / `run_plan_reviews` / `confirm_plan`
- 验收点
  - 选多个 Agent 发起讨论，聊天流出现各 Agent 输出摘要。
  - 右栏出现共识/冲突/风险；Create tasks 后看板出现 todos。
  - 对应 `requirements.md §11.2`。

### M4 · 任务详情 · 执行中（Agent session）
- 任务
  - `SessionPane`：可见 agent loop（thinking → Read/Edit/Run 工具调用 → diff），数据来自 `agentInvocations` + `events` + `planTodos`；底部 steer 输入框（`append_feedback`）。
  - 子任务列表（`planTodos`）、Assignee 切换（`primaryAgentId`）。不新增 `assign_primary_agent` 命令；执行时通过扩展 `start_todo(project_path, task_id, todo_id, primary_agent_id?)` 写回当前主 Agent。
  - **Mark ready for testing**：只在所有 `planTodos` 已 `done`、任务处于 `reviewing` 时启用；再调用 `mark_ready_for_testing`（reviewing → debugging，M0.5 新命令）→ 打开测试详情。不要对 `pending` todo 直接调用 `complete_todo`，因为后端只允许完成 `implementing` todo。
- 文件：`src/components/TaskDetail/SessionPane.tsx`
- 后端命令：`start_todo` / `complete_todo` / `append_feedback` / `mark_ready_for_testing`（M0.5）
- 验收点
  - 执行中详情展示工具调用块与子任务进度。
  - Mark ready 后任务 status 变为 `debugging` 并打开测试详情（依赖 M0.5）。
  - 对应 `requirements.md §11.3`。

### M5 · 任务详情 · 测试（多终端 + Debug 循环）
- 任务
  - `TestingPane`：按项目类型起多终端——全栈/Tauri 同时开 **Frontend**（优先 `ProjectSummary.suggestedCommands` 中的 `pnpm dev` / `npm run dev`，否则用 MVP 默认 `pnpm dev`）+ **Backend**（Tauri 项目默认 `cargo tauri dev`）两个 `start_command_run`。
  - **日志分流（评审修正，必做）**：后端只有**一条全局事件流** `loom://command-log`（payload 带 `runId`）。改 reducer 把 `commandLogs` 从"单一全局 299 截断"改为**按 `runId` 分桶**（`Record<runId, lines[]>`，每桶独立截断），TestingPane 用 `event.payload.runId` demux 到对应 `TerminalCard`，避免两终端互相挤掉日志。
  - **移除全局清日志副作用**：`startCommandRun` 不再 dispatch 全局 `commands/cleared`；如需清理，只清指定 run 的日志桶或在项目切换时清空全部日志。
  - Debug agent：观察日志 → 摘要报错（`generate_repair_context`，置 fixing）→ 提出修复 → `append_feedback`（人工介入）→ `stop_command_run` 重启对应 run → 再次验证（命令跑完由 `finish_command_run` 自动置 verifying/debugging，形成 fix→restart→re-test）。
  - 错误识别：展示 detected error（命令失败 / 日志错误）。
- 文件：`src/components/TaskDetail/TestingPane.tsx`、`src/components/TaskDetail/TerminalCard.tsx`、`src/state/reducer.ts`（日志分桶）、`src/hooks/useCommandBridge.ts`（复用）
- 后端命令：`start_command_run` / `stop_command_run` / `generate_repair_context` / `append_feedback`
- 验收点
  - 同时启动两个终端并各自独立滚动日志；**组件/单元测试断言两终端日志互不覆盖**（按 runId 分桶）。
  - 制造一个失败命令 → 出现 detected error → 走一次修复重启再验证。
  - 至少跑通两轮"调试→修复→验收"（对应 `requirements.md §11.6`，依赖 M0.5 的 `complete_task` 才能最终收尾到 Done）。

### M6 · 任务详情 · 完成（总结）
- 任务
  - 进入 Done 前需 **Accept / 验收完成**：`verifying` 态下用户点"完成"→ `complete_task`（M0.5，verifying → completed）。这是 Done 详情唯一入口。
  - `DonePane`：交付总结（完成项、变更文件、验证证据=`commandRuns` 退出码/测试结果、时间线=`events`、剩余风险/Follow-up）。
  - **Start follow-up**：新建后续任务 → 回到计划聊天室（`create_task`）。
- 文件：`src/components/TaskDetail/DonePane.tsx`、`src/hooks/useTaskBridge.ts`（`complete_task` bridge）
- 后端命令：`complete_task`（M0.5，进入 Done）/ `create_task`（follow-up）
- 验收点
  - `verifying` 态可点"完成"→ status 变 `completed` → 展示总结四块（交付/变更/验证/时间线）。
  - Start follow-up 生成新任务并进入计划。

### M7 · 设置（6 页 + 滚动）
- 任务
  - 6 页：General / Appearance（主题/取色/字体/字号/密度）/ Agents（`list/create/update/delete/set_agent_enabled`）/ Commands & Safety（展示 `ProjectSummary.suggestedCommands`、MVP 默认命令、高风险策略文案与执行护栏；本轮不新增命令预设持久化模型）/ Notifications / About。
  - 右栏内容过长可滚动、左侧导航 sticky 固定（设计稿 `app.css` 已修复此布局：`.app` 锁行高 + `.setcols`/`.setbody` `min-height:0`，迁移时一并带入）。
  - 主题切换接 `ThemeContext`。
- 文件：`src/components/Settings/pages/*`、`src/components/Settings/SettingsPage.tsx`、`src/styles/theme.css`
- 后端命令：Agent 全套 CRUD
- 验收点
  - 六页均有实际内容；右栏长内容滚动、左导航不被顶走（窗口压到 ~680px 高验证）。
  - Agents 页可增删改、启停；“默认/主 Agent”通过新建任务推荐与执行时 `primaryAgentId` 持久化体现，不新增全局默认 Agent 配置（对应 `requirements.md §11.1`）。

### M8 · 端到端贯通 + 验收脚本
- 任务
  - 串起全链路路由跳转（§7 闭环），消除控制台报错与孤立态。
  - 写"冒烟脚本"`scripts/smoke-flow.sh`（或 `docs` 内手动 checklist）驱动 web 预览，逐屏核对。
  - 关键页与 `shots/` 视觉比对（人工或截图）。
- 文件：`scripts/smoke-flow.sh`（可选）、本计划 §7 清单
- 验收点：§3 成功标准 1–5 全部满足。

## 7. 自我验收与测试策略（重点）

### 7.1 分层测试
| 层 | 工具 | 覆盖 | 命令 |
|---|---|---|---|
| 类型 | tsc | 全前端类型 | `pnpm exec tsc --noEmit` |
| 单元 | 当前：Node 内置 test runner + TypeScript 临时编译；目标：vitest | reducer / selectors / commandLine 工具 | `pnpm test` |
| 组件 | 目标：vitest + testing-library（需显式批准新增 dev dependencies 后升级） | Sidebar/Board/Planning/各 Pane/Settings 渲染与交互（mock bridge） | `pnpm test` |
| Rust | cargo | 后端模块（已有） | `cargo test --manifest-path src-tauri/Cargo.toml` |
| 构建 | vite | 产物可构建 | `pnpm build` |
| 端到端 | 手动 + 脚本 | 见 7.2 闭环清单；主屏/Settings URL smoke | `pnpm smoke` |
| 交互 smoke | Chrome CDP | Add Project / New Task / Settings / session / testing 的真实点击与输入 | `pnpm smoke:interaction` |
| 视觉回归 | Chrome headless + 人工对照 | 关键页 vs `designs/redesign-2026-06/shots/`；dark/light 双主题 DOM 关键文案 + 截图非空；覆盖 loom 与 speaker 两个项目看板 | `pnpm smoke:visual` |

> 组件测试统一 mock `@tauri-apps/api` 的 `invoke`，断言"某 UI 动作 → 调用了正确的后端命令 + 正确参数"，从而在不起 Tauri 的情况下自证接线正确。

### 7.2 端到端闭环清单（每次大改后手动走一遍）
1. ＋ Add project → 选目录 → 侧栏出现并选中。
2. 项目 hover ＋ → 新建任务（标题/描述/邀请 Agent + 推荐主 Agent）→ Start discussion。
3. 计划聊天室：多 Agent 发言 → 右栏共识/冲突/风险 → Create tasks from plan。
4. 看板：出现 todos → ▶ Run 一个 → 进入执行中。
5. 执行中：可见 agent loop / 子任务 → Mark ready for testing。
6. 测试：前后端双终端日志各自滚动（互不覆盖）→ 制造报错 → Debug 修复 → 重启 → 再验证（≥2 轮）→ 命令通过后 status=verifying。
7. 完成：`verifying` 态点 **Accept/完成**（`complete_task`，status→completed）→ 总结四块 → Start follow-up → 回到计划。
8. 设置：6 页可用；右栏长内容滚动、左导航固定；主题切换深/浅。
9. 全程无控制台报错；刷新后任务状态可恢复（`list_tasks` 持久化）。

### 7.3 需求验收映射（`requirements.md §11`）
- §11.1 Agent 接入 → M7 Agents 页（≥3 Agent、检测可用、发送上下文）。
- §11.2 计划阶段 → M3（多 Agent 计划、汇总、确认进入实施）。
- §11.3 实施阶段 → M2/M4（选主 Agent、目录内实施、协作 Review 记录展示）。
- §11.4 调试验收 → M5（启动命令、实时日志、识别失败、交 Agent 修复、重跑验证）。
- §11.5 人工介入 → M5（输入问题 + 当前日志一并交 Agent，修复有验证记录）。
- §11.6 完成闭环 → M5/M6（≥2 轮循环、生成总结含修改范围/验证/剩余风险）。**前置依赖 M0.5 的 `complete_task`**，否则任务无法收尾到 `completed`、§11.6 不可达。

## 8. 决策、风险与待确认问题

### 已定决策（评审落定）
- **D1 status 权威性**：任务 status **以后端为准**（`list_tasks`/`tasks/upserted` 回写）。前端 `tasks/todoCompleted` 等本地分支仅作即时反馈，后端回写后即对齐，避免乐观更新与权威态打架。
- **D2 进入 Testing**：用真实命令 `mark_ready_for_testing`（reviewing→debugging，M0.5），而非"启动命令的副作用"，让按钮语义诚实。
- **D3 进入 Done**：`completed` 由用户**手动 Accept** 触发（`complete_task`，verifying→completed），保留人工验收把关，不自动完成。
- **D4 多终端日志**：reducer 按 `runId` 分桶（取代旧的单一全局 299 截断），每桶独立截断；前端按 `runId` demux；启动新 run 不再全局清空日志。
- **D5 主 Agent 持久化**：MVP 不新增独立 Assignee 命令；新建任务可写入 `primaryAgentId`，执行 todo 时 `start_todo` 可写回当前选择的主 Agent。
- **D6 命令预设范围**：MVP 不新增 Commands & Safety 持久化模型；测试命令先来自 `ProjectSummary.suggestedCommands` + 安全默认值，Settings 只展示和解释执行护栏。

### 风险
- **token 迁移期双套变量并存**导致样式漂移 → 用别名过渡、按屏切换、最后统一删除旧 `--theme-*`，每屏对照 `shots/`。
- **状态机边界态**（blocked/cancelled）设计稿未画 → 用 Board 兜底；详情仅对 implementing / debugging∪verifying∪fixing / completed 三类渲染对应 Pane，其余回 Board。
- **M0.5 状态转换的前驱校验**：命令需校验合法前驱态（如 `complete_task` 仅允许 verifying），非法转换返回错误并由 UI 提示，避免越级。

### 待确认问题（默认值已采用，可推翻）
- **①** 聊天室"@ 邀请 Agent"是否支持讨论中途追加（影响 `run_planning_discussion` 是否需增量）？**默认仅发起前选定。**
- **②** 测试阶段前/后端命令来源——Commands & Safety 预设 vs 项目分析自动推断？**默认用 `ProjectSummary.suggestedCommands` + MVP 安全默认值（如 `pnpm dev` / `cargo tauri dev`）；持久化命令预设列为后续。**
- **③** "Suggested primary"推荐算法——能力标签静态匹配 vs 历史成功率？**默认静态匹配（Implement→Claude Code）。**

## 9. 验证策略（声称完成前必须执行）

```bash
# 前端
pnpm exec tsc --noEmit
pnpm test
pnpm build
# Rust
cargo test --manifest-path src-tauri/Cargo.toml
# 预览 / 视觉
pnpm smoke
pnpm smoke:interaction
pnpm smoke:visual
# 端到端（手动走 §7.2 清单）
scripts/start-local.sh web      # http://127.0.0.1:1420
scripts/start-local.sh stop     # 验收后清理，勿留后台预览
```

完成判定：§3 成功标准 1–5 全绿 + §7.2 闭环清单全过 + §9 命令全绿。

## 10. 当前实施记录（2026-06-08）

- 已落地：设计 token、项目侧栏、计划聊天室、Linear 看板、执行中 Agent session、Testing 双终端 + Debug agent、Done 总结、6 页 Settings、主 Agent 持久化、`mark_ready_for_testing` / `complete_task`、命令日志按 `runId` 分桶。
- 已新增验证：`pnpm test`（无外部依赖，覆盖 reducer / selectors / commandLine）、`pnpm smoke`（主屏、speaker 项目看板和 Settings URL smoke）、`pnpm smoke:interaction`（Chrome CDP 真实点击 Add Project、New Task、项目切换、Settings、session/testing）、`pnpm smoke:visual`（Chrome headless 覆盖 dark/light 双主题 DOM 关键文案 + 28 张截图）、Rust command runner 真实进程 smoke（start、stop/cancel、两轮 failure→repair、success→accept 到 completed）。
- 已修复本地验证链路：Tauri `devUrl` 统一为 `http://127.0.0.1:1420`，避免 `localhost` 与 Vite `127.0.0.1` 监听不一致时桌面窗口回退到旧 bundled 资源；`scripts/start-local.sh stop` 补充清理本仓库 Tauri dev binary / bundled app 残留；`scripts/preview.sh status` 以固定端口监听进程为准，避免 `pnpm dev` 父进程 PID 与 Vite 监听进程不一致造成误判。
- 当前自动化验证全绿：`pnpm exec tsc --noEmit`、`pnpm test`、`cargo test --manifest-path src-tauri/Cargo.toml`、`pnpm build`、`pnpm smoke`、`pnpm smoke:interaction`、`pnpm smoke:visual`、`git diff --check`。
- 当前桌面启动与交互验证通过：`scripts/start-local.sh desktop` 能启动 `pnpm tauri dev`，Vite 固定端口 `http://127.0.0.1:1420` 返回 200，Rust dev binary 编译并运行 `target/debug/loom`；截图 `/tmp/loom-desktop-window.png` 显示新 Sidebar（Search tasks / Add project / Settings）。真实桌面已验证 Add Project 弹窗、Tauri 原生目录选择对话框（截图 `/tmp/loom-desktop-native-dir-dialog.png`）、项目切换到 Board、新建任务标题/描述输入使 Start discussion 启用，以及 Settings 六个页签均可点击并显示对应标题。验证结束后 `scripts/start-local.sh stop`，`scripts/start-local.sh status` 确认 preview 与 desktop dev 均未运行。
- 当前外部 Agent CLI 验证：`codex --version` 为 `codex-cli 0.133.0`，`claude --version` 为 `2.1.167 (Claude Code)`，`amp --version` 为 `0.0.1780306933-g944f6c`。已用 `codex exec --cd ... --sandbox read-only -` 跑真实只读 Agent smoke，返回指定文本 `LOOM_AGENT_SMOKE_CODEX` 且退出码 0（过程中有远端 websocket/analytics 警告，但命令成功）；已用 `claude -p --permission-mode plan --output-format text` 跑真实 Agent smoke，返回指定文本 `LOOM_AGENT_SMOKE_CLAUDE` 且退出码 0。`amp -x` 在当前账号下返回 `Error: Execute mode ... require paid credits ...`，退出码仍为 0；`run_cli_profile` 已通过 `stderr_only_error_is_failed_even_with_zero_exit` 单测覆盖该类“零退出码但 stderr 报错”的失败识别，UI 应展示 failed invocation 而非假成功。
- 约束记录：新增 `vitest` / Testing Library / `jsdom` 的 dev dependencies 曾因缺少显式批准被拒绝；当前先用 Node 内置 test runner 满足 `pnpm test` 门槛。若后续明确批准新增 dev dependencies，应升级到计划目标中的 Vitest + Testing Library 组件测试。
- 外部依赖边界：Amp 当前账号缺少 execute-mode paid credits，因此无法证明 Amp 成功模型输出；已证明其失败会被适配器判定为 failed invocation。除该账号/计费前置外，当前自动化、Rust smoke、真实桌面验证、Codex CLI smoke 与 Claude Code CLI smoke 已覆盖 Web 预览关键路由、点击输入、视觉截图、Tauri 桌面原生目录选择、真实本地命令 run/stop/fix/accept、两轮 debug→repair→re-test、以及真实外部 Agent CLI 调用。

### 10.1 真实计划闭环硬化（2026-06-08 补充）

- **背景**：M3 计划聊天室前后端早已接真实 CLI（`run_planning_discussion` 直接 spawn 本地 `codex`/`claude`、写证据文件、落 `docs/plans/` 计划文档），并非 mock；"全是 mock" 的体验来自 `/preview/planning` 预览壳（`hasTauriRuntime()` 为 false 时 bridge 全部 no-op）。本轮把"真实可用"从"已接线"推进到"端到端实证 + 修复真实缺陷"。
- **修复 1（Claude 计划输出被截断）**：`default_profile_args` 的 Claude 计划参数去掉 `--permission-mode plan`，改为 `-p --output-format text`。plan 模式下 Claude 把真实计划交给 ExitPlanMode 工具，`--output-format text` 只打印一句确认语，导致采集到的计划文档近乎为空。只读语义由 prompt 约束 + headless `-p`（拒绝写文件工具）兜底；Codex 仍由 `--sandbox read-only` 在 OS 层强制只读。
- **修复 2（计划文档只有一行摘要）**：`render_final_plan` 新增 `## Agent Proposals` 段，内嵌每个成功 Agent 的**完整 `raw_output`**（失败 Agent 不贡献正文）。此前计划文档只放 `output_summary` 单行摘要，真实计划正文仅存于证据文件，导致交付物形同模板。
- **超时放宽**：`PLANNING_TIMEOUT_MS` 120s → 240s/Agent（实测单个 Claude 计划约 72s；Agent 串行执行，总墙钟约为此值 × Agent 数）。
- **前端反馈**：`PlanningChat` 在 `isLoadingTasks` 期间显示"Running planning discussion"虚线脉冲提示并列出参与 Agent、按钮变"Discussing…"，避免 1–4 分钟阻塞被误判为卡死。
- **新增验证**：`agents::tests::claude_planning_profile_avoids_plan_permission_mode`、`final_plan_embeds_full_successful_agent_proposals`（常规 `cargo test`）；`real_claude_planning_agent_produces_usable_plan_document`（`#[ignore]`，需本地 `claude` 凭据，`--ignored` 显式跑通，实测 71.7s 返回 >200 字符真实计划且全文进入计划文档）。
- **全绿复跑**：`cargo test`（62 passed / 1 ignored）、`pnpm exec tsc --noEmit`、`pnpm test`（12）、`pnpm build`。
- **已知剩余项（非本轮）**：多 Agent 仍是**串行**（计划 §8 设想的"并行"未落地）；无逐 Agent 流式进度（只有整体 loading）；打包后的桌面 App 需保证 `claude`/`codex` 在 PATH（dev 从终端 `cargo tauri dev` 继承 PATH 没问题）。

### 10.2 真机反馈修复（2026-06-08 桌面试用）

- **计划输入框删除后被回填（bug）**：`PlanningChat` 原有 `useEffect([message, task.rawRequirement])` 在 `message` 变空时把任务需求写回，导致用户删字后被重新填充。改为仅在**所选任务变化**（keyed on `taskId`）时 seed，且仅当任务尚无 `discussionSummary` 时用需求预填；用户编辑（含清空）不再被覆盖。讨论成功后清空输入框（需求已在对话流里）。
- **New task 不再强制填 title/description**：移除 Board "New task" 与侧栏项目 `+` 弹出的 `NewTaskModal`，改为新增 reducer action `tasks/new` → 直接进计划聊天室（`currentView=planning`、清空 `selectedTaskId`），空输入框等待用户描述需求。（`NewTaskModal` 仍保留给 Done 的 Start follow-up 与预览壳。）
- **任务标题由首个计划会话自动生成**：后端 `run_planning_discussion` 在**首个 planning run 且有成功 Agent**时调用新增 `derive_plan_title`，从首个成功 Agent 计划输出里抽取 Goal（支持 `## Goal` 标题式与 `**Goal:** …` 行内式，去 Markdown 装饰并截断；无 Goal 则回退首个正文行）覆盖任务标题，前端创建时仅给临时标题占位。新增 4 个 `derive_plan_title` 单测。
- **复跑全绿**：`cargo test`（66 passed / 1 ignored）、`pnpm exec tsc --noEmit`、`pnpm test`（12）、`pnpm build`。

### 10.3 真机反馈修复（2026-06-08 第二轮）

- **@mention 打错导致无法发送**：`PlanningChat` 的 `selectedAgents` 原来在有 `@mention` 但无匹配时返回空集合 → "No available planning Agent selected." 且 Send 禁用。改为**无匹配时回退到全部可用 planning Agent**，typo 不再阻塞发送。发送按钮文案 `Start discussion` → **`Send`**（运行中 `Sending…`）。
- **移除右上角 Stop Task 按钮**：`Header` 删除 `Stop Task`（无真实停止语义，且 session 生命周期不该从这里控制），连带移除未用的 `canStopTask` 与 `Button` 引用。
- **侧栏右键删除任务**：`Navigation` 任务项加 `onContextMenu` → 光标处弹出上下文菜单（"Delete task"）→ 确认弹窗（Cancel / Delete）→ `deleteTask`。新增后端 `delete_task(project_path, task_id)`（删除任务 json + 尽力清理该任务 `.loom/planning/<task_id>` 证据目录，缺失即 no-op 成功）并注册；前端 `useTaskBridge.deleteTask` + reducer `tasks/removed`（移除任务与其 commandRuns，若为当前选中则清空选择并回 Board）。上下文菜单与确认弹窗样式自带于 `Sidebar.css`（不依赖 Board.css）。
- **澄清 Codex `--ask-for-approval` 报错**：当前代码与磁盘 `agents.json`（codex `args: []`）均无该参数，截图里的失败来自**旧构建留下的 run 证据**；重新构建运行后该路径已是 `codex exec --cd … --sandbox read-only -`。
- **新增验证**：`delete_task_removes_file_and_is_idempotent`（cargo）；reducer 单测 `tasks/new`、`tasks/removed`（选中/未选中两种）。
- **复跑全绿**：`cargo test`（67 passed / 1 ignored）、`pnpm exec tsc --noEmit`、`pnpm test`（15）、`pnpm build`。

## 11. 建议实施顺序

`M0 → M0.5 → M1 → M2 → M3 → M4 → M5 → M6 → M7 → M8`。
- **M0**（token+测试工具链）是地基，必须最先做。
- **M0.5**（后端 2 个状态命令）紧随其后：它是 M4/M5/M6 闭环的硬前置，越早补越能尽早端到端验证。
- **M7**（设置）相对独立，可与 M3–M6 并行。
- **M8** 在所有屏就绪后收口跑 §7.2 闭环清单。
