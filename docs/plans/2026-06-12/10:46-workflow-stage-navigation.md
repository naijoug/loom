# 流程 UI 优化：阶段回看 + 看板入口

- 日期：2026-06-12 10:46
- 状态：已实施（2026-06-24 完成 M1–M4；`pnpm test`/`pnpm build`/`cargo check` 全绿）
- 主题：解耦"任务所处阶段"与"用户正在查看的阶段"，让任务进入后续阶段后仍可回看 Planning / Implementing 内容；为 Linear 式看板补一个稳定入口，并清理看板中的假数据。

## 背景与问题诊断

实测反馈两个可用性问题：

1. **任务开始后无法回看之前阶段的内容。**
   根因在 `src/components/Workspace/WorkspaceSplit.tsx:143-201`：整个工作区按 `task.status` 硬切换唯一的 pane——
   - `completed` → `DonePane`
   - `debugging / fixing / verifying` → `TestingPane`
   - `ready_to_implement / implementing / reviewing` → `SessionPane`
   - 其余（planning 阶段）→ `PlanningChat`

   任务一旦进入实施，`PlanningChat`（含计划时间线、互评、最终计划）就不可达；进入测试后 `SessionPane`（todo 进度、实施日志）也不可达。顶部 `TaskFlow` 步骤条（`src/components/Header/TaskFlow.tsx`）是纯展示组件，步骤不可点击。此外 `SessionPane`、`TestingPane` 内部各自还有一套**静态假 stepper**（`session-header` 的 Plan/Implement/Test/Done 与 `testing-header` 的 Implement/Test/Done），与 header 的 Planning/Implementing/Testing/Done 命名不一致且同样不可点，加剧了"看起来能点但点不了"的困惑（即截图中红线标注的部分）。

2. **看板（Board）没有入口。**
   `currentView: "board"` 在 reducer 中存在，但只有三条路径能到达：注册项目（`projects/registered`）、切换到另一个项目（`projects/selected`）、删除当前选中任务（`tasks/removed`）。一旦点进任务（`tasks/selected` → `task-detail`），UI 上没有任何元素能回到看板：breadcrumb 不可点，sidebar 没有"全部任务"入口，再次点击当前项目只会折叠/展开任务列表（`Navigation.tsx:127-137`）。

   另外 `Board.tsx` 本身存在演示残留：标题硬编码（`"Children Three Kingdoms · audio"` / `"Add log search & filter"`，`Board.tsx:172`）、List/Board 切换是死的、Filter/Status 按钮无功能、优先级条与 tag（`taskTag`）是按 agentId 编造的假数据。

## 目标

1. 任务处于任何阶段（含完成后），用户都能回看已经过阶段的内容：计划讨论时间线、最终计划、实施 todo 与日志、测试周期记录。
2. header 的 `TaskFlow` 步骤条成为唯一、可点击的阶段导航；删除各 pane 内部重复的静态 stepper。
3. 从任意视图都有显式入口回到项目看板（Board）。
4. 清理 Board 的假数据，使标题、tag、优先级要么真实要么删除。

## 非目标

- 不做 Linear 式拖拽改状态（kanban 列间拖动）。
- 不允许"回到过去阶段重新执行"（如回到 planning 重新发起讨论、回退 task.status）；本期回看是只读的，阶段回退另立计划。
- 不改动 Rust 侧数据模型与持久化（所需数据 `planningRuns`、`planTodos`、`commandRuns` 等已全部在 `Task` 上持久化）。
- 不实现 Board 的 Filter / Status 真实筛选（可保留按钮占位或删除，见待确认问题）。

## 成功标准

1. 任务处于 `verifying` 时，点击 header 的 "Planning" 步骤能看到该任务完整的计划时间线和最终计划；点击 "Implementing" 能看到 todo 列表与实施记录；再点 "Testing" 回到当前阶段，全程不改变 `task.status`。
2. 回看非当前阶段时有明确的"回看模式"提示和一键"回到当前阶段"，且所有会触发执行/状态变更的控件（发起讨论、Start todo、Run、Accept/Done 等）被禁用或隐藏。
3. 尚未到达的阶段步骤不可点击（禁用态），当前阶段保留 running 动效。
4. 在任务详情视图下，至少两条路径可回看板：点击 breadcrumb 的项目名；sidebar 当前项目任务列表顶部的"All tasks"入口。
5. Board 标题显示真实项目名，不再出现硬编码演示文案；假 tag / 假优先级条被移除或替换为真实数据。
6. `pnpm test` 全绿（含为 `viewedStage` 新增的 reducer 用例），`pnpm build`（tsc + vite）通过。

## 当前状态（关键事实）

