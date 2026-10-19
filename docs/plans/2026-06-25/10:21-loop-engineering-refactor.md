# Loom 重构：以 Loop Engineering 为骨架

- 日期：2026-06-25 10:21
- 状态：实施中（M0-M4 代码路径已落地并通过自动验证；M6 已完成 record/replay timeline canary、headless real-agent repair-loop canary、真实桌面 UI Auto repair canary 与严格机器校验；M5 为可选战略里程碑）
- 主题：用「Loop Engineering」的工程范式重构 Loom 的编排核心——把散落在各 UI 组件里的"调 agent + 推进状态"逻辑，收敛成一个**有界、可验证、上下文受控、可观测**的统一循环引擎；并把已有的确定性验证（testing acceptance gate）接进 implement 回路，闭合「实现→验证→修复」。

## 背景：Loop Engineering 是什么，Loom 落在哪

Loop Engineering（2026 业界范式，见参考）把工程重心从 prompt → context → harness → **loop** 逐层上移，核心两层：

- **内循环（机制层）**：单个 agent 任务的标准循环 `Init → Reason → Act → Observe → Update → Compact → Verify → Decide`。三大硬问题：①上下文管理（压缩 / 防 context rot）②终止与无进展检测（预算 / escalation）③验证即奖励信号（确定性检查 > LLM 自评）。
- **外循环（编排层）**：核心论点"别再手动 prompt agent，去设计替你 prompt 的循环系统"。组件：Automations(调度) / Worktrees(并行隔离) / Skills(项目知识) / Connectors(MCP) / Sub-agents(maker-checker) / State(持久记忆)。分级 L1(只报告)→L2(辅助+人工闸门)→L3(无人值守)。

**Loom 的定位**：它已是一个强 **harness + State** 层，且 planning 阶段已实现 maker-checker（多 agent 起草+互评+综合）。因此本重构**不追求全自动调度（L3）**，而是把每个阶段做成"教科书级"的循环：有界、确定性验证、上下文受控、单一可观测 trace。

### 对照体检（关键事实见「当前状态」）

| 支柱 | 现状 | 缺口 |
|---|---|---|
| State/记忆 | 强（`.loom/` 全量持久化） | trace 碎片化 |
| maker/checker | planning 已做 | implement/test 无 verifier sub-agent |
| 确定性验证 | testing 有 exit-code + acceptance gate | 未接进 implement 回路、不自动迭代 |
| 上下文工程/压缩 | 弱（前端字符串整段拼接） | 无压缩、不复用 session |
| 终止/预算/no-progress | 弱（planning 重试+超时；Auto 仅一轮） | 无迭代上限/预算/无进展检测/escalation |
| 可观测 trace | 分散 | 无统一 trace / token / 终止原因 |
| Worktree 隔离 | 无 | 多 agent/修复撞工作树 |

## 目标

1. **统一 Loop Engine（Rust core）**：把"组装上下文 → 调 agent → 观测结果 → 确定性验证 → 决定(继续/停/升级人)"做成单一、可 headless 测试的循环抽象；implement / repair / planning 阶段成为它的不同配置，而非各自重写编排。
2. **有界循环**：每个循环带预算（max 迭代、wall-clock 超时、可选 token）+ **no-progress 检测** + **escalation（升级给人）**，多个独立退出条件，杜绝无限循环/烧 token。
3. **闭合验证回路**：把 testing 的确定性验证（validation 命令 exit code / errorSummary / acceptance gate）作为循环的 reward signal 接进 implement——todo 跑完自动验证，pass 推进 / fail 进有界修复回路。
4. **上下文工程层**：以预算化、可压缩的 context-builder 取代前端字符串拼接；修复回路复用 agent session（resume）而非每轮全新 prompt。
5. **统一可观测 trace**：一条 append-only trace（每步：上下文摘要 / 动作 / 结果 / 迭代号 / 可选 token/usage / 终止原因），前端用单一 timeline 呈现，替代碎片化视图。
6. **Loom 自举验证（dogfood）**：把 Loom 自己后续的小型开发迭代纳入 Loom 流程，用真实需求、真实 agent run、真实 validation 命令和真实 trace 来验证 Loop Engineering，而不只停留在 mock 单测。
7. （战略可选）**worktree 隔离 + verifier sub-agent + L1/L2/L3 正式化**：为并行与半自主打基础。

