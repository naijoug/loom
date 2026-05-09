# Loom 功能内核实施计划

- **日期**：2026-05-09
- **作者**：Codex
- **状态**：已实施（2026-05-09）
- **最近更新**：2026-05-09 补充状态管理、ID 策略、并发写入、schema 迁移、命令契约和开放问题决议
- **背景**：当前项目已经完成高保真 UI 架子和 Tauri spike，但还没有真实的项目、Agent、任务、命令执行和持久化功能。

## 目标

把 Loom 从“静态 UI + spike 验证”推进到第一条真实可用的本地开发闭环：用户可以登记项目路径，识别项目脚本，配置或使用 dummy Agent，创建任务，保存计划和事件，运行本地验证命令，并在 UI 中看到实时日志与命令结果。

这一步的目标不是完成全部多 Agent 编排，而是先打通后续 MVP 所依赖的功能内核和数据流。

## 非目标

- 不继续扩展纯静态 UI 页面，除非它服务于真实数据接入。
- 不引入新依赖；文件选择器先用路径输入，不接入 Tauri dialog 插件。
- 不接入真实 Codex / Claude / Hermes 交互式终端协议，先实现 dummy adapter 与简单 CLI adapter 契约。
- 不实现完整自动修复循环；本计划只做到可记录失败、错误摘要和下一轮修复上下文所需数据。
- 不实现云同步、团队账号、插件市场或复杂权限沙箱。

## 成功标准

- `src/App.tsx` 渲染的数据来自应用状态或 Tauri command，而不是硬编码 mock。
- 用户可以输入项目路径并保存最近项目记录。
- 后端可以分析一个 Node/Tauri/Rust 项目，返回识别到的技术栈、脚本、Git 状态和建议命令。
- 用户可以新增、启用、禁用 Agent 配置；内置 dummy Agent 可用于无外部 Agent 环境的本地验证。
- 用户可以创建任务，保存需求、当前阶段、最终计划草稿和事件历史。
- 用户可以运行一条项目验证命令，实时看到 stdout/stderr，并保存退出状态。
- 任务历史中能关联 Agent 输出、命令运行、日志摘要和人工反馈。
- `pnpm build`、`cargo check`、`cargo test` 通过；手动验证完成“项目 -> 任务 -> dummy 规划 -> 命令运行 -> 日志记录”流程。

## 当前代码 Review 结论

- `src/App.tsx`：只组合 `ThemeProvider`、`AppLayout`、`Sidebar`、`Header`、`WorkspaceSplit`，没有应用级任务状态或真实数据装载。
- `src/components/Sidebar/Navigation.tsx`、`src/components/Header/Header.tsx`、`src/components/Workspace/ImplementationPane.tsx`：项目、Agent、流程和实施消息均为静态 mock。
- `src/components/Workspace/DebugPane.tsx`：保留了健康检查和 spike 控制，但同时混有静态终端示例日志，需要在功能接入后删除 mock 日志。
- `src/hooks/useTauriBridge.ts`：已有 `health_check`、`start_spike_run`、`stop_spike_run` 调用和 `loom://spike-log` 监听，可作为通用命令日志桥的前身。
- `src/domain/*.ts`：已有 `AgentConfig`、`Task`、`CommandRun`、`ProjectSummary` 等类型雏形，但缺少与后端 DTO 的成体系映射、任务事件细分和持久化字段。
- `src-tauri/src/lib.rs`：只注册健康检查和 spike runner，尚无项目、Agent、任务、存储、真实命令执行模块。
- `src-tauri/src/spike_runner.rs`：已经验证 `tokio::process`、stdout/stderr 读取、事件推送和进程组终止思路，可以升级为通用 `command_runner`，再删除 spike 专用入口。
- `package.json`：仅有 `dev`、`build`、`preview`、`tauri`，没有前端测试脚本；本阶段不新增测试框架，先以类型检查和 Rust 单元测试兜底。
- 验证结果：2026-05-09 已运行 `pnpm build` 通过，`src-tauri/ cargo check` 通过。

## 实施原则

- 后端优先承载本地文件系统、进程执行、项目分析、持久化和日志采集。
- 前端只负责流程展示、表单输入、状态导航、日志查看和人工反馈。
- 先实现能被 CI 和本机稳定验证的 dummy adapter，再接真实 CLI adapter。
- 所有状态变化都写成事件，避免后续修复循环缺少审计记录。
- 从 spike 迁移到正式模块时保留可回滚路径：先并行新增正式 command，再删除 spike UI 和旧 command。

### 关键技术决策（2026-05-09 定稿，实施期间不再讨论）