- 阶段映射已有现成函数：`src/state/selectors.ts` 的 `deriveTaskStages` 把 12 个 `TaskStatus` 归并为 planning / implementing / debugging(Testing) / done 四阶段。
- `PlanningChat` 通过 `state.app.selectedTaskId` 取任务并渲染 `PlanningTimeline`，对后期状态的任务渲染历史数据本身可行，只是路由不可达；其底部 composer 会触发新讨论，回看时需要禁用。
- `SessionPane` / `TestingPane` 接收 `project + task` props，渲染所需数据全部来自已持久化的 task 字段与 `state.commandRuns`，只读渲染无障碍。
- `AppView` 已含 `"board"`；缺的只是导航入口，不缺视图本身。
- 单测基建在 `tests/unit/reducer.test.cjs` 与 `tests/unit/selectors.test.cjs`（node 原生 runner，经 `scripts/run-tests.mjs`）。
- 计划创建后已有提交 `f63ebd2` 加入 planning live logs、session resume 与失败处理；因此 `PlanningTimeline` 也包含会触发后端/状态变更的控件（Retry、Re-run reviews、Record decision、Create tasks from plan），readOnly 必须覆盖到时间线内部，而不只是隐藏 `PlanningChat` composer。

## 总体方案

引入一个**纯前端、不持久化**的视图状态 `viewedStage`，把"任务在哪个阶段"（`task.status`，由流程驱动）与"用户在看哪个阶段"（UI 状态）解耦：

```ts
// src/state/reducer.ts AppSlice 新增
type WorkflowStageId = "planning" | "implementing" | "testing" | "done";
viewedStage: WorkflowStageId | null;
// null = 跟随 task.status（默认行为，与现状一致）
```

- 新 action：`app/stageViewed { stage | null }`。
- 在 `tasks/selected`、`tasks/new`、`projects/registered`、`projects/selected`、删除当前选中任务时重置为 `null`，避免回看状态泄漏到别的任务。
- `tasks/upserted` 需要区分场景：如果 upsert 后仍是同一个 `selectedTaskId`，保留 `viewedStage`，避免同一任务的实时更新/反馈/repair context 打断用户回看；如果 upsert 导致选中任务发生变化，再重置为 `null`。
- `WorkspaceSplit` 的 pane 选择改为：`effectiveStage = viewedStage ?? stageOf(task.status)`，再按 `effectiveStage` 渲染对应 pane；各 pane 增加 `readOnly` prop（`effectiveStage !== stageOf(task.status)` 时为 true）。
- `stageOf` 与 `deriveTaskStages` 共用同一映射，放在 `selectors.ts`，避免两份阶段表（目前 `WorkspaceSplit.STAGE_GROUPS` 和 `selectors.statusToFlowStep` 已经是重复的两份，本期顺手合并）。
- 对外阶段 id 统一为 `planning / implementing / testing / done`；现有 selector 内部的 `"debugging"` id 要迁移为 `"testing"`，只把底层 `TaskStatus.debugging/fixing/verifying` 归并到 `testing`。

## 里程碑

### M1：阶段回看（核心）

1. `reducer.ts`：新增 `viewedStage` 状态、`app/stageViewed` action、各重置点；`selectors.ts` 导出 `WorkflowStageId` 与 `stageOf(status)` 统一映射（`blocked/cancelled` 归入其发生时所在阶段的展示问题暂沿用现状归 testing）。
2. `TaskFlow.tsx` / `TaskFlowStep.tsx`：步骤改为 button——已完成与当前阶段可点（dispatch `app/stageViewed`；点当前阶段即重置为 null），未到达阶段 disabled；被回看的阶段加 `viewing` 选中态样式，同时当前实际阶段继续保留 running 动效。
3. `WorkspaceSplit.tsx`：按 `effectiveStage` 渲染 pane；顶部插入回看提示条（"正在回看 Planning 阶段（任务当前在 Testing）· 回到当前阶段"）。
4. 各 pane 接入 `readOnly`：
   - `PlanningChat`：readOnly 时隐藏 composer 与 agent 选择，并把 `readOnly` 继续传给 `PlanningTimeline`。
   - `PlanningTimeline`：readOnly 时只保留查看/复制/打开证据类动作；禁用或隐藏 Retry、Re-run reviews、Record decision、Create tasks from plan。
   - `SessionPane`：readOnly 时禁用 Start todo / Complete todo / Mark ready for testing / 指导输入 / agent 选择，todo 列表、最新 run 日志与文件证据照常展示。
   - `TestingPane`：readOnly 时禁用 Run / Stop / Re-run / repair handoff / feedback / Accept，保留 test cycles 与日志展示。
   - `DonePane`：当前实际阶段为 completed 时仍可作为正常 Done 视图；只有未来存在"非当前 done 回看"场景时才需要 readOnly。`Start follow-up` 不属于本期阶段回看控件，不在非 Done 阶段回看时展示。
5. 删除 `SessionPane` 与 `TestingPane` 内部的静态 stepper（`session-header` / `testing-header` 中的 `testing-stepper` 块），阶段导航统一收敛到 header；保留各自的状态 pill。

### M2：看板入口

1. `Header.tsx`：breadcrumb 拆段，项目名段渲染为 button，点击 dispatch `app/viewSelected: "board"`（保留 `data-tauri-drag-region` 在容器上，避免破坏窗口拖拽）。
2. `Navigation.tsx`：当前激活项目展开的任务列表顶部加一条 "All tasks"（看板图标）入口，点击进入 board；与任务项同级、样式区分。
3. （低成本可选）Esc 键在 task-detail 视图返回 board。

