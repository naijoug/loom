# 修复：循环引擎单一真相源 + 有界超时

- 日期：2026-06-25 14:54
- 状态：已完成（方案 B 已落地；`SessionPane` 与 `TestingPane` 共用 `loopPolicy`；生产 timeout ticker + `terminationReason=timeout` 持久化通道已接通；Rust 死代码已删除；验证全绿）
- 主题：消除"Rust LoopEngine/validation 死代码 vs 前端重写循环"的双实现，确立**单一可测的循环策略真相源**；给生产循环补 **wall-clock 超时**；统一 no-progress 阈值。

## 背景：code review 发现的三个问题

10:21 计划实施后 review 结论：功能能跑、测试全绿，但**核心架构目标未达成**——

1. **死代码 + 双实现（major）**：`src-tauri/src/loop_engine.rs` 与 `validation.rs` 在生产路径**零引用**（`grep loop_engine:: / validation::` 仅 validation.rs 内部引了一个类型，而 validation.rs 本身没人调）。真正的 implement/repair 循环策略（预算/no-progress/escalation）和验证 fingerprint **在前端 `SessionPane.tsx` 的 `autoLoop` 里重写了一遍**。headless cargo 测试测的是不跑的代码。
2. **生产循环无 wall-clock 超时（major）**：前端 autoLoop 纯靠"run 完成"事件驱动，只有迭代上限(3)+no-progress(3)，**无时间预算**；超时退出只存在于不跑的 Rust 引擎里。validation/repair run 挂住不返回 → 自动循环永久停滞、不升级。`command_runner` 对这些 run 也无超时。
3. **重复策略已漂移（minor）**：Rust `no_progress_limit=2` vs 前端 `NO_PROGRESS_FAILURE_LIMIT=3`。

> 做对的部分（不在本计划改动）：`session_capture.rs`（正确落在 command-run 路径）、`context_builder.rs`（经 `build_implementation_context` 暴露 + 前端兜底回退）、`analyze_error` 改 `pub(crate)`、builders 已 dedup、CommandRun intent/loop/session/trace 字段、resume invocation。

## 方案选择（A vs B）

- **方案 A（忠于原计划）**：把 Rust `LoopEngine`+`validation` 真正接进生产——加 async loop driver / Tauri 命令用它驱动循环，前端退化为渲染 trace + 人工闸门。**代价高**：LoopEngine 是同步 + sync trait，而 agent run 本质异步、事件驱动；让 Rust 拥有进程 spawn/wait/重发提示是一次大重架构，风险高。
- **方案 B（本计划默认）**：承认循环就在前端驱动（已跑通）——把循环策略**抽成纯函数 TS 模块并 node 单测**（恢复可测性，不靠 Rust 往返），补 wall-clock 超时，统一阈值，**删除/降级**死代码 `loop_engine.rs`+`validation.rs`。小、低风险、贴合现状。

本计划按 **B** 执行。方案 A 已排除为本轮非目标：若未来要把循环下沉 Rust，需要单独立项，而不是在本收尾修复中翻转。

## 目标

1. **单一真相源（覆盖两个生产循环）**：implement/repair 循环的"决策（继续/停/升级）+ no-progress 指纹 + 预算（迭代 + wall-clock）"只存在于**一处**、可单测（纯函数 TS 模块）。**`SessionPane.autoLoop`（implement）与 `TestingPane.autoMode`（testing 自动修复）都改用它**——否则只消除了一半重复。
2. **有界超时**：生产循环有 wall-clock 预算；某 run 挂住超时后**升级给人**（不无限停滞）。
3. **统一阈值**：no-progress / max-attempt 常量只定义一次，前后端不再各执一份。
4. **不留死代码**：`loop_engine.rs`/`validation.rs` 要么真正接进生产（方案 A），要么删除/降级为共享类型（方案 B 默认删）。
5. 既有正确部分（session_capture / context_builder / resume / trace 字段）**不回退**。

## 非目标

