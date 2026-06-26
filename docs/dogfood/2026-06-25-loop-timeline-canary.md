# Loom-on-Loom Dogfood：Loop Timeline Canary

- 日期：2026-06-25 13:26 CST
- 计划：`docs/plans/2026-06-25/10:21-loop-engineering-refactor.md`
- 范围：M4 单一 timeline 小切片（`TaskTimeline` + `buildTaskTimelineRows` + reducer live trace 同步）
- 结果：通过（strict canary：timeline + repair-loop evidence）

## 任务边界

- 任务标题：Loop timeline canary
- 需求来源：Loop Engineering 重构计划 M4/M6
- 验证命令：
  - `pnpm test`
  - `pnpm build`
  - `cargo test --manifest-path src-tauri/Cargo.toml`
- 回滚边界：timeline utility/component/tests 和 dogfood 文档；不涉及发布、外部账号、生产数据或不可逆迁移。

## 执行记录

### Planning

- 计划来源：`10:21-loop-engineering-refactor.md` 的 M4/M6 要求。
- 人工决策：选择低风险 timeline canary，不选择工作树隔离或 verifier sub-agent 等 M5 可选项。

### Implement

- 主要实现：
  - `src/utils/taskTimeline.ts`：把 planning runs、agent invocations、plan reviews、decisions、loop trace、command runs、task events 合成单一时间线。
  - `src/components/TaskDetail/TaskTimeline.tsx`：改为渲染统一 row feed。
  - `src/state/reducer.ts`：command start/finish 事件同步当前 task 的 commandRuns、loopTrace 和 validation 状态，避免 UI 等待 reload。
  - `tests/unit/taskTimeline.test.cjs`、`tests/unit/reducer.test.cjs`：覆盖 timeline 合并、trace 去重、live trace 更新。
- Agent command run：`run-dogfood-codex-audit`（read-only Codex audit of the dogfood verifier/report）
- Repair command run：`run-dogfood-repair-loop-codex-repair`（real Codex repair action from the repair-loop canary）
- Session id：`019efd45-cae1-7ca0-a5f4-2ccad09c9901`
- Resume command：`codex resume 019efd45-cae1-7ca0-a5f4-2ccad09c9901`
- 说明：主要实现仍由当前 Codex 线程完成，未通过 Loom UI 的 command_runner 启动；本 agent run 是后置 read-only audit，用于补强真实 agent-action 证据但不冒充完整 UI 端到端执行。

### Validation

| Command | Result | Evidence |
|---|---|---|
| `pnpm test` | pass | 55 tests passed |
| `pnpm build` | pass | TypeScript + Vite build passed；保留既有 large chunk warning |
| `cargo test --manifest-path src-tauri/Cargo.toml` | pass | 126 passed / 1 ignored |

### Repair Loop

未触发自动修复。验证一次通过。

| Attempt | Trigger | Agent action | Validation result | Termination reason |
|---|---|---|---|---|
| 0 | initial validation | timeline merge + live trace sync | pass | succeeded |
| 1 | dogfood evidence audit | read-only Codex audit of verifier/report | pass | succeeded |

### Trace Audit

- loop_id：`dogfood-loop-timeline-canary`
- iteration count：1 validation iteration + 1 read-only agent audit
- no-progress signal：无
- budget/exhaustion status：未耗尽
- token_usage：未捕获（当前实现字段保留为 nullable）
- human escalation：无

### Strict Repair Evidence

默认 dogfood task record 还附加了 repair-loop canary 的严格证据链：

| Step | Command run | Result | Evidence |
|---|---|---|---|
| failed validation | `run-dogfood-repair-loop-validation-fail` | failed, exit `42` | `error: repair-loop sentinel missing at .loom/dogfood/repair-loop-sentinel.txt` |
| repair agent | `run-dogfood-repair-loop-codex-repair` | succeeded | Codex created `.loom/dogfood/repair-loop-sentinel.txt` only |
| validation rerun | `run-dogfood-repair-loop-validation-pass` | succeeded | `repair-loop sentinel present` |

## 本地 Loom 记录

为了让 Loom UI 可以回放该 canary，本轮还创建了一个本地、git-ignored 的任务记录：

- `.loom/loom.json`
- `.loom/tasks/task-loop-timeline-canary.json`

该记录按当前 `Task` schema 写入 command runs、loop trace、failed validation、repair agent action 与 passing validation rerun。

## Desktop UI Replay

- 运行方式：`scripts/start-local.sh desktop`
- UI 动作：通过桌面端 Add Project 选择 `/Users/guojian/Workspace/naijoug/loom`，Tauri runtime 注册并加载真实 `.loom` task record。
- 观察结果：桌面 UI 成功显示 `loom / Loop timeline canary`，证明项目注册和任务读取路径经过真实桌面 runtime。
- 截图证据：`.loom/screenshots/loop-timeline-canary-desktop-load.png`
- 限制：本次 UI 回放证明桌面 runtime 加载 canary task；repair-loop 证据来自 headless real-agent canary，并已并入默认 task record。

## 验收结论

- 通过证据：三条验证命令均通过；timeline row builder 与 reducer live trace 均有单测覆盖；默认 task record 含 failed validation、repair agent action 与 passing validation rerun。
- 机器校验：`pnpm dogfood:verify` 校验本地 `.loom` task record、agent action、desktop UI replay、validation command runs 与 loop trace 是否一致。
- 严格校验：`pnpm dogfood:verify -- --strict` 校验默认 task record 中的 failed-validation + repair-loop evidence。
- 限制：repair-loop 证据是 headless real-agent canary，不声明完整 UI 内创建任务并自动修复的端到端覆盖。
