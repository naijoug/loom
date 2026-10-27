# Loom 架构

## 当前范围（2026-10-20）

当前产品是按项目组织的本机 CLI Chat，实施入口为 [简化 Chat 计划](plans/2026-10-20/01:01-simple-chat-implementation.md)。React 负责交互与投影；Rust/Tauri 负责项目分析、CLI 探测、参数与权限映射、进程管理、日志、持久化与导出。

Task/Board/Planning 的 UI、命令及独占模块已删除。`crates/`、`packages/`、`src/workbench/`、`src-tauri/src/workbench/` 保留冻结；默认应用不挂载这些模块，也不依赖 Bun 或 SDK worker。

Skills 模块自 2026-10-21 开始按 [迁移计划](plans/2026-10-21/06:12-skills-module-migration.md) 实施。用户已明确入口位于 Settings 侧边栏，Skills 不新增顶层视图。当前已接通本地/Git/GitHub API 来源、固定快照、操作记录，以及 copy/symlink 安装、显式更新/卸载、共享指令/wrapper、只读诊断、事务恢复和无窗口 CLI 核心；管理功能继续由 Loom 维护；2026-10-25 用户取消旧数据迁移并授权源 Manager 代码退役，真实账号/桌面隔离验证限制单独保留。契约与状态见 [Skills 契约](architecture/skills-contracts.md)，不挂载冻结 Workbench。

Learn 模块于 2026-10-26 按用户要求新增为顶层视图（入口在 Chat 侧边栏），输入公开 GitHub 仓库与 Agent/模型，生成架构、分级模块 Wiki 和可定位的源码引用。数据只写应用数据目录，Agent 以只读权限在 checkout 中运行。契约见 [Learn 契约](architecture/learn-contracts.md)。

## 生产模块

| 模块 | 职责 |
|---|---|
| `src/App.tsx` | Chat / Learn / Settings；Learn 或设置打开时保留 Chat 挂载、草稿与活动会话 |
| `src/features/chat/ProjectTree.tsx` | 最近项目、按需展开会话、搜索、归档过滤、折叠状态 |
| `ChatPage` / `useChatBridge` / `ChatProjection` | 会话选择、异步归属、序号补流与快照协调 |
| `ChatComposer` / `AgentPicker` / `ModeSwitch` | 发送、停止、IME 保护、斜杠命令、模式和 Agent 选择 |
| `ChatTranscript` / `ToolCallCard` / `MarkdownView` | 纯文本安全渲染、工具 ID 配对投影、差异预览 |
| `src/i18n/` | 英文源字典、受类型约束的简体中文、locale 持久化 |
| `src/features/settings/SettingsPage.tsx` / `src/features/skills/SkillsPanel.tsx` | 设置侧边栏 Skills 板块，来源/搜索/详情/文件、安装到工具、已安装清单和恢复状态；设置切换保留 Chat 挂载 |
| `skills/{catalog,repository,snapshots,operations,commands}.rs` | 有界扫描、来源登记、不可变快照、受控 Git 子进程与 typed IPC；来源移除只取消登记 |
| `agent/{registry,capabilities,diagnostics}.rs` | Agent 调用层：PATH 优先、通用安装目录、命令覆盖、限时版本探测、带缓存的启动前检查（Chat 发送、Learn 开始前）与能力声明；功能模块只经 `agent/mod.rs` 的公开接口调用 |
| `agent/error.rs` | `AgentError`：面向用户的错误统一为“摘要 + 空行 + 原始详情”，前端 `components/common/ErrorText` 解析展示 |
| `agent/invocation.rs` / `chat_context.rs` | CLI 参数、权限、原生续聊和有界历史重放 |
| `agent/runner.rs` / `agent/event.rs` | 通用 runner `agent::run`：启动并登记进程（`ProcessOwner` 由调用方给出）、stdin、超时/取消/输出限额、进程组清理，输出 `AgentEvent`（文本/工具片段/结束）与 `AgentOutcome`；Chat、Learn 都经它启动 Agent CLI |
| `agent/stream.rs` / `agent/tools.rs` | 各 CLI 流式 JSON 的通用解析、工具片段生成与按 `callId` 合并；不破坏 JSON 信封的日志脱敏 |
| `chat/{repository,journal,service}.rs` | v2 journal、幂等接收、停止与恢复 |
| `chat/runtime.rs` / `chat/file_changes.rs` | Chat 薄封装：Chat 进程所有者、可写 Codex 的有界文件差异（作为 runner 的行补充回调），把 `AgentEvent` 映射为 `ChatStreamEvent` |
| `process_supervisor.rs` | 进程组归属、超时、TERM/KILL、应用退出协调 |
| `projects.rs` / `storage.rs` / `settings.rs` | 项目登记、原子写入、设置与旧数据兼容 |
| `diagnostics.rs` / `chat/export.rs` / `app_update.rs` | 有界诊断包、会话导出、应用内更新与草稿恢复 |
| `src/features/learn/` | Learn 研究列表、新建表单、阶段/日志、Wiki 目录与页面、架构图、源码查看器；入口为 Chat 侧边栏 Learn 按钮 |
| `learn/{store,fetch,inventory,source,generate,jobs,commands}.rs` / `git_fetch.rs` | 研究存储与作业登记、加固浅克隆、静态清单、受限源码读取、提示词与输出/引用校验、只读 Agent 作业、typed IPC；Git 加固配置与 Skills 共享 |
| `contracts/tauri-contract.json` | Rust/TypeScript 共同验证的命令、事件和模型样本 |

## 执行与存储

```text
Composer → typed IPC → registered canonical project
         → ChatRepository.begin_once → context / adapter
         → chat::runtime → agent::run / process supervisor
         → journal + per-turn stdout/stderr
         → chat-event → ChatProjection → transcript
```

项目目录仍使用 `.loom/chat/v2/sessions/<id>/events.jsonl` 和派生的 `snapshot.json`。消息、执行快照、退出状态、错误、续聊句柄与幂等回执持久化。迁移仅读取 v1 会话；旧 Task 文件不参与应用加载或诊断导出。

`chat_list_project_groups` 不带路径时只枚举会话目录计数；传入展开项目路径时复用会话仓储读取摘要。某个目录不可读只返回该项目的 `storageError`。列表不读取折叠项目的消息正文。

权限仍由 CLI 执行。`chat_send` 校验登记项目、存储会话路径和准备后的 cwd；前端不构造 CLI 参数。原生续聊配置绑定与补流、幂等、退出边界见 [Chat 契约](architecture/chat-contracts.md)。

## 冻结代码

Workbench 的历史设计与验证记录继续保留，不能作为当前 Chat 的完成证据。`pnpm check:core` 是显式选择的历史内核检查，不在 `pnpm check` 或默认 CI 中执行。冻结代码不保证与当前已收窄的桌面 IPC 兼容；重新启用前需独立计划。