- **前端状态方案**：M1 使用 React `Context + useReducer`，不引入 Zustand/Redux 等新依赖；状态切片按 `app / projects / agents / tasks / commandRuns` 划分，单一 reducer 入口。
- **ID 生成策略**：所有持久化 ID（`taskId`、`runId`、`projectId`、`agentId`）由后端生成，统一格式 `<prefix>-<unix-ms>-<seq>`（例如 `task-1715241600000-1`）；不引入 `uuid` crate，复用 spike 已有的 `AtomicU64` 计数器思路。
- **持久化并发写入**：每个 `<task-id>.json` 在内存中持有一个 `tokio::sync::Mutex`，写入采用“同目录临时文件 + `rename`”原子替换；同任务事件追加串行，跨任务并发。不引入 `tempfile` crate，临时文件名使用 `<target>.tmp-<unix-ms>-<seq>`。
- **schemaVersion 不匹配行为**：读到不识别的 `schemaVersion` 时，将原文件备份为 `<name>.bak-<unix-ms>`，然后按当前版本初始化空记录并在 UI 提示；不做自动迁移。
- **`.loom/` 位置**：默认写入用户项目根目录，便于审计和随项目携带；首次写入前在 UI 中显式确认并把路径写入 `recent-projects.json`。
- **危险命令策略**：本阶段只记录和展示，不阻止；阻止列表留待自动修复阶段。
- **CLI adapter 协议**：第一批仅支持 stdin prompt 和 prompt-file 两种输入，不做 PTY；交互式会话留待后续阶段。
- **命令执行输入契约**：正式 `command_runner` 接收结构化 `{ program, args, cwd, taskId? }`，展示层可拼接为命令行文本；仅在后续确有需要时再增加显式 shell 模式。
- **全局配置目录**：通过 Tauri `app_data_dir` 解析 `<app-data>/loom/`，首次使用时创建目录；`agents.json`、`recent-projects.json` 和 `preferences.json` 的读写统一走 `storage.rs`。

## 里程碑

### M1 — 应用状态和后端模块边界

**结果**：前端有真实的顶层 App 状态模型，后端拆出项目、Agent、任务、存储、命令执行模块骨架。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 1.1 | 定义前端应用状态（`Context + useReducer`，切片：app/projects/agents/tasks/commandRuns）和阶段派生逻辑 | `src/state/AppStateContext.tsx`, `src/state/reducer.ts`, `src/domain/app.ts`, `src/domain/task.ts`, `src/App.tsx` | 无 | `pnpm build` 通过；Header 阶段由任务状态派生 |
| 1.2 | 新增后端模块骨架并注册 Tauri commands；统一解析 `<app-data>/loom/` 全局配置目录 | `src-tauri/src/lib.rs`, `src-tauri/src/projects.rs`, `src-tauri/src/agents.rs`, `src-tauri/src/tasks.rs`, `src-tauri/src/storage.rs`, `src-tauri/src/command_runner.rs` | 无 | `cargo check` 通过；`health_check` 不回退 |
| 1.3 | 定义 Rust DTO 与前端类型字段对齐 | `src-tauri/src/models.rs`, `src/domain/*.ts` | 1.1, 1.2 | camelCase 序列化与 TypeScript 类型一致 |

### M2 — 本地项目登记与项目分析

**结果**：用户可以输入项目路径，后端验证目录并返回项目摘要与建议命令。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 2.1 | 实现项目路径验证、最近项目保存，以及 `schemaVersion` 校验（不匹配时备份原文件并初始化） | `src-tauri/src/projects.rs`, `src-tauri/src/storage.rs` | 1.2 | 有效目录返回 `ProjectSummary`；无效路径返回可展示错误；schema 不匹配生成 `<name>.bak-<unix-ms>` |
| 2.2 | 实现项目分析器：识别 `package.json` scripts、`Cargo.toml`、`src-tauri/tauri.conf.json`、Git 仓库 | `src-tauri/src/projects.rs` 或 `src-tauri/src/project_analyzer.rs` | 2.1 | 在当前 Loom 仓库能识别 React/Vite/Tauri/Rust、`pnpm build`、`cargo check` |
| 2.3 | 接入项目选择 UI，替换 Sidebar 项目 mock | `src/components/Sidebar/*`, `src/features/projects/*` | 2.1 | 输入当前仓库路径后 Sidebar 显示真实项目 |

### M3 — Agent 配置与 dummy adapter

