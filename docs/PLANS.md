# 计划文档索引

## 2026-06-05

- 18:47-redesign-impl.md
  > 把 designs/redesign-2026-06 新 IA 与视觉落到现有 Tauri+React 应用：前端重构覆盖项目侧栏、计划聊天室、loom/speaker Linear 看板、状态化任务详情、多终端测试、6 页设置和 token 迁移，并补闭环后端命令、日志分桶、主 Agent 持久化、Tauri devUrl/preview 修复、真实桌面交互验证、command runner 真实进程闭环 smoke、Codex/Claude 真实 CLI smoke、Amp paid-credits 失败路径识别，以及 `pnpm test` / `pnpm smoke` / `pnpm smoke:interaction` / `pnpm smoke:visual` 自我验收路径。

## 2026-05-13

- 09:02-final-shape-roadmap.md
  > 重新规划 Loom 最终形态的剩余功能路线，并将下一步实施重点收敛到 Agent 配置闭环、Review、调试修复、权限审计和任务总结。

## 2026-05-11

- 17:52-real-cli-agent-adapter.md
  > 已实施第一阶段真实 CLI adapter，并在 2026-05-12 补齐真实 CLI smoke、计划确认流程、`docs/plans/YYYY-MM-DD/HH:mm-xxx.md` 输出和多轮前端/Rust 夜间验证记录。

## 2026-05-09

- 15:24-planning-discussion-mvp.md
  > 已完成计划讨论 MVP，并在真实 CLI adapter 切片中补齐 Codex、Claude Code 与 Amp planning 调用。
- 14:01-functional-core.md
  > 将 Loom 从静态 UI 和 spike 验证推进到可登记项目、配置 Agent、创建任务、运行验证命令并持久化日志的功能内核。
- 12:20-ui-refactor.md
  > 基于 Option 4 设计方案重构 Loom MVP 前端 UI，搭建支持深浅主题的高保真左右分栏工作台。
- 10:42-mvp.md
  > 定义 Tauri 桌面端 MVP 的端到端开发闭环，包括多 Agent 规划、实施 Review、调试验收和修复循环。