### M3：Board 去假数据

1. 标题改为真实项目名（删掉 `Board.tsx:172` 的硬编码三元）。
2. 删除假优先级条（`board-priority`）与编造的 `taskTag`；agent 徽标（`agentInitial`）保留，它是真实数据。
3. List/Board 切换、Filter、Status 三个死控件：本期直接移除（保持界面诚实），Board 列视图与筛选另立计划（见待确认问题 3）。
4. `taskShortId` 中 `task-preview-*` 的硬编码映射仅服务 preview 模式，移到 preview 入口或保留并注明，不影响真实任务。

### M4：测试与验证

见验证策略。

## 具体任务清单

- [x] reducer：`viewedStage` + `app/stageViewed` + 重置逻辑（M1）
- [x] selectors：统一 `WorkflowStageId`、导出 `stageOf`，`deriveTaskStages` 增加 `viewedStage` 入参输出 `viewing` 态，并把当前 `"debugging"` stage id 迁移为 `"testing"`（M1）
- [x] TaskFlow / TaskFlowStep 可点击化 + 样式（done 可点、active 动效、viewing 选中、pending 禁用）（M1）
- [x] WorkspaceSplit 按 effectiveStage 路由 + 回看提示条（M1）
- [x] PlanningChat / PlanningTimeline / SessionPane / TestingPane 的 readOnly 模式（M1）
- [x] 移除 pane 内重复 stepper（M1）
- [x] Header breadcrumb 项目段可点击回看板（M2）
- [x] Sidebar "All tasks" 入口（M2）
- [x] task-detail 视图下 Esc 返回看板（M2）
- [x] Board 假数据清理（M3）
- [x] reducer 单测：stageViewed 设置/重置、tasks/selected / projects/selected / tasks/new 重置 viewedStage、同任务 tasks/upserted 保留 viewedStage、跨任务 tasks/upserted 重置 viewedStage（M4）
- [x] selector 单测：`stageOf` 状态归并、`deriveTaskStages(status, viewedStage)` 的 viewing 态、pending 阶段不可点击数据（M4）
- [x] 更新 `docs/PLANS.md` 索引（随本计划已完成）

## 风险

1. **回看时实时事件仍在推进**：回看 Planning 时，testing 阶段的命令日志事件仍会更新 state；pane 按阶段隔离渲染，互不干扰，但提示条需实时反映 `task.status` 变化（如 verifying → completed 时提示文案更新）。注意同任务 `tasks/upserted` 不应自动清掉 `viewedStage`。
2. **readOnly 漏网**：pane 内可触发副作用的控件较分散（PlanningTimeline 的 retry/review/decision/confirm、SessionPane 的 subtask 双按钮与指导输入、TestingPane 的 run/stop/feedback/repair/accept）。需逐个核对，验收时以"回看模式下无任何 Tauri command 或任务状态写入被触发"为准。
3. **breadcrumb 可点击与 Tauri 拖拽区冲突**：`data-tauri-drag-region` 在父容器上，子 button 需确认不被拖拽吞掉点击（macOS 下需实测）。
4. **blocked/cancelled 的阶段归属**：现状归入 testing/debugging 组展示，回看语义下可接受，但若任务在 planning 即 blocked，header 高亮会跳到 Testing——沿用现状缺陷，不在本期修，记录之。

## 待确认问题

1. 回看 Planning 时是否需要"重新打开计划阶段"（修改计划并重置后续进度）？本期按只读处理，若需要则是状态机回退，单独立计划。
2. completed 任务默认落在 `DonePane`，四个阶段是否全部开放回看？计划按"全部开放"实现，成本相同。
3. Linear 式看板列视图（Board 多列 kanban + 真实 List/Board 切换）是否值得单独排期？本期只保证"看板可达 + 数据真实"，列视图建议作为下一个计划。
4. Board 的 Filter / Status 控件是删除还是保留占位？计划默认删除。

## 验证策略

1. `pnpm test`：现有用例全绿；新增 reducer 用例覆盖 `app/stageViewed`、切换任务/项目/新建任务时 `viewedStage` 重置、同任务 upsert 保留回看状态；新增 selector 用例覆盖阶段归并、viewing 态与 pending 禁用数据。
2. `pnpm build`：tsc 类型检查 + vite 构建通过（`WorkflowStageId` / `viewedStage` 类型扩散到 selectors/组件 props）。
3. Rust 侧无改动，跑 `cargo check`（在 `src-tauri/`）确认无意外牵连即可。
4. 手动验证（`scripts/preview.sh` 或 `scripts/start-local.sh`，端口 1420，结束后 `scripts/preview.sh stop`）：
   - 造一个推进到 testing 的任务，依次点击 header 四个阶段，确认内容、只读禁用、回看提示条与"回到当前阶段"。
   - 回看模式下逐个确认无命令被触发（观察日志面板无新 commandRun）。
   - 从任务详情经 breadcrumb 与 sidebar "All tasks" 两条路径回看板；看板标题为真实项目名。
   - 切换任务/项目后确认回看状态被重置。
