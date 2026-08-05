# Loom 任务状态机

任务状态由 `src-tauri/src/task_state.rs` 定义，`tasks.rs` 是持久化入口。前端只发出动作，不自行决定权威状态。

## 主流程

```text
drafting_requirements → planning → plan_review → ready_to_implement
→ implementing → reviewing → debugging ↔ fixing → verifying → completed
```

`blocked` 和 `cancelled` 是生命周期状态。暂停用 `TaskLifecycle.paused` 表示，从而保留原阶段；恢复回到精确的 `resume_status`。`cancelled` 与 `completed` 为终态。

## 动作与门禁

| 动作 | 前置 | 结果 / 额外门禁 |
|---|---|---|
| 开始规划 | 已创建任务 | `planning` |
| 计划产出 | 至少一份可用最终计划 | `plan_review` |
| 确认计划 | 最终计划可解析出 Todo | `ready_to_implement` |
| 开始 Todo | Todo 存在；主 Agent 有 implementation 权限 | `implementing`；首次捕获 Git baseline |
| 完成 Todo | 当前 Todo 为 implementing | 仍有 Todo 时 `implementing`，全部完成时 `reviewing` |
| 进入 Testing | 全 Todo 完成；至少一个成功 Review；Reviewer 与主 Agent 不同；无 open/pending blocker | `debugging` |
| 验证失败 | validation run 非零退出 | `debugging`，可生成 repair context 进入 `fixing` |
| 修复完成 | Agent action 结束 | 返回验证循环 |
| 验证成功 | 最新 validation run 成功 | `verifying` |
| 人工验收 | 当前为 verifying，且后端确认最新 validation/legacy validation 成功 | `completed` 并生成交付总结 |
| 暂停 | 非终态 | 停止关联进程，阶段不变，禁止变更动作 |
| 阻塞 | 非终态，需原因 | `blocked`，记录恢复状态并停止进程 |
| 恢复 | paused/blocked | 回到原状态 |
| 取消 | 非终态，需原因 | `cancelled`，停止进程且不可恢复 |

## 命令 intent

- `agent_action`：实施或修复，不可伪造验收通过。
- `validation`：一次性验证；只有该 intent（以及旧任务兼容的 `legacy`）能推进到 verifying/complete。
- `preview`：长时间 PTY 服务，只提供人工调试表面。
- `loop_step`：循环内部证据，不直接越过阶段门禁。

## Review finding

Finding severity 为 `blocker | risk | suggestion | info`；状态为 `open | pending_re_review | accepted_risk | dismissed | superseded`。Blocker 只能修复后重审，或由用户明确记录 accepted risk 与理由，不能静默忽略。

## 恢复语义

重启不会盲目附着未知子进程。孤立 command/review 标记 `interrupted`；用户可重试，或使用已记录的 Codex/Claude 原生 session resume 命令继续。所有历史 run 和 evidence ref 保留。