## 非目标

- 不做 L3 全自动无人值守调度（Automations / 定时 triage / CI sweeper）——属产品方向大注，单独决策（见待确认问题 1）。
- 不接外部 Connectors（开 PR / 联 ticket / Slack）——同上，后置。
- 不替换底层 agent 适配器协议（codex_cli / claude_code_cli），Loop Engine 建在适配器之上。
- 不重写 planning 的多 agent 互评（它已符合 evaluator-optimizer，本期只把它纳入统一循环抽象，不改其语义）。
- 不在本期引入新的重型框架/依赖（loop 逻辑用 Rust 原生实现）。
- 不改 Tauri / 前端技术栈选型。
- 不要求本期一开始就让 Loom 完整托管自身全部开发；自举验证按能力成熟度分阶段启用，早期只记录和人工闸门，M2/M4 后再闭环。

## 成功标准

1. 存在一个 Rust `loop_engine` 模块，能在**无 GUI**下被 `cargo test` 驱动：给定 task 状态 + 一个可注入的 agent/验证 mock，循环按 `act → verify → decide` 推进，并在 ①验证通过 ②达迭代上限 ③超时 ④no-progress 四种条件下分别正确退出/升级。
2. implement 阶段：完成一个 todo 后**自动**选择并运行该 todo/LoopSpec 绑定的 validation 命令；通过则推进，失败则进入**有界修复回路**（默认 ≤N 轮，超出升级给人）。没有明确 validation 来源时不自动闭环，退回人工闸门。
3. 修复回路复用同一 agent session（resume），而非每轮重新拼全量 prompt；上下文按预算组装（不再整段塞 `finalPlan` 原文）。
4. 每个循环产生一条统一 trace，包含迭代号、动作、确定性验证结果、终止原因；token/usage 作为可选字段保留，待结构化输出解析就位后补充。前端能用单一 timeline 渲染 planning/implement/test 全过程。
5. `pnpm test` / `pnpm build` / `cargo test` 全绿；新增 loop_engine 与 context-builder 的单测。
6. 既有交互（手动 planning / 手动起 todo / testing cockpit）在重构后**行为不回退**，且可切换"自动闭环"与"逐步手动"（沿用并正式化 Auto/Manual）。
7. 至少完成一条 **Loom-on-Loom dogfood 迭代**：在 Loom 中创建/运行一个针对本仓库的小型真实任务（例如 M0 后的 selector/gate 清理、trace timeline 小切片或文档索引修正），记录从 planning/implement/testing 到验收的 trace，并把 `pnpm test` / `pnpm build` / `cargo test` 中相关命令作为 acceptance evidence。

## 当前状态（关键事实）

