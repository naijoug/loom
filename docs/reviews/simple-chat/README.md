# 简化 CLI Chat 变更清单

- 基线提交：`3dd9a9f`；之前的协议样本提交为 `7e1dfcb`。
- `frontend-deletions.json`、`backend-deletions.json` 列出本次移除的旧工作流文件。
- `reachability.json` 记录 `src/main.tsx` 与 `src-tauri/src/lib.rs` 的模块可达性，以及保留冻结的 Workbench 文件。
- 清单描述重构基线，不包含之后尚未提交的 UI 打磨。工作区运行 `node scripts/simple-chat-reachability.mjs` 会重新生成当前工作区的报告，需要重新核对基线标记。
- 删除的是代码与独占测试；旧项目的 `.loom/` Task 数据没有被删除、移动或迁移。

验收层级、已通过场景及用户接手项见 [验收记录](../../dogfood/2026-10-20-simple-chat-acceptance.md)。