- 不做方案 A 的 Rust 驱动重架构（除非待确认 #1 翻转）。
- 不动 `session_capture.rs` / `context_builder.rs` / resume invocation / CommandRun 新字段——这些 review 判定正确。
- **不重建服务端 validation 信号**（`validation.rs` 的 split-log 读取是前端无法复刻的 fs 能力）——有意取舍：前端用服务端已算的 `run.errorSummary` + commandLogs 足够，故 `validation.rs` 可直接删。
- 不改 testing-cockpit（2026-06-24）的 PTY/管道双路与 acceptance gate 语义。
- 不引入新依赖/框架。

## 成功标准

1. 循环策略集中在**一个纯函数 TS 模块**（如 `src/utils/loopPolicy.ts`），含：`decide(state, signal, budget)`、no-progress 指纹比较、预算（迭代 + wall-clock）；`SessionPane.autoLoop` 改为调用它，不再内联策略。
2. 该模块有 **node 单测**覆盖：通过即停、失败重试、达迭代上限升级、**超时升级**、no-progress 升级——与原 Rust `loop_engine` 四+一退出路径对齐。
3. 生产循环：制造一个**不返回的 run**，循环在 wall-clock 预算后**升级给人**（UI 提示 + 持久化 `task.loopTrace[].terminationReason=timeout`），不永久停滞。
4. no-progress / max-attempt 阈值**只定义一次**；前端与（若保留的）任何 Rust 常量一致。
5. `loop_engine.rs`/`validation.rs`：按方案 B **删除**（含其 cargo 测试），或若决策为 A 则接线；不存在"测过但不跑"的模块。
6. `pnpm test` / `pnpm build` / `cargo test` 全绿；M6 既有 canary（timeline / repair-loop / Auto repair / `pnpm dogfood:verify -- --strict`）不回退。

## 完成状态（关键事实）

- `src/utils/loopPolicy.ts` 是自动循环策略单一真相源，集中定义 `MAX_AUTO_REPAIR_ATTEMPTS=3`、`NO_PROGRESS_LIMIT=3`、`LOOP_WALL_CLOCK_MS=480_000`，并提供 validation/repair 决策、fingerprint、timeout 与升级文案。
- `SessionPane.autoLoop` 已补 `startedAtMs`、改用 `loopPolicy`，并通过独立 ticker 在 wall-clock timeout 时升级给人并停止当前 loop run。
- `TestingPane.autoMode` 已从一次性 `autoHandledRef` 触发改为同一套有界 repair → validation 循环，保留 manual 修复路径与 acceptance gate 语义。
- `stopCommandRun(runId, "timeout")`、`stop_command_run`、`CommandFinishedEvent`、reducer 和 `tasks::finish_command_run` 已贯通 timeout reason；loop trace 最终可记录 `terminationReason=timeout`。
- `src-tauri/src/loop_engine.rs` 与 `src-tauri/src/validation.rs` 已按方案 B 删除；`analyze_error` 已回退为 `command_runner` 私有函数。
- 验证通过：`pnpm test`、`pnpm build`、`cargo test`、`pnpm dogfood:verify -- --strict`。

## 里程碑

> 当前仓库已进入 M1 半实施状态。继续实施前先修构建红线，再完成 TestingPane 收编；M3 删除按已定方案 B 执行，不再等待 A/B 决策。

### M0：修复当前半实施构建红线
1. 在 `SessionPane.handleCompleteTodo` 创建 `AutoImplementationLoop` 时补 `startedAtMs: Date.now()`，保证 `LoopProgress` 的 timeout 计算有真实起点。
2. 跑 `pnpm build`，确认当前半迁移不会让 TSC/Vite 失败；再继续 TestingPane 改造。