- **编排逻辑散在前端**：`SessionPane`（implement，逐 todo `startTodo + startCommandRun`）、`TestingPane`（test，PTY/管道双路 + acceptance gate + Auto 单轮修复）、`PlanningChat/PlanningTimeline`（多 agent 起草+互评+综合）各自维护 run 跟踪与状态推进，无统一循环。
- **状态机分散**：转移散在 `tasks/todoCompleted→reviewing`、`markReadyForTesting`、`completeTask`、`confirmPlan` 等命令里（`src-tauri/src/tasks.rs` + reducer）。
- **确定性验证已存在但未闭环**：testing 的 validation 命令走管道（保留 stderr 分离）→ `analyze_error`（`command_runner.rs:370`）→ `errorSummary` → acceptance gate（`TestingPane` 已实现）。这是现成的 reward signal，但只服务 testing，未接 implement。
- **CommandRun 语义混杂（关键约束，review 修订）**：当前 `CommandRun` 同时承载 implement/fix agent run、validation run、PTY preview；`tasks::add_command_run` 会把任务状态硬置为 `debugging`，`finish_command_run` 又按成功/失败硬置为 `verifying/debugging`。因此在接 implement loop 前，必须先给 command run 增加 `intent/kind`（如 `agent_action` / `validation` / `preview` / `loop_step`）并把状态推进从 command runner 下沉/转交给 Loop Policy，否则 implementation agent run 会被误当作 testing validation 推进。
- **validation 命令目前是项目级，不是 todo 级**：`PlanTodoItem` 没有 validation command/cwd 字段；validation 来源主要是 `TerminalSlot(kind=validation)` 和 `TestingPane` 的项目级 slots。故 M2 不能假设"todo 对应 validation"天然存在，必须先定义绑定策略：todo 显式绑定、LoopSpec 显式传入，或无绑定时退回人工闸门。
- **上下文拼接在前端**：`SessionPane.buildImplementationPrompt`、`utils/agentRun.buildRepairPrompt` 用字符串整段塞 `task.finalPlan` / `repairContextPreview`，无压缩、无预算。
- **session 捕获只在 planning 路径（关键约束，review 修订）**：`session_id`/`resume_command` 只存在于 **planning 路径**（`agents.rs` 的结构化输出解析 + `resume_command_for_profile`，agents.rs:712-715；落在 `AgentInvocation`）。implement / repair 实际走 `command_runner`——**那条路完全不捕获 session**（`command_runner.rs` 无 session 逻辑，`CommandRun` 结构无 session 字段）。故 M3 的「resume 回路」**不是简单复用**，要先把 session 捕获**下沉到 command-run 路径**（复用 agents.rs 的解析+resume 逻辑，给 `CommandRun` 加 `session_id/resume_command`）。
- **无 token 捕获（review 修订）**：`models.rs` 全无 token/usage 字段，命令路径不解析用量。故预算应以**迭代数 + wall-clock 为主（确定性、现成）**；token 预算/trace 列为后续，依赖结构化输出解析。
- **prompt 构建分散且重复**：`buildAgentCommandArgs` 在 `SessionPane.tsx:71`、`utils/agentRun.ts:19`、遗留 `Workspace/ImplementationPane.tsx:57` **共有三份**；`buildImplementationPrompt`(SessionPane/ImplementationPane) 与 `buildRepairPrompt`(agentRun) 分散——M3 下沉到 core 时顺手 dedup，并清理未路由使用的旧 Workspace pane，避免双实现漂移。
- **已有"摘要即记忆"雏形**：`discussionSummary`、`repairContextPreview`（`tasks::generate_repair_context`，`tasks.rs:462`）——可系统化为 context-builder 的压缩输出。
- **命令执行底座可复用**：`command_runner`（管道，agent run + 确定性验证）+ `pty`（dev-server）+ registry/finish 事件；Loop Engine 在其上编排。
- **持久化齐全**：`planningRuns / agentInvocations / planTodos / commandRuns / feedback / events`，但分散、无统一 trace / 迭代号 / token / 终止原因字段。
- **Auto/Manual 雏形**：`TestingPane` 已有本地 Auto 开关 + `autoHandledRef` 去重，是 L1/L2 的起点。
- **测试基建**：Rust `cargo test`（loop_engine 可在此 headless 测）；前端 node runner 仅覆盖纯函数/reducer/selector（React 无 RTL）——故循环核心放 Rust 是可测性的关键。
- **天然 dogfood 场景**：Loom 本仓库已具备可运行的项目级 validation（`pnpm test` / `pnpm build` / `cargo test`）和真实 agent 配置路径，适合作为 Loop Engine 的自举验证工作负载；但在 M2 之前只能作为人工闸门/记录型验证，不能假装已经自动闭环。

## 总体方案

把"循环"作为一等抽象下沉到 Rust core，前端退化为"渲染 trace + 触发/审批"：

```
LoopEngine.run(spec) 循环:
  ctx     = ContextBuilder.build(task, iteration, budget)   // 上下文工程：压缩/预算
  result  = AgentAdapter.invoke(ctx, resume_session?)       // 行动（复用 session）
  signal  = Verifier.check(task, result)                    // 确定性验证优先
  trace.append(iteration, ctx_summary, action, signal, usage?)
  decision = Policy.decide(signal, budget, no_progress)     // 继续/停/升级人
  → success / escalate / continue
```

