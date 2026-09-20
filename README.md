# Loom

Loom 是基于 Tauri、React 和 Rust 的本地编程 Agent 工作台。当前优先交付可靠的单 Agent Chat：选择项目、检测本机 CLI、建立会话、查看流式回复、停止和续聊。CLI 可以调用远端模型，“本机调用”不等于离线推理。

## 当前交付范围

- 至少一个真实本机 Agent 完整通过多轮、停止和重启验收；其他 Agent 按实际能力与证据展示支持状态。
- ChatSession 与 Task 分离，普通聊天不要求多 Agent 讨论、计划确认或实施 Review。
- 当前 Ask 是每回合发送前的写入授权；尚无逐工具审批传输。权限限制与现有缺口见 [Chat 契约](docs/architecture/chat-contracts.md)。
- Chat 重构仍在进行。网页预览、fixture 或历史工作流验收不能代替当前真实桌面 Chat 验收。

## 启动与验证

```bash
pnpm install
scripts/debug.sh stop
scripts/debug.sh desktop
# 验证结束后
scripts/debug.sh stop
```

仅浏览器预览使用 `scripts/debug.sh web`，固定端口 1420。开发检查按 [验证矩阵](docs/testing.md#开发变更验证矩阵) 选择；里程碑完整门禁为 `pnpm check`，桌面构建另跑 `pnpm tauri build --no-bundle`。

## 高级 Task 工作流

Task 保留需求讨论 → 实施与独立 Review → 调试验收 → 修复循环：多 Agent 起草与互评、主 Agent Todo 实施、blocker 门禁、日志与附件、Git baseline 归因和 JSON/Markdown 总结。这些能力有独立的历史验收记录，不是 Chat 使用前提。

## 文档入口

- [现行需求与 Chat 成功标准](docs/requirements.md)
- [架构与模块边界](docs/architecture.md)
- [Chat 当前契约与重构目标](docs/architecture/chat-contracts.md)
- [Agent Adapter](docs/agent-adapter.md)
- [高级 Task 状态机](docs/task-state-machine.md)
- [安全策略](docs/security-policy.md)
- [开发指南与计划规范](docs/development.md)
- [测试与验收](docs/testing.md)
- [Agent 行为验收案例](docs/guides/agent-behavior-evaluation.md)
- [当前计划与历史索引](docs/PLANS.md)
- [Beta 发布资料](docs/release/README.md)（仅发布准备时查阅）
- [高级工作流历史需求审计](docs/requirements-audit.md)与[历史验收报告](docs/dogfood/complete-version-2026-07-24.md)