**结果**：应用可以保存 Agent 配置，并在没有外部 Agent CLI 时用 dummy Agent 生成确定性规划/Review 输出。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 3.1 | 实现 Agent 配置 CRUD 和命令可用性检查 | `src-tauri/src/agents.rs`, `src-tauri/src/storage.rs` | 1.2 | 可新增 CLI Agent；不存在的命令显示 unavailable |
| 3.2 | 内置 dummy Agent 配置和 deterministic 输出 | `src-tauri/src/agents.rs`, `src-tauri/src/agent_adapter.rs` | 3.1 | dummy planning 输出包含目标、风险、验证建议 |
| 3.3 | 接入 Agent 列表 UI，替换 Sidebar Agent mock | `src/components/Sidebar/Navigation.tsx`, `src/features/agents/*` | 3.1 | 新增/禁用 Agent 后 UI 状态刷新 |

### M4 — 任务模型、事件历史和计划草稿

**结果**：用户可以创建任务，保存需求和最终计划草稿，任务阶段驱动 Header 和 Workspace。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 4.1 | 扩展任务模型：计划、命令运行、Agent 调用、反馈、摘要引用 | `src/domain/task.ts`, `src-tauri/src/models.rs` | 1.3 | TypeScript 与 Rust DTO 字段一致 |
| 4.2 | 实现任务 CRUD 和事件追加（每任务一个 `tokio::sync::Mutex`，写入用同目录临时文件 + `rename` 原子替换） | `src-tauri/src/tasks.rs`, `src-tauri/src/storage.rs` | 4.1 | 创建任务后磁盘中能读回完整记录；并发追加事件不丢失、不损坏文件 |
| 4.3 | 用任务状态驱动 Header 流程条和 ImplementationPane | `src/components/Header/*`, `src/components/Workspace/ImplementationPane.tsx`, `src/App.tsx` | 4.2 | 从 drafting 到 planning 的状态变化会刷新 UI |
| 4.4 | 运行 dummy planning Agent 并写入任务事件 | `src-tauri/src/agents.rs`, `src-tauri/src/tasks.rs`, `src/features/planning/*` | 3.2, 4.2 | 同一任务中能看到需求、dummy 输出和最终计划草稿 |

### M5 — 正式命令执行器和日志流

**结果**：把 spike runner 升级为可配置命令执行器，支持项目工作目录、实时日志、退出状态和停止。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 5.1 | 从 `spike_runner` 抽象通用 `CommandRegistry` 和 `start_command_run` / `stop_command_run`；输入为 `{ program, args, cwd, taskId? }`，runId 使用 `run-<unix-ms>-<seq>` 格式 | `src-tauri/src/command_runner.rs`, `src-tauri/src/lib.rs` | 1.2 | 可以运行 `pnpm build` 或 `cargo check` 并返回 runId |
| 5.2 | 将日志事件改为 `loom://command-log` 并包含 `taskId`、`runId`、`stream`、`line`、`timestampMs` | `src-tauri/src/command_runner.rs`, `src/hooks/useTauriBridge.ts` | 5.1 | DebugPane 实时显示真实日志 |
| 5.3 | 保存命令运行记录和退出状态 | `src-tauri/src/tasks.rs`, `src-tauri/src/command_runner.rs` | 4.2, 5.1 | 任务历史包含命令、cwd、开始/结束时间、exitCode |
| 5.4 | 删除 DebugPane 静态终端 mock 和 spike 专用按钮（单独 commit，便于回滚） | `src/components/Workspace/DebugPane.tsx`, `src/hooks/useTauriBridge.ts`, `src-tauri/src/spike_runner.rs` | 5.2, 5.3 | UI 只显示真实日志和空状态；spike command 和事件名从 `lib.rs` 与前端桥彻底移除 |

### M6 — 错误摘要、人工反馈和下一轮修复上下文

**结果**：命令失败和用户反馈可以进入任务记录，为后续自动修复循环提供上下文。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 6.1 | 实现基础日志错误摘要：exit code、stderr 末尾 20 行、按正则 `(?i)^(error\|fail(ed)?\|panic\|fatal)[:\s]` 匹配的行、非零 exit code | `src-tauri/src/log_analyzer.rs`, `src-tauri/src/command_runner.rs` | 5.3 | 失败命令生成 `errorSummary`，包含上述三类字段；`cargo test` 覆盖每条正则 |
| 6.2 | 人工反馈输入绑定当前任务和当前命令运行 | `src/components/Workspace/DebugPane.tsx`, `src-tauri/src/tasks.rs` | 4.2, 5.3 | 提交反馈后任务事件包含用户输入 |
| 6.3 | 生成修复上下文预览，不实际调用修复 Agent | `src-tauri/src/tasks.rs`, `src-tauri/src/repair_context.rs` | 6.1, 6.2 | 上下文包含最终计划、失败摘要、日志片段、反馈 |