- **RunIntent/状态边界**：所有命令运行先声明 intent（agent action / validation / preview / loop step）。`command_runner` 只负责进程、日志、退出码、错误摘要；任务状态推进由 Loop Policy 或显式 Tauri command 负责，避免 run 完成时隐式跳阶段。
- **Verifier**：implement 用 todo/LoopSpec 绑定的 validation 命令（复用 testing 的 `analyze_error`/exit-code）；planning 用互评结果（已存在）。确定性优先，LLM 判断仅兜底。Verifier 输出应包含 exit code、stdout/stderr tail、log refs、错误指纹，而不只是一份 stderr `ErrorSummary`。
- **Policy/预算**：max 迭代、超时（沿用 480s 量级）、no-progress（连续 N 轮状态/错误指纹不变则升级）、escalation 写入 trace。
- **ContextBuilder**：按预算挑 当前 todo + 最近日志尾 + 相关 feedback + 文件证据 + compact 历史摘要；输出受控 prompt；优先 resume session 仅追加增量。
- **Trace**：append-only 事件，统一 planning/implement/test；前端单一 timeline。
- **Dogfood lane**：每个里程碑完成后选择一个小而真实的 Loom 自身后续迭代作为 canary task，在 Loom 内跑 planning → implement → validation → acceptance；早期只记录 trace 和人工决策，M2 后验证有界修复，M4 后验证单一 timeline。dogfood 不替代单测/构建，只作为端到端真实性证明。
- 前端各 pane 改为：读 trace 渲染 + 在 escalation/risky 点提供人工闸门（保留手动模式）。

> 与 testing-cockpit（2026-06-24）衔接：那期已把 testing 的确定性验证 + 人机协同修复做出来；本期是把同一回路抽象化、接进 implement、并统一可观测。

## 里程碑

### M0：CommandRun 语义拆分（前置地基，防状态误推进）
1. 扩展 `CommandSpec` / `CommandRun`：增加 `intent`（`agent_action` / `validation` / `preview` / `loop_step`）、`loop_id`、`iteration`、`attempt`、`termination_reason` 等可选字段；保持旧 `.loom` 数据可反序列化（serde default）。
2. 调整 `tasks::add_command_run` / `finish_command_run`：默认不再对所有 run 硬编码切换任务状态；validation/loop run 的状态推进交给 Loop Policy，手动按钮仍走现有显式 command。
3. 统一 PTY 与管道 run 的 intent 标记：preview=PTY、validation=管道、agent_action=实现/修复 agent；前端 selectors/gate 依赖 intent，而不是仅靠 command 字符串和状态。
4. `cargo test` 覆盖旧数据兼容、不同 intent 的状态推进、validation 成功/失败仍能服务 acceptance gate。

### M1：Loop Engine 内核（Rust，地基，纯 headless 可测）
1. `src-tauri/src/loop_engine.rs`：定义 `LoopSpec`（阶段、预算、策略）、`LoopState`、`StepOutcome`、退出枚举（Success/Escalate/BudgetExhausted/NoProgress）。
2. Trait 抽象：`AgentInvoker`（行动）、`Verifier`（确定性验证）、`LoopTrace`（事件汇）——便于 mock 测试。
3. `Policy.decide`：多退出条件 + no-progress（错误指纹/状态哈希）+ 预算（迭代/超时）。
4. `cargo test` 覆盖四种退出路径（注入 mock invoker/verifier）。

### M2：闭合 implement 验证回路
1. 把 testing 的确定性验证抽成可被 implement 复用的 `Verifier`：`analyze_error`（command_runner.rs:370）现为**私有 `fn` 且仅基于 stderr**，需改 `pub(crate)` + 抽取。注意它只对**管道型 run** 有效（PTY 合并流不行）——故 implement 自动验证**用管道跑 validation 命令**（非 PTY），并扩展 Verifier 输出为 exit code + stdout/stderr tail + log refs + fingerprint。
2. 定义 validation 来源策略：优先 todo/LoopSpec 显式绑定 command+cwd，其次项目级 validation slot 默认项；无明确来源时不自动闭环，写 trace 并升级人工。
3. todo 跑完自动触发绑定 validation（管道，intent=`validation`）；pass 推进、fail 进有界修复回路（默认 ≤N 轮）。
4. escalation：超预算/no-progress 时回到人工，trace 记录原因；保留"逐步手动"模式（正式化 Auto/Manual = L1/L2）。
5. **预算口径**：迭代数 + wall-clock 为主（确定性）；token 预算为后续（依赖 M3 的结构化输出解析）。

