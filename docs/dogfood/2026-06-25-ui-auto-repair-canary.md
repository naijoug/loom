# Loom-on-Loom Dogfood：UI Auto Repair Canary

- 日期：2026-06-25 14:24 CST
- 计划：`docs/plans/2026-06-25/10:21-loop-engineering-refactor.md`
- 范围：M2/M3/M4/M6 桌面端 Auto 修复闭环
- 结果：通过（real desktop UI Auto repair loop）

## 任务边界

- 任务标题：UI Auto repair sentinel canary
- 任务记录：`.loom/tasks/task-ui-auto-repair-canary.json`
- 验证命令：`node .loom/dogfood/ui-auto-validation.cjs`
- 修复目标：创建 git-ignored 文件 `.loom/dogfood/ui-auto-repair-sentinel.txt`，内容为 `ui-auto-repaired`
- 回滚边界：`.loom/` 本地状态和本报告；不触碰生产数据、外部账号或发布流程。

## Desktop UI Replay

- 运行方式：`scripts/start-local.sh desktop`
- UI 动作：
  - 通过桌面端选择 `loom` 项目。
  - 打开 `UI Auto repair sentinel canary`。
  - 在 implementation pane 切换到 `Auto`。
  - 点击 todo completion 控件触发自动验证。
  - 在 validation pass 后点击 `Accept / Done`。
- 截图证据：
  - `.loom/screenshots/ui-auto-final-auto-enabled.png`
  - `.loom/screenshots/ui-auto-final-after-todo-click.png`
  - `.loom/screenshots/ui-auto-final-validation-passed.png`
  - `.loom/screenshots/ui-auto-final-accepted.png`

## 执行记录

### Validation Failure

- Command run：`run-1782368575711-2`
- 命令：`node .loom/dogfood/ui-auto-validation.cjs`
- 结果：failed，exit code `43`
- 错误摘要：`error: ui-auto repair sentinel missing at .loom/dogfood/ui-auto-repair-sentinel.txt`
- Loop id：`loop-task-ui-auto-repair-canary-todo-ui-auto-repair-sentinel-1782368575710`
- 关键结论：失败后 task status 保持 `reviewing`，未跳入 Testing-pane-only `fixing` 状态。

### Agent Repair

- Command run：`run-1782368575928-3`
- 命令：`codex exec --json --cd /Users/guojian/Workspace/naijoug/loom --sandbox workspace-write ...`
- 结果：succeeded，exit code `0`
- Attempt：`1`
- Session id：`019efd72-29d7-7530-86d3-8643ab33d54e`
- Resume command：`codex resume 019efd72-29d7-7530-86d3-8643ab33d54e`
- 修复结果：仅创建 `.loom/dogfood/ui-auto-repair-sentinel.txt`，内容为 `ui-auto-repaired`。

### Validation Pass

- Command run：`run-1782368638749-4`
- 命令：`node .loom/dogfood/ui-auto-validation.cjs`
- 结果：succeeded，exit code `0`
- 输出：`ui-auto repair sentinel present`
- Task status：`verifying`

### Acceptance

- UI action：点击 `Accept / Done`
- Task status：`completed`
- Acceptance evidence：`run-1782368638749-4`
- Desktop UI evidence event：`.loom/screenshots/ui-auto-final-validation-passed.png`

## 回归修复

- `src-tauri/src/tasks.rs`：`generate_repair_context` 对 loop-bound implementation validation failure 不再强制写入 `fixing`，保留 `implementing/reviewing` 阶段，使 `SessionPane` 的 Auto loop 可以继续驱动 validation rerun。
- `src-tauri/src/command_runner.rs`：command monitor 在进程退出后对 stdout/stderr reader drain 设置上限，避免 descendant 进程继承 pipe 导致已退出命令永久停留在 `running`。

## 机器校验

- Focused backend checks：
  - `cargo test --manifest-path src-tauri/Cargo.toml tasks::`
  - `cargo test --manifest-path src-tauri/Cargo.toml command_runner::tests`
- Dogfood strict check：
  - `pnpm dogfood:verify -- --strict --task .loom/tasks/task-ui-auto-repair-canary.json --report docs/dogfood/2026-06-25-ui-auto-repair-canary.md --validation "node .loom/dogfood/ui-auto-validation.cjs"`

## 验收结论

- 通过证据：真实桌面 UI 触发 Auto validation failure，真实 `agent_action` 修复，自动 validation rerun 通过，并通过 UI acceptance 完成任务。
- 本 canary 覆盖 implementation-loop status gating、repair context packaging、session/resume capture、command-run completion、loop trace persistence 和 Testing pane acceptance gate。
