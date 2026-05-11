# 计划讨论 MVP — 实施计划

- **日期**：2026-05-09
- **作者**：Codex
- **状态**：已实施；真实 CLI adapter 切片已在 2026-05-11 补齐
- **关联设计**：`designs/loom.pen` 中 `appComp` 的计划阶段版本；导出图 `designs/exports/lGxnq.png`
- **布局方向**：参考 Codex App，左侧只做项目维度列表，右侧为 Agent 讨论输出流，底部固定需求输入框与 `@agent` 选择入口。

## 目标

实现 Loom 三步核心流程中的第一步：用户在主页面底部输入需求，通过输入框内的 `@codex`、`@claude`、`@amp` 等 mention 或显式选择控件邀请多个本地 Agent 参与计划讨论，系统在右侧输出完整讨论过程，保存每个 Agent 的原始输出与摘要，汇总生成最终计划文档，并把任务推进到可进入实施的 todo 列表。

本阶段完成后，用户能从“需求输入”走到“计划文档 + 实施 todo”。用户确认计划后进入实施界面：左侧显示待办任务列表，每个 todo 可通过 icon 按钮启动实施；右侧显示执行该 todo 的 Agent 输出过程。

## 非目标

- 不实现第二步代码实施和第三步调试验收闭环。
- 不引入 PTY、交互式终端会话或复杂 Agent 会话管理。
- 不新增依赖；优先使用现有 Tauri command、Rust 标准库、React 状态层和现有 UI 组件。
- 不实现云同步、插件市场、团队账号或远程 Agent。
- 不自动修改用户项目代码；计划阶段 Agent 默认只读。

## 成功标准

- 主页面右侧底部 composer 支持输入需求、通过 `@agent` 或选择控件选择多个 planning capability 的 Agent，并启动讨论。
- Agent 的讨论输出按消息流显示在右侧主体区域，而不是分散在左右两个工作区。
- 后端能按统一 adapter 协议调用至少一个真实 CLI Agent；无真实 Agent 时 dummy adapter 仍可验证完整流程。
- 每个 Agent 的输入、输出、退出状态、摘要和 evidenceRef 被持久化到任务历史。
- 系统生成最终计划 Markdown，并写入选定项目的 `docs/plans/YYYY-MM-dd-xxx.md`。
- 任务状态从 `drafting_requirements` / `planning` 推进到 `plan_review`，用户确认计划后进入 `ready_to_implement`。
- UI 显示最终计划摘要和 todo 列表，并提供确认计划/进入实施按钮。
- 进入实施后，左侧为 todo 列表，右侧为当前 todo 的 Agent 实施输出和补充输入框。
- `pnpm build`、`cargo check`、相关 Rust 单元测试通过。

## 当前状态

- 计划讨论 UI 已拆为右侧讨论流 + 底部 composer，支持 `@codex` / `@claude` / `@amp` mention 和 Agent chip。
- `run_planning_discussion` 已成为正式 Tauri command，能按统一 adapter 契约调用 Codex CLI、Claude Code CLI（binary 为 `claude`）、Amp CLI 或 dummy fixture。
- 每次 planning invocation 会保存 prompt、stdout、stderr evidence，记录状态、耗时、退出码、stderr tail 和 final plan path。
- dummy Agent 仅作为 `adapterType: "dummy"` 的测试 fixture；旧 `run_dummy_planning` bridge 已移除。
- 任务可从 planning 推进到 `plan_review`，展示 final plan；用户确认后生成 todo 并进入 `ready_to_implement`。
- `docs/requirements.md`：明确要求多 Agent 讨论、共识/冲突/风险汇总和最终计划文档。
- `designs/loom.pen`：已更新为 Codex App 风格计划阶段 UI，包括左侧项目列表、右侧讨论输出、底部 composer、`@agent` 选择和确认计划入口。

## 实施结果（2026-05-11）

- 真实 CLI adapter 已覆盖 Codex CLI、Claude Code CLI（命令名 `claude`）和 Amp CLI。
- 旧 dummy-only planning 入口已经从前端 hook 和 Tauri command handler 中移除；dummy 仍保留在 adapter 内部作为测试 fixture。
- README、计划索引和真实 CLI adapter 计划已同步当前能力边界。
- 已通过 `pnpm build`、`cargo check --manifest-path src-tauri/Cargo.toml`、`cargo test --manifest-path src-tauri/Cargo.toml`。

## 关键交互设计