### M3：上下文工程层
1. `ContextBuilder`（Rust core）：预算化组装（当前 todo / 日志尾 / feedback / 文件证据 / compact 历史摘要），取代 `buildImplementationPrompt`/`buildRepairPrompt` 的整段拼接；**同时 dedup 三份 `buildAgentCommandArgs`（SessionPane + utils/agentRun + Workspace/ImplementationPane）**，prompt/argv 构建集中到单一入口。
2. 清理或明确废弃未被 `WorkspaceSplit` 路由使用的旧 Workspace pane（至少 `ImplementationPane` / `ImplementationOutputPane` / `DebugPane`），避免后续维护两套 implementation UI。
3. **先建 command-run 路径的 session 捕获（review 修订，前置）**：把 `agents.rs` 的结构化输出解析 + `resume_command_for_profile`(712-715) **抽成共享模块复用**，给 `CommandRun` 加 `session_id/resume_command`，在 `command_runner` 里解析 agent 结构化输出捕获 session。
4. 在 3 之上，修复/迭代回路改为 **resume 同一 agent session**，仅追加增量上下文（而非每轮全新进程+全量 prompt）；resume 不可用时回退全量 prompt + compact 摘要兜底。
5. 系统化"摘要即记忆"：每轮产出 compact summary 落库，作为下一轮上下文来源（扩展 `repairContextPreview`/`discussionSummary` 模式）。

### M4：统一可观测 Trace + 单一 Timeline UI
1. 统一 trace 模型（迭代号 / 上下文摘要 / 动作 / 验证结果 / 终止原因 / 可选 token），planning/implement/test 共用；append-only 持久化。token 字段先 nullable，不阻塞 timeline 落地。
2. 前端单一 `TaskTimeline` 组件渲染 trace，逐步替代 `PlanningTimeline` / `SessionPane` / 部分 `TestingPane` 的定制视图（增量替换，不一次性推倒）。
3. 暴露迭代/预算/终止原因，让"观测"成为一等体验。

### M5（战略可选，单独决策）：worktree 隔离 + verifier sub-agent + L1/L2/L3 正式化
1. implement/fix agent 在 `git worktree` 隔离工作树运行，完成后 merge/产出 diff。
2. implement 阶段加独立 `verifier` sub-agent（不同会话/模型）按测试+约定 review，复刻 planning 的 maker-checker。
3. 把 Auto/Manual 正式化为 L1(只建议)/L2(辅助+人工闸门)/L3(守护内自主) 分级；为后续 Automations/Connectors 留接口（不在本期实现）。

### M6：Dogfood 验证与测试
1. 建立 `Loom-on-Loom` 自举验证任务模板：任务范围必须小、可回滚、可由 `pnpm test` / `pnpm build` / `cargo test` 证明，禁止把高风险迁移作为首批 dogfood。
2. M0 完成后：用 Loom 记录一次针对本仓库的手动/L1 迭代（例如 run intent selector 或文档计划修正），重点验证 command run intent、历史兼容和 acceptance evidence。
3. M2 完成后：选择一个有明确 validation 的小修复，开启 L2 自动闭环，验证「实现→验证失败→有界修复→升级/通过」。
4. M4 完成后：用单一 timeline 回放一条真实 Loom-on-Loom 迭代，确认 planning/implement/test trace 连贯、终止原因和人工闸门可审计。
5. dogfood 产物写入任务记录：计划、trace、validation 命令、失败/修复轮次、人工接管点、最终验收证据；作为后续 plan-review 和回归样本。

## 具体任务清单

