# Loom 架构

## 目标与边界

Loom 是本地优先的 Tauri 桌面开发工作台。React 负责流程展示和人工交互；Rust 负责所有有副作用的能力，包括文件访问、项目分析、Agent 调用、命令与 PTY 生命周期、安全策略、日志、任务状态和持久化。核心闭环不依赖云服务。

## 模块

| 层 | 主要模块 | 职责 |
|---|---|---|
| UI | `src/components`, `src/state`, `src/hooks` | 四阶段导航、表单、实时日志、Review、人工反馈、总结与设置 |
| Chat | `src/features/chat/`, `src-tauri/src/chat.rs`, `docs/architecture/chat-contracts.md` | Craft 启发本机 Agent Chat（Inbox / Transcript / Composer / 权限三档 / 可选上下文空态）；经 `agent_adapter` + `ProcessSupervisor(Chat)`；**不等于** Task |
| Orchestrator | `task_state.rs`, `task_repository.rs`, `tasks/{lifecycle,testing}.rs`, `run_recovery.rs` | 权威状态转换、事务化任务 mutation、生命周期门禁、重启对账、任务事件 |
| Agent Adapter | `agent_adapter.rs`, `agent_diagnostics.rs`, `agents/{config,orchestrator,prompts,stream,artifacts}.rs` | Codex、Claude、自定义 CLI 参数映射、能力/权限检查、输出解析、规划编排和 session |
| Review Engine | `implementation_review.rs` | 独立 Reviewer 上下文、结构化 finding、决策、blocker gate 与重审 |
| Execution | `execution_policy.rs`, `process_supervisor.rs`, `command_runner.rs`, `pty.rs` | cwd 边界、危险分类、一次性审批、统一进程组/停止原因、超时、实时日志和历史 |
| Project | `projects.rs`, `terminals.rs`, `project_preferences.rs`, `project_git.rs` | 技术栈/脚本发现、终端槽位、阶段偏好、Git baseline 与归因 |
| Evidence | `context_builder.rs`, `attachments.rs`, `task_summary.rs` | prompt 预算、附件、repair context、JSON/Markdown 交付总结 |
| Persistence | `migrations.rs`, `storage.rs`, `.loom/` | 版本化 JSON 迁移、原子 JSON/文本写入、任务、日志、配置、附件和总结 |
| Contract | `contracts/tauri-contract.json`, `src/api/` | command/event 名称、状态枚举、持久化 wire sample 与前端 typed client |

## 关键数据流

1. 用户登记项目；Rust 分析栈、Git 和可用命令，并初始化 `.loom/`。
2. 多个 Planning Agent 并行起草，随后交叉 Review 和合成；计划与原始证据落盘。
3. 用户确认计划后生成 Todo。第一次开始实施时捕获 Git baseline；主 Agent 的每次执行都经过 adapter 和 execution policy。
4. Todo 全部完成后，至少一个非主 Agent 基于计划、Git diff、命令证据和决策执行结构化实施 Review。未解决 blocker 阻止进入 Testing。
5. Testing 运行 PTY 或一次性验证命令。日志异步写文件并发事件到 UI；失败可触发有界修复循环，人工反馈和附件进入同一 repair context。
6. 最新验证必须成功，后端才允许完成。完成时基于 baseline 生成 `summary.json` 与 `summary.md`，再原子保存 Task 引用。

## 持久化布局

```text
<project>/.loom/
  loom.json
  agent-preferences.json
  terminals.json
  chat/
    index.json
    sessions/<session-id>.json
  tasks/<task-id>.json
  tasks/<task-id>/attachments/*
  tasks/<task-id>/summary.json
  tasks/<task-id>/summary.md
  logs/<task-id>/*
  planning/<task-id>/*
```

计划文档位于项目的 `docs/plans/YYYY-MM-DD/`，并同步维护 `docs/PLANS.md`。

Task、Agent 配置、App Settings、终端槽位和项目 Agent 偏好均使用 `{ schemaVersion, data }` 信封。首次读取旧版裸 JSON 时会原子迁移为当前版本；遇到无效数据或未来版本时保留 `.bak-<kind>-<timestamp>` 副本并返回明确错误，不静默覆盖。计划、Review 证据、HTML 和索引同样通过临时文件加 rename 原子替换。

`contracts/tauri-contract.json` 是 Rust 与 TypeScript 共享的 canonical contract。Rust 测试校验 Tauri 注册表、事件发送方、枚举解析、schema version 和持久化模型序列化；前端测试校验同一 fixture，并禁止桥接层之外直接调用 `@tauri-apps/api/core` 或 `event`。

## 并发与恢复

- Planning Agent 可并行运行；Agent、命令、PTY 和 Review 共享 `ProcessSupervisor` 的 run metadata、进程组终止和 stop reason，任务暂停、阻塞或取消会停止关联进程组。
- 所有版本化 store 与结构化产物使用临时文件加 rename 原子替换，避免半写入或悬空索引。
- 所有 Task mutation 通过 `TaskRepository` 的任务级 read-modify-write 锁执行；锁在成功、失败和删除后回收，长任务提交结果前会与最新 lifecycle、run、feedback 和 decision 合并。
- 桌面端重启后，持久化为 `running` 且不在本进程 supervisor 中的 command/PTY/review 会改为 `interrupted`，保留日志、退出原因与可用的原生 session resume 信息。
- UI 状态只是 Task 的投影。所有阶段推进和验收门禁均在 Rust 再校验，不能通过前端直接 invoke 绕过。

## 扩展 Agent

新增 Agent 时实现 adapter 的 prepare 映射，声明 output mode、阶段能力、文件/命令权限和可恢复 session 规则。核心 task、review 和 testing 流程只依赖 `PreparedAgentInvocation`，不依赖具体 CLI。详见 [Agent Adapter](agent-adapter.md)。

## 本机 Agent Chat（Phase 1）

Craft 启发的本机会话面已完成 M0–M5：默认冷启动进 Chat；会话收件箱（`active` / `needs_attention` 过滤 / `archived`）、三档权限（`explore`/`ask`/`auto`）、流式 Turn + best-effort `parts`、内联 Agent 诊断、可选右侧「上下文」空态（项目路径 / `.loom/chat` / 权限 / 诊断；**无 MCP 连接**）。升格为 Task 仅为草稿 stub，不自动推进状态机。

进程：`ProcessSupervisor` + `ProcessKind::Chat`（`task_id=chat:{sessionId}`）。契约见 [chat-contracts.md](architecture/chat-contracts.md)；dogfood 清单见 [craft-chat-phase1-checklist.md](dogfood/craft-chat-phase1-checklist.md)。

**Phase 2+（明确推迟）**：Sources/MCP 连接、Ask 审批 UI、后台任务产品化、Inbox 五态对齐 Craft。