### M1：抽出可测的循环策略模块（单一真相源，服务两个 pane）
1. 完成/保留 `src/utils/loopPolicy.ts`：纯函数按实际形态拆成 `decideAfterValidation` / `decideAfterRepair` / `failureFingerprint` / `isTimedOut` / `escalationNotice`；常量集中（`MAX_AUTO_REPAIR_ATTEMPTS`、`NO_PROGRESS_LIMIT`、`LOOP_WALL_CLOCK_MS`）。
2. `SessionPane.autoLoop` 改为调用 `loopPolicy`，删除内联策略；修复 `startedAtMs` 后保持行为对齐现状（不回退）。
3. **`TestingPane.autoMode` 也改用 `loopPolicy`**：把"一次性触发"升级为同一套有界循环（顺带获得多轮预算 / no-progress / 超时），删除其内联 `autoHandledRef` 即兴逻辑（保留 Manual 路径与去重语义）。
4. 保留 `tsconfig.test.json` 对 `loopPolicy.ts` 的纳入；保留并扩展 `tests/unit/loopPolicy.test.cjs`，覆盖通过、失败重试、达迭代上限、超时、no-progress、repair agent failed。

### M2：生产循环补 wall-clock 超时
1. autoLoop 记录 `startedAtMs`；在 `loopPolicy` 里判 `elapsedMs >= LOOP_WALL_CLOCK_MS → escalate("timeout")`。
2. **超时必须由独立 ticker 驱动**（`setInterval`/`setTimeout`），**不能只挂在 run-completion 事件上**——挂住的 run 不产生完成事件，正是要兜的场景。ticker 周期评估超时；超时则升级给人、UI 提示，并停止或标记当前 running loop run。
3. **补 timeout reason 持久化通道（M2 必做，不再隐含）**：扩展 `stop_command_run`/`stopCommandRun` 支持可选 `terminationReason`，或新增专用 timeout finish/trace command；`CommandFinishedEvent` 与前端 reducer 必须透传 `terminationReason`；`tasks::finish_command_run` / `command_loop_trace_entry` 最终要持久化 `termination_reason=timeout`，而不是只落 `cancelled`。
4. （可选稳健）`command_runner` 给 `loop_step`/`validation`/`agent_action` 类 run 加可配置 run 级超时，超时 kill + 标记 failed，使挂住的 run 也能驱动循环前进。见待确认 #2。

### M3：清理死代码 + 统一阈值
1. 方案 B：**删除** `loop_engine.rs` + `validation.rs` 及其 cargo 测试与 `lib.rs` 的 `pub mod` 声明；确认无残留引用（已验证：生产零外部引用，`LoopTraceEntry` 与死的 `loop_engine::LoopTraceEvent` 解耦，删引擎不伤 timeline）。
2. 删 `validation.rs` 后 `analyze_error` 仅 command_runner 自用，把可见性从 `pub(crate)` **回退为私有**（或注明保留无害）。
3. 统一阈值：常量只在 `loopPolicy.ts` 定义一份；删除 SessionPane/TestingPane 旧常量与 Rust 旧默认的重复（注意原 Rust `no_progress_limit=2` 与前端 `=3` 不一致，统一时明确取值）。

### M4：测试与验证
见验证策略。

## 具体任务清单

- [x] M0：补 `SessionPane` 创建 loop 时的 `startedAtMs`，先让 `pnpm build` 过
- [x] M1：`loopPolicy.ts` 纯函数策略 + 常量集中
- [x] M1：`SessionPane.autoLoop` 改用 `loopPolicy`，删内联策略（已部分完成；待补 `startedAtMs` + build 验证）
- [x] M1：**`TestingPane.autoMode` 改用 `loopPolicy`**（一次性触发 → 有界循环；删内联 autoHandledRef 即兴逻辑）
- [x] M1：`tsconfig.test.json` + `tests/unit/loopPolicy.test.cjs`（五条退出路径含超时）
- [x] M2：autoLoop wall-clock 预算 + **独立 ticker** 超时升级 + trace `terminationReason`
- [x] M2：扩展 timeout reason 持久化通道（`CommandFinishedEvent`/reducer/`stop_command_run` 或专用 trace command）
- [ ] M2（可选）：command_runner run 级自动超时（未做；本轮实现 UI ticker + `stopCommandRun(runId, "timeout")` kill/persist 通道）
- [x] M3：删除 `loop_engine.rs`+`validation.rs`+测试+`lib.rs` 声明（方案 B）
- [x] M3：`analyze_error` 可见性回退为私有
- [x] M3：阈值统一为单一来源（明确 no-progress 取值，化解 2 vs 3）
- [x] M4：`pnpm test`/`pnpm build`/`cargo test` 全绿 + M6 canary 不回退
- [x] 更新 `docs/PLANS.md` 索引（随本计划已完成）