- [x] M0：给 `CommandSpec`/`CommandRun` 增加 `intent`、loop/iteration/attempt/termination 可选字段，旧数据 serde default 兼容
- [x] M0：移除 `add_command_run`/`finish_command_run` 对所有 run 的硬编码阶段推进，改由 intent + Loop Policy/显式 command 决定
- [x] M0：前端 gate/selector 改用 run intent 识别 validation/preview/agent action，避免靠 command 字符串误判
- [x] M0：`cargo test` 覆盖旧数据兼容、intent 状态推进、validation acceptance gate 不回退
- [x] M1：`loop_engine.rs` 内核（LoopSpec/State/Outcome/退出枚举）+ trait 抽象 + Policy/预算/no-progress
- [x] M1：`cargo test` 覆盖 success/escalate/budget/no-progress 四路径（mock invoker+verifier）
- [x] M2：抽出可复用 `Verifier`（`analyze_error` 改 `pub(crate)`+抽取；输出 exit code/stdout/stderr/log refs/fingerprint；仅服务管道型 run）
- [x] M2：定义 todo/LoopSpec validation command 绑定策略；无绑定时升级人工而非自动闭环
- [x] M2：implement「todo 完成→自动跑绑定管道 validation→有界修复回路→escalation」
- [x] M2：预算口径=迭代+wall-clock 为主（token 后续）
- [x] M2：正式化 Auto/Manual = L1/L2（保留逐步手动不回退）
- [x] M3：`ContextBuilder` 预算化组装，替换前端字符串拼接 + dedup 三份 `buildAgentCommandArgs`
- [x] M3：清理/废弃未路由使用的旧 Workspace implementation/debug pane，避免双实现漂移
- [x] M3：**先**给 command-run 路径加 session 捕获（抽共享模块复用 agents.rs 解析+resume；`CommandRun` 加 session 字段）
- [x] M3：在上一步之上做修复/迭代回路 resume session（不可用回退全量 prompt）
- [x] M3：每轮 compact summary 落库作为下一轮上下文
- [x] M4：统一 trace 模型 + 持久化（迭代/终止原因/可选 token）
- [x] M4：前端单一 `TaskTimeline` 增量替换碎片视图
- [ ] M5（可选）：git worktree 隔离运行 implement/fix
- [ ] M5（可选）：implement verifier sub-agent（maker-checker）
- [ ] M5（可选）：L1/L2/L3 分级正式化 + 预留 Automations/Connectors 接口
- [x] M6：建立 Loom-on-Loom 自举验证任务模板（小范围、可回滚、validation 明确）
- [x] M6：建立 timeline record/replay canary + 桌面 runtime task 加载证据，验证统一 timeline 的本地任务回放路径
- [x] M6：补充 headless real-agent repair-loop canary，验证 failed validation → Codex repair → passing validation 的持久化证据
- [x] M6：M0 后用 Loom 记录一次手动/L1 自身迭代，验证 run intent、历史兼容和 acceptance evidence
- [x] M6：M2 后用 Loom 跑一次 L2 自动闭环自身小修复，验证有界修复/升级
- [x] M6：M4 后用单一 timeline 回放 dogfood 任务，确认 trace 连贯可审计
- [x] M6：`pnpm test` / `pnpm build` / `cargo test` 全绿 + 新增 loop_engine/context-builder 单测
- [x] 更新 `docs/PLANS.md` 索引（随本计划已完成）

## 实施进展（2026-06-25）

- 已落地：M0-M4 的代码路径、单测和前端类型/构建检查。
- 已落地：`docs/dogfood/loom-on-loom-template.md` 和 `docs/dogfood/2026-06-25-loop-timeline-canary.md`，并新增 `pnpm dogfood:verify` 校验本地 `.loom` record/replay canary 的 agent action、desktop UI replay、command runs 与 loop trace 一致性。
- 已落地：`docs/dogfood/2026-06-25-repair-loop-canary.md` 和 `.loom/tasks/task-repair-loop-canary.json`，记录 failed validation（exit 42）→真实 Codex repair（含 session/resume）→ passing validation（exit 0）的 headless 修复回路证据。
- 已落地：`docs/dogfood/2026-06-25-ui-auto-repair-canary.md` 和 `.loom/tasks/task-ui-auto-repair-canary.json`，通过真实桌面 UI 触发 Auto validation failure（exit 43）→真实 Codex repair（含 session/resume）→自动 validation rerun pass（exit 0）→`Accept / Done` 完成任务。
- 当前证据：`pnpm dogfood:verify` 通过；`pnpm dogfood:verify -- --strict --headless --task .loom/tasks/task-repair-loop-canary.json --report docs/dogfood/2026-06-25-repair-loop-canary.md --validation "node .loom/dogfood/repair-loop-validation.cjs"` 通过；`pnpm dogfood:verify -- --strict --task .loom/tasks/task-ui-auto-repair-canary.json --report docs/dogfood/2026-06-25-ui-auto-repair-canary.md --validation "node .loom/dogfood/ui-auto-validation.cjs"` 通过且 0 warning。
- 本轮补充修复：loop-bound implementation validation failure 生成 repair context 时不再强制进入 `fixing`；command runner 在进程退出后对 stdout/stderr reader drain 设置上限，避免已结束 agent run 因 descendant pipe 卡住而无法触发后续 validation。

