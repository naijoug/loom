# Loom Agent 指南

## 项目背景

Loom 是一个基于 Tauri 的桌面端应用，用于编排多个本地编程 Agent 工具，并通过结构化开发流程完成需求讨论、实施 Review、调试验收和修复循环。

产品需求位于 `docs/requirements.md`。除非用户给出更新指令，否则该文件是产品范围的依据。

## 技术方向

- 桌面端外壳：Tauri。
- 涉及本地进程、文件系统、项目分析、日志流、持久化的能力，应优先放在 Rust/Tauri commands 中。
- 涉及流程展示、表单、任务导航、日志查看、人工反馈的能力，应放在前端 Web UI 中。
- MVP 优先跑通端到端本地开发闭环，再考虑插件市场、云同步、团队账号或高级浏览器自动化。

## 计划文档

- 实施计划写入 `docs/plans/YYYY-MM-DD/HH:mm-<topic>.md`。
- 使用当前本地日期和时间生成目录与文件名。
- 计划文档默认使用中文。
- 计划需要能被独立执行和独立 Review。
- 每份计划应包含：目标、非目标、成功标准、当前状态、里程碑、具体任务、风险、待确认问题和验证策略。
- MVP 计划默认使用 `docs/plans/YYYY-MM-DD/HH:mm-mvp.md`，除非用户指定更具体的主题。
- `docs/PLANS.md` 是计划文档索引；每次生成新的计划文档时，除了写入 `docs/plans/YYYY-MM-DD/HH:mm-<topic>.md`，必须在同一轮变更中把该计划加入 `docs/PLANS.md`。
- 更新 `docs/plans/` 下已有计划时，也必须同步更新 `docs/PLANS.md` 中对应条目的摘要或排序。
- `docs/PLANS.md` 按 `docs/plans/YYYY-MM-DD/` 日期倒序排列，并用 `## YYYY-MM-dd` 分隔每天的计划。
- 每天内的计划按文件名时间倒序排列，列表格式为：
  - `HH:mm-xx.md`
    > 用一句话总结这个计划干了什么。

## 开发约定

- 优先拆成小而可逆的里程碑。
- 不要在没有明确 MVP 理由时添加新依赖。
- 所有 Agent 调用和调试命令都应保留可见的命令执行记录与日志。
- Agent 集成必须放在适配器接口之后，确保 Codex、Claude Code、OpenClaw、Hermes 和未来工具可以扩展，而不需要重写编排逻辑。
- 需要持久化任务历史、Agent 调用、命令运行、Review 结果、用户反馈和验证证据。

## 本地预览

- 启动 Web/Tauri UI 预览前，必须先关闭上一轮 Loom/Vite 预览，避免多个 `vite` 进程占用不同端口。
- 本地启动优先使用 `scripts/dev.sh`：默认启动 Tauri 桌面，`web` 启动浏览器预览，`stop` 清理当前仓库的预览/桌面 dev 进程。
- 默认使用 `scripts/preview.sh` 启动和停止预览；固定端口为 `1420`，日志为 `/tmp/loom-preview-vite.log`，PID 文件为 `/tmp/loom-preview-vite.pid`。
- 需要手工启动时，也必须使用固定命令形态：`pnpm dev --host 127.0.0.1 --port 1420 --strictPort`，并先确认同一仓库没有旧 Vite 预览进程。
- 验证结束后运行 `scripts/preview.sh stop`，不要遗留后台预览进程。

## 验证

- 声称完成前必须验证。
- 代码变更后，在相关命令存在时运行前端、Rust 和 Tauri 检查。
- 仅文档变更时，验证文件路径、Markdown 可读性，以及与 `docs/requirements.md` 的一致性。

## 设计资源

- UI 设计原稿（如 `.pen` 文件）及其他相关设计资源均统一存放在项目的 `designs/` 目录下。
- 当更新或生成新的设计时，请确保文件保存在该目录中。