- **左侧导航**：只承载项目列表和添加项目入口，不展示 Agents 分组，也不展示本轮参与 Agent。Agent 参与者只从右侧 composer 的 `@agent` mention 或选择器进入。
- **计划讨论区**：右侧主体区域展示用户需求、各 Agent 的计划建议、风险审查、冲突点和最终汇总消息，形成一条连续讨论流。
- **底部 composer**：固定在右侧底部，支持自然语言需求输入、`@codex` / `@claude` / `@amp` mention、Agent chip 展示和发送按钮。
- **计划确认**：Agent 讨论完成后，顶部或最终计划消息提供“Confirm Plan”操作；确认后写入计划文档并生成 todo。
- **实施入口**：进入实施界面后，左侧变为计划派生的 todo 列表；每项 todo 左侧有 icon 按钮用于启动该任务实施，右侧展示执行过程、日志、Agent 输出和补充输入框。
- **状态显示原则**：没有项目、没有任务、任务已就绪但未执行时，不显示 loading/spinner；只有后端正在执行 planning / implementing / debugging / fixing / verifying 时才显示运行中状态。

## 当前实施切片

本次先交付可运行的最小纵向切片：

- 左侧实际 UI 删除 Agents 管理区，只保留项目列表和添加项目。
- 添加项目通过 PROJECTS 标题右侧 icon 打开系统目录选择器，不再手动输入 path。
- 顶部栏背景与左侧导航保持同一 surface 颜色。
- 计划阶段右侧使用讨论流 + 底部 composer，composer 支持 `@agent` 文本解析并生成 agent chip。
- 后端新增正式 planning command，通过真实 CLI adapter 或 dummy fixture 形成多 Agent 讨论记录、最终计划和 todo。
- 确认计划后任务进入 `ready_to_implement`，前端切到 todo/实施占位视图；todo icon 可把单项任务标记为实施中并在右侧显示 Agent 输出占位。

## 里程碑

### M1 — 数据模型与状态契约

**结果**：计划讨论的核心数据能在前后端稳定表达和持久化。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 1.1 | 增加 `PlanningRun`、`AgentInvocation`、`PlanTodoItem` 类型 | `src/domain/task.ts`, `src-tauri/src/models.rs` | 无 | TypeScript 与 Rust camelCase DTO 字段一致 |
| 1.2 | 扩展 `Task`：保存 planningRuns、planTodos、finalPlanPath、discussionSummary | `src/domain/task.ts`, `src-tauri/src/models.rs` | 1.1 | 创建任务后 JSON 可读回新增字段默认值 |
| 1.3 | 增加 reducer action：planning started / invocation updated / plan generated | `src/state/reducer.ts` | 1.1 | UI 状态能从 running 到 plan_review，再经确认进入 ready_to_implement |

### M2 — Agent adapter 最小调用协议

**结果**：真实 CLI Agent 与 dummy Agent 使用同一 planning adapter 契约。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 2.1 | 定义 planning prompt 输入：需求、项目摘要、约束、期望输出结构 | `src-tauri/src/agents.rs` 或 `agent_adapter.rs` | M1 | dummy 输出包含同一 prompt 上下文 |
| 2.2 | 实现非交互 CLI adapter：`command + args + prompt-file`，不使用 PTY | `src-tauri/src/agents.rs`, `src-tauri/src/storage.rs` | 2.1 | 可对可用 CLI 写入 prompt 文件并捕获 stdout/stderr |
| 2.3 | 保存每次 Agent invocation 的原始输出、退出状态、耗时和错误摘要 | `src-tauri/src/tasks.rs`, `src-tauri/src/models.rs` | 2.2 | 失败 Agent 不阻断其它 Agent，任务事件记录失败 |

### M3 — 讨论汇总与计划文档生成

**结果**：多 Agent 输出被汇总为可执行计划文档和 todo。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 3.1 | 实现 deterministic summarizer：提取共识、冲突、风险、待确认问题 | `src-tauri/src/agents.rs` 或 `planning.rs` | M2 | 多份 dummy/fixture 输出能生成稳定摘要 |
| 3.2 | 生成最终 Markdown 计划文档 | `src-tauri/src/tasks.rs`, `src-tauri/src/storage.rs` | 3.1 | `docs/plans/YYYY-MM-dd-xxx.md` 存在且包含要求章节 |
| 3.3 | 从最终计划派生实施 todo 列表 | `src-tauri/src/models.rs`, `src-tauri/src/tasks.rs` | 3.2 | todo 至少包含标题、状态、排序、planRef |
| 3.4 | 计划成功后设置任务状态为 `plan_review`，确认后进入 `ready_to_implement` | `src-tauri/src/tasks.rs` | 3.2, 3.3 | Header 流程条停留在规划 Review，确认按钮可用 |