## 风险

1. **大重构打断现有可用功能**：planning/implement/testing 已能用，下沉到 Rust 循环易回退。缓解：增量、可逆里程碑；M1 仅新增内核不接线；每步保留"逐步手动"路径；行为对照测试。
2. **前端编排逻辑迁移面广**：run 跟踪、状态推进散在多组件。缓解：M4 增量替换视图，不一次性推倒 `PlanningTimeline`/`SessionPane`。
3. **确定性验证并非处处可得**：有的 todo 无对应 validation 命令。缓解：无确定性验证时退回人工闸门（不自动闭环），由人确认；不强行用 LLM 自评当奖励。
4. **resume session 的前提与适配器差异**：① 前提——command-run 路径现**无 session 捕获**，M3 须先建（见 M3 步骤 2）；② codex/claude 的 resume 语义/可用性不同，长会话可能超上下文。缓解：先复用 agents.rs 已有的解析+resume 逻辑下沉；resume 失败回退全量 prompt + compact 摘要兜底；按适配器能力分支。
5. **no-progress 误判**：错误指纹相同但实则有进展（或反之）。缓解：指纹 + 迭代号 + 人工可随时接管；保守阈值，宁可升级给人。
6. **自动闭环放大错误（comprehension debt / cognitive surrender）**：loop engineering 反复强调的人侧风险。缓解：risky 动作（删除/迁移/提交）强制人工闸门；trace 可读、可审计；默认 L2 不默认 L3。
7. **token 成本**：多轮回路烧 token。缓解：预算硬上限 + 压缩 + session 复用 + 早验证早止损；适配器提供 usage 时在 trace 暴露 token。
8. **Rust↔前端契约扩面**：trace/loop 事件类型扩散到 TS。缓解：domain 类型集中、随里程碑小步演进。
9. **CommandRun 旧数据兼容与语义迁移**：已有任务里的 command run 没有 intent，若直接改 gate/selector 可能看不见历史验证证据。缓解：serde default + 前端 fallback（旧 run 通过 command/slot 兼容识别），新 run 必填 intent。
10. **validation 绑定不清导致误自动化**：项目级 validation 命令可能过宽、过慢，或与当前 todo 无关。缓解：LoopSpec/todo 显式绑定优先；无绑定时人工确认；trace 记录选择来源。
11. **dogfood 产生自证偏差**：Loom 用自己验证自己，容易把 demo 成功误当系统可靠。缓解：dogfood 只作为端到端真实性证据，必须与 mock 单测、构建、行为对照并行；失败也要保留 trace，不粉饰为通过。
12. **dogfood 任务过大导致验证噪声**：如果首批自举任务过大，失败原因会混在产品需求、agent 能力和 loop 基建之间。缓解：首批只选小切片，每条 dogfood 任务必须有明确 validation 命令和回滚边界。

## 待确认问题

