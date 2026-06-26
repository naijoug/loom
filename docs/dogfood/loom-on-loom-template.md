# Loom-on-Loom 自举验证模板

## 用途

用于记录 Loom 用自身流程交付一个小型、可回滚、可验证的真实迭代。该模板服务于 Loop Engineering 重构的 M6 验证：证明实现回路、确定性验证、修复循环、trace 和人工闸门可以在 Loom 仓库自身上跑通，而不是只依赖单测。

## 任务边界

- 任务标题：
- 需求来源：
- 计划文档：
- 当前里程碑：
- 负责人 Agent：
- 验证命令：
- 回滚边界：

## 选择标准

- 范围小：单个 UI/状态/文档/测试切片，不跨多个产品方向。
- 可证明：至少一个确定性命令能证明成功或失败，例如 `pnpm test`、`pnpm build`、`cargo test --manifest-path src-tauri/Cargo.toml`。
- 可回滚：不涉及发布、凭据、外部账号、生产数据或不可逆迁移。
- 可审计：所有 agent action、validation、repair、人工接管点都必须落入任务记录和 loop trace。

## 执行记录

### Planning

- 计划 Agent：
- Review Agent：
- 关键分歧：
- 最终计划：
- 人工决策：

### Implement

- Todo：
- Agent command run：
- Session id：
- Resume command：
- Context summary：
- Compact summary：

### Validation

- Validation command：
- Exit code：
- stdout log：
- stderr log：
- Failure fingerprint：
- Acceptance evidence：

### Repair Loop

| Attempt | Trigger | Agent action | Validation result | Termination reason |
|---|---|---|---|---|
| 1 |  |  |  |  |
| 2 |  |  |  |  |
| 3 |  |  |  |  |

### Trace Audit

- loop_id：
- iteration count：
- no-progress signal：
- budget/exhaustion status：
- token_usage（如可用）：
- human escalation：

## 验收结论

- 结果：通过 / 失败 / 升级人工
- 通过证据：
- 未验证项：
- 后续修复：

