# Loom 架构

## 当前范围（2026-10-20）

当前产品是按项目组织的本机 CLI Chat，实施入口为 [简化 Chat 计划](plans/2026-10-20/01:01-simple-chat-implementation.md)。React 负责交互与投影；Rust/Tauri 负责项目分析、CLI 探测、参数与权限映射、进程管理、日志、持久化与导出。

Task/Board/Planning 的 UI、命令及独占模块已删除。`crates/`、`packages/`、`src/workbench/`、`src-tauri/src/workbench/` 保留冻结；默认应用不挂载这些模块，也不依赖 Bun 或 SDK worker。

Skills 模块自 2026-10-21 开始按 [迁移计划](plans/2026-10-21/06:12-skills-module-migration.md) 实施。用户已明确入口位于 Settings 侧边栏，顶层仍为 Chat / Settings。当前已接通本地/Git/GitHub API 来源、固定快照、操作记录，以及 copy/symlink 安装、显式更新/卸载、共享指令/wrapper、只读诊断、事务恢复和无窗口 CLI 核心；管理功能继续由 Loom 维护；2026-10-25 用户取消旧数据迁移并授权源 Manager 代码退役，真实账号/桌面隔离验证限制单独保留。契约与状态见 [Skills 契约](architecture/skills-contracts.md)，不挂载冻结 Workbench。

## 生产模块

| 模块 | 职责 |
|---|---|
| `src/App.tsx` | Chat / Settings；设置打开时保留 Chat 挂载、草稿与活动会话 |
| `src/features/chat/ProjectTree.tsx` | 最近项目、按需展开会话、搜索、归档过滤、折叠状态 |
| `ChatPage` / `useChatBridge` / `ChatProjection` | 会话选择、异步归属、序号补流与快照协调 |
| `ChatComposer` / `AgentPicker` / `ModeSwitch` | 发送、停止、IME 保护、斜杠命令、模式和 Agent 选择 |
| `ChatTranscript` / `ToolCallCard` / `MarkdownView` | 纯文本安全渲染、工具 ID 配对投影、差异预览 |
| `src/i18n/` | 英文源字典、受类型约束的简体中文、locale 持久化 |
| `src/features/settings/SettingsPage.tsx` / `src/features/skills/SkillsPanel.tsx` | 设置侧边栏 Skills 板块，来源/搜索/详情/文件、安装到工具、已安装清单和恢复状态；设置切换保留 Chat 挂载 |
| `skills/{catalog,repository,snapshots,operations,commands}.rs` | 有界扫描、来源登记、不可变快照、受控 Git 子进程与 typed IPC；来源移除只取消登记 |
| `agents/config.rs` / `agent_diagnostics.rs` | PATH 优先、通用安装目录、命令覆盖、限时版本探测与能力声明 |
| `agent_adapter.rs` / `chat_context.rs` | CLI 参数、权限、原生续聊和有界历史重放 |
| `chat/{repository,journal,service,runtime}.rs` | v2 journal、幂等接收、输出限额、停止与恢复 |
| `chat/tools.rs` / `chat/file_changes.rs` / `agents/stream.rs` | 工具解析、配对及有界文件差异；不破坏 JSON 信封的日志脱敏 |
| `process_supervisor.rs` | 进程组归属、超时、TERM/KILL、应用退出协调 |
| `projects.rs` / `storage.rs` / `settings.rs` | 项目登记、原子写入、设置与旧数据兼容 |
| `diagnostics.rs` / `chat/export.rs` / `app_update.rs` | 有界诊断包、会话导出、应用内更新与草稿恢复 |
| `contracts/tauri-contract.json` | Rust/TypeScript 共同验证的命令、事件和模型样本 |

## 执行与存储

```text
Composer → typed IPC → registered canonical project
         → ChatRepository.begin_once → context / adapter
         → runtime / process supervisor
         → journal + per-turn stdout/stderr
         → chat-event → ChatProjection → transcript
```

项目目录仍使用 `.loom/chat/v2/sessions/<id>/events.jsonl` 和派生的 `snapshot.json`。消息、执行快照、退出状态、错误、续聊句柄与幂等回执持久化。迁移仅读取 v1 会话；旧 Task 文件不参与应用加载或诊断导出。

`chat_list_project_groups` 不带路径时只枚举会话目录计数；传入展开项目路径时复用会话仓储读取摘要。某个目录不可读只返回该项目的 `storageError`。列表不读取折叠项目的消息正文。

权限仍由 CLI 执行。`chat_send` 校验登记项目、存储会话路径和准备后的 cwd；前端不构造 CLI 参数。原生续聊配置绑定与补流、幂等、退出边界见 [Chat 契约](architecture/chat-contracts.md)。

## 冻结代码

Workbench 的历史设计与验证记录继续保留，不能作为当前 Chat 的完成证据。`pnpm check:core` 是显式选择的历史内核检查，不在 `pnpm check` 或默认 CI 中执行。冻结代码不保证与当前已收窄的桌面 IPC 兼容；重新启用前需独立计划。