1. **要不要走 M5 的自主化（L3 / Automations / Connectors）？** 这是 Loom 从"交互式 cockpit"扩成"半自主循环平台"的产品大注。计划默认**只做到 L2（辅助+人工闸门）**，M5 标可选、单独决策。
2. **implement 自动闭环默认开还是关？** 建议默认**关（手动/L1）**，自动闭环为显式开启（与现 Auto/Manual 一致，更安全）。
3. **有界修复回路的默认轮数 N？** 参考 loop engineering「3 次同测失败即升级」，建议默认 N=3，可配置。
4. **trace 是新建一等模型还是在现有 events 上扩展？** 建议**新建统一 trace 模型**（含迭代/可选 token/终止原因），现有 `events/runs/invocations` 逐步并入或映射；避免再造碎片。（涉及 Rust 持久化 schema，M4 开工时定）
5. **worktree 隔离的合并策略？**（M5 时定）自动 merge / 产 diff 给人 / 开 PR——取决于问题 1 的取向。
6. **planning 是否纳入统一 Loop Engine？** 它已符合 evaluator-optimizer。建议**先纳入抽象、不改语义**（M1 抽象兼容，M4 trace 统一），实际改造后置。
7. **command-run 的 session 捕获走哪条路？**（M3 开工时定，review 提出）二选一：①**扩展 `command_runner`**（加结构化输出解析，改动小但多一处解析）；②把 implement/fix 改走 **`agents.rs` 式路径**（更统一，但牵动更多）。建议先①（小步），统一留给 M4。
8. **token 预算/可观测何时做？** 现无 token 捕获基础设施；建议 M3 结构化解析就位后再加 token，trace 先留字段占位。
9. **todo 的 validation 绑定存在哪里？** 可选：①扩展 `PlanTodoItem` 加 `validationCommand/cwd`；②不改 todo，把绑定放在 `LoopSpec`；③沿用 project terminal slots 并在启动自动闭环时让用户选择。建议先②（最小 schema 扩散），M2 稳定后再评估是否持久化到 todo。
10. **首批 Loom-on-Loom dogfood 任务选什么？** 建议从低风险、验证明确的小任务开始：run intent selector 清理、trace formatter、文档索引/计划同步检查、旧 Workspace pane 删除中的一项；不要用 M0/M1 本身作为首条自动闭环任务。

## 验证策略

1. `cargo test`：M0 run intent/旧数据兼容/状态推进边界；loop_engine 四退出路径（mock）；Verifier 复用 `analyze_error` 的回归；ContextBuilder 预算/压缩纯函数；no-progress 指纹。**核心循环在 Rust 可 headless 测**是本计划可验证性的关键。
2. `pnpm test`：前端纯函数（run intent selectors、trace 格式化、context 引用格式化）+ 既有 reducer/selector 不回退。
3. `pnpm build`：tsc + vite（trace/loop 类型扩散）。
4. **行为对照**：重构后手动 planning / 手动起 todo / testing cockpit 行为不回退（保留逐步手动路径作为对照）。
5. **Loom-on-Loom dogfood 验证**：
   - M0 后：在 Loom 中创建一个针对本仓库的小任务，以手动/L1 方式跑完 planning → implement → validation，确认 run intent、日志和 acceptance evidence 被正确记录。
   - M2 后：选择一个有明确 validation 的小修复，开启 L2 自动闭环，确认失败进入有界修复、同错 no-progress 会升级给人。
   - M4 后：用单一 timeline 回放 dogfood 任务，确认 trace 能解释每次行动、验证、终止和人工决策。
6. 手动验证（`scripts/start-local.sh`，结束 `stop`）：
   - 造一个有 validation 命令的 todo：开自动闭环 → 确认「实现→自动验证→（失败）有界修复→（N 轮不过）升级给人」，trace 完整、终止原因正确。
   - 关自动闭环 → 确认仍可逐步手动，无行为回退。
   - 制造 no-progress（反复同错）→ 确认升级给人、不烧无限 token。
   - 单一 timeline 能连贯呈现 planning→implement→test 全过程。

## 参考

- Addy Osmani — Loop Engineering（命名并结构化该实践）
- O'Reilly Radar — Loop Engineering
- Tosea.ai — What Is Loop Engineering? A Complete Guide（内循环 anatomy / 失败模式 / 验证层级 / 预算）
- GitHub cobusgreyling/loop-engineering（五大件 + L1/L2/L3 + 模式目录）
- 关联背景：Anthropic「Building Effective Agents」（evaluator-optimizer / orchestrator-workers）、「Effective Context Engineering」、ReAct、Reflexion