### M7 — 端到端冒烟和文档同步

**结果**：完成第一条功能闭环验收，并更新过时文档。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 7.1 | 端到端手动验收：当前 Loom 仓库作为项目，dummy Agent 规划，运行 `pnpm build` | 应用整体 | M2-M6 | 任务历史完整记录规划、命令日志、退出状态 |
| 7.2 | 增加 Rust 单元测试覆盖项目分析、存储读写、日志摘要 | `src-tauri/src/*` | M2, M4, M6 | `cargo test` 通过 |
| 7.3 | 更新 README 当前状态，避免继续声称“尚未包含应用代码” | `README.md` | 7.1 | README 与实际项目阶段一致 |
| 7.4 | 运行最终构建检查 | `package.json`, `src-tauri/` | 7.1 | `pnpm build`、`cargo check`、`cargo test` 通过 |

## 建议持久化结构

```text
.loom/
  loom.json                 # 项目级元数据和 schemaVersion
  tasks/
    <task-id>.json          # 任务、事件、Agent 调用、命令运行索引
  logs/
    <task-id>/
      <run-id>.stdout.log
      <run-id>.stderr.log
  plans/
    <task-id>-final-plan.md
```

全局配置放应用数据目录：

```text
<app-data>/loom/
  agents.json
  recent-projects.json
  preferences.json
```

## 风险

| 风险 | 可能性 | 影响 | 缓解措施 |
|------|--------|------|----------|
| 一次性做完整多 Agent 编排导致长期不可用 | 高 | 高 | 本计划先做 dummy + 单任务 + 单命令纵向闭环 |
| 命令执行和任务持久化耦合过早 | 中 | 高 | `command_runner` 只负责进程与事件，任务模块负责记录引用 |
| Tauri 文件权限或路径策略在不同平台表现不一致 | 中 | 中 | 第一版用手动路径输入和 Rust 标准库校验，避免新增 dialog 依赖 |
| 日志量过大阻塞 UI 或写盘 | 中 | 高 | 前端保留环形缓冲，后端按 run 写文件，UI 只读近期行 |
| 真实 Agent CLI 协议差异大 | 高 | 高 | dummy adapter 先锁定 orchestrator 契约，CLI adapter 只支持非交互 stdin/prompt-file |
| 现有 spike 代码删除过早 | 低 | 中 | 正式 command runner 验证通过后再移除 spike command 和 UI 控件 |

## 待确认问题

> 原 `.loom/` 位置、危险命令策略、CLI adapter 协议三个问题已在【关键技术决策】中固化，本阶段无未决问题。如实施过程中发现新问题，追加在此。

## 验证策略

- 每个里程碑至少运行一次 `pnpm build` 和 `cargo check`。
- M2、M4、M6 的纯后端逻辑补 `cargo test`。
- M5 必须手动验证长命令停止、失败命令退出状态、stdout/stderr 实时显示。
- 最终端到端场景：
  1. 启动应用。
  2. 输入当前 Loom 仓库路径。
  3. 识别项目为 React/Vite/Tauri/Rust。
  4. 创建一个“为当前项目补功能内核”的任务。
  5. 运行 dummy planning Agent。
  6. 确认最终计划草稿。
  7. 运行 `pnpm build`。
  8. 查看实时日志、退出状态和任务事件。
  9. 提交一条人工反馈。
  10. 查看生成的修复上下文预览。

## 推荐下一步

继续将 dummy/CLI 最小契约升级为真实 Codex / Claude Code 等 Agent adapter，并补充 Review Agent、命令安全策略和前端交互测试。

## 实施结果（2026-05-09）

- 已建立 `Context + useReducer` 前端状态层，项目、Agent、任务、命令日志和运行状态进入统一状态流。
- 已新增 Rust 后端模块：`projects`、`agents`、`tasks`、`storage`、`command_runner` 和共享 `models`。
- 已支持本地项目路径登记、`.loom/loom.json` 初始化、最近项目保存、项目栈/脚本/Git 状态分析。
- 已支持 Agent 配置保存、命令可用性检查，以及内置 dummy planning adapter。
- 已支持任务创建、事件历史、计划草稿、命令运行记录、日志文件、退出状态、错误摘要、人工反馈和修复上下文预览。
- 已删除 spike runner、spike UI 控件和旧 `useTauriBridge`，DebugPane 只走正式 `command_runner`。
- 已更新 README 当前状态。
- 验证通过：`pnpm build`、`cargo check`、`cargo test`、`pnpm tauri build`。