## 风险

1. **删 Rust 模块误删被引用项**：`validation.rs` 的 `analyze_error`(其实在 command_runner)、`VerificationSignal` 等。缓解：删前 `grep` 确认零引用；`cargo check` 兜底。
2. **行为回退**：把内联策略搬到模块时改变现有自动循环语义。缓解：先做等价重构（常量/逻辑 1:1 搬运）再补超时；M6 canary 对照。
3. **超时误杀正常长 run**：实现/验证有时本就耗时。缓解：WALL_CLOCK 取保守值（参考原 Rust 480s 量级，且可配置）；超时是"升级给人"而非静默丢弃。
4. **半实施状态造成构建红线**：当前 M1 已部分修改 `SessionPane`，`pnpm test` 不覆盖该 TSX 类型错误。缓解：把 `pnpm build` 前移到 M0，先修 `startedAtMs` 再继续。
5. **timeout 只停 UI、不落持久 trace**：若只做前端 ticker，用户能看到 timeout，但历史记录仍可能是 running/cancelled。缓解：M2 必做 terminationReason 持久化通道。

## 待确认问题

1. **方案 A 还是 B？** —— ✅ 已定（2026-06-25）：**方案 B**（前端驱动 + 可测 `loopPolicy` 模块 + 删死代码）。理由：A 让同步 Rust 引擎驱动异步 agent run 是大重架构、风险高；B 的纯函数策略模块同样可测，且贴合 Loom 交互式定位。`loopPolicy.ts` 写成纯逻辑，保留将来 L3 自主化时移植回 Rust 的可能。
2. **超时是否要 command_runner run 级 kill？**（M2）纯前端计时即可升级，但挂住的进程仍在跑；加 run 级超时更稳健但动 Rust。建议本轮至少实现 `stopCommandRun(runId, "timeout")` 级别的 kill + timeout reason 持久化，完整 run 级自动 timeout 可后续加。
3. **WALL_CLOCK_MS 默认值？** ✅ 暂定 `480_000` ms（8 分钟），与原 Rust 默认量级一致；后续可按阶段/项目配置。
4. **SessionPane 与 TestingPane 的循环合一到什么程度？**（review 补充）建议**共用 `loopPolicy` 内核**（预算/no-progress/超时一致），但保留各自语义差异（implement = todo→validate→repair 多轮；testing = validation 失败 → 修复一轮再验）。即"同一策略内核、不同 spec"，而非强行同一流程。

## 验证策略

1. `pnpm test`：新增 `loopPolicy.test.cjs` 覆盖 通过/失败重试/达上限/超时/no-progress 五路径；既有前端测试不回退。
2. `cargo test`：删模块后全绿、无悬挂引用（`cargo check` 干净）。
3. `pnpm build`：tsc+vite 通过。
4. 手动验证（`scripts/start-local.sh`，结束 `stop`）：
   - 造一个**不返回的 validation/repair run** → 确认循环在 wall-clock 后升级给人、trace 记 `terminationReason=timeout`、UI 有提示、无永久停滞。
   - 正常失败 → 确认有界重试（≤N）后升级，阈值与模块常量一致。
   - **两个 pane 都验**：SessionPane（implement 自动循环）与 TestingPane（testing 自动修复）均走同一 `loopPolicy`，预算/no-progress/超时行为一致；TestingPane 不再是无界一次性触发。
   - 自动循环整体行为与重构前一致（除新增的超时升级 + TestingPane 获得的有界性）。
   - M6 canary（timeline / repair-loop / Auto repair / `pnpm dogfood:verify -- --strict`）全绿。