### M4 — 主页面计划 UI 接入

**结果**：现有主页面从 dummy 按钮升级为计划讨论工作台。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 4.1 | 将 `ImplementationPane` 拆出计划阶段视图组件，计划态采用“右侧讨论流 + 底部 composer”布局 | `src/components/Workspace/ImplementationPane.tsx`, `src/components/Workspace/PlanningPane.tsx` | M1 | 无任务/有任务/计划中/计划完成四种状态可渲染 |
| 4.2 | 增加底部需求 composer、`@agent` mention/chip、Agent 多选和启动讨论按钮 | `src/components/Workspace/PlanningPane.tsx`, `src/components/Workspace/Workspace.css` | 4.1, M2 | 禁用态、错误态、空 Agent 态明确；长需求不撑破布局 |
| 4.3 | 展示 Agent 讨论消息流、摘要、最终计划路径和确认计划入口 | `src/components/Workspace/PlanningPane.tsx` | M3 | 多 Agent 输出和 finalPlan 都能显示；讨论流可滚动 |
| 4.4 | 新增实施态布局：左侧 todo 列表，todo icon 启动实施；右侧显示当前 todo 的 Agent 输出 | `src/components/Workspace/ImplementationPane.tsx`, `src/components/Workspace/WorkspaceSplit.tsx` | 4.3 | 点击 todo icon 后 selected todo 进入实施中，右侧输出区显示 Agent 过程 |

### M5 — 验证与文档同步

**结果**：计划讨论 MVP 可被构建、检查和手动验收。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 5.1 | 为 adapter 输出解析、计划文档生成、todo 派生补 Rust 单元测试 | `src-tauri/src/*` | M2, M3 | `cargo test` 通过 |
| 5.2 | 构建检查 | `package.json`, `src-tauri/` | M4 | `pnpm build`、`cargo check` 通过 |
| 5.3 | 手动端到端验收 | 应用整体 | M1-M4 | 当前 Loom 仓库作为项目，dummy + 一个可用真实 Agent 生成计划并进入 todo |
| 5.4 | 更新 README 当前 MVP 状态 | `README.md` | 5.3 | README 与实际功能一致 |

## 风险

| 风险 | 可能性 | 影响 | 缓解措施 |
|------|--------|------|----------|
| 不同 Agent CLI 输入输出协议差异过大 | 高 | 高 | 第一版只支持非交互 prompt-file；adapter 失败只记录，不阻断其它 Agent |
| 计划汇总质量不稳定 | 中 | 中 | 先用结构化 prompt + deterministic 汇总模板，后续再引入专门汇总 Agent |
| 前端计划和实施职责混在一个组件 | 中 | 中 | 拆出 `PlanningPane`，保留 `ImplementationPane` 给第二阶段 |
| 写入用户项目 `docs/plans` 和 `.loom/` 可能产生脏文件 | 中 | 中 | 最终计划写入 `docs/plans`，raw evidence 集中在 `.loom/planning`，UI 显示路径 |
| 真实 CLI 调用耗时长或失败 | 中 | 中 | invocation 有状态、耗时、stderr 摘要；允许部分成功生成计划 |

## 待确认问题

- [x] 第一版真实 Agent 同时覆盖 Codex CLI、Claude Code CLI（binary 为 `claude`）和 Amp CLI。
- [ ] “进入实施任务”后 todo 是否允许逐项实施，还是先进入整个计划级实施。
- [x] 最终计划需要用户显式确认后才进入 `ready_to_implement`。

## 验证策略

- 数据层：用 Rust 单元测试覆盖 Agent invocation 序列化、计划文档写入、todo 派生和失败 Agent 记录。
- 前端：运行 `pnpm build` 验证类型和组件集成；手动检查空项目、无 Agent、运行中、失败、完成状态。
- 后端：运行 `cargo check` 与 `cargo test`。
- 端到端：登记当前 Loom 项目，输入需求，选择一个或多个可用真实 Agent，生成计划文档，确认 `docs/plans/YYYY-MM-dd-xxx.md` 存在，UI 显示 todo 并能点击进入实施入口。
