# Loom 最终形态剩余功能 — Plan

- **Date**: 2026-05-13
- **Author**: Codex
- **Status**: in-progress

## 目标

把 Loom 从“已能添加 Agent、发起多 Agent 计划讨论、生成计划并进入 todo 执行”的 MVP 骨架，推进到符合 `docs/requirements.md` 的本地多 Agent 开发工作台：任务是核心边界，Agent adapter、命令执行、Review、调试、修复、验收证据和总结都围绕任务持久化。

本计划参考 `docs/ref.md` 中的 Craft Agents 与 Codex 研究结论：借鉴 Craft Agents 的 session/task 作为权限、日志、工具、上下文和持久化边界；借鉴 Codex 的 registry/control、命令执行事件流、权限策略和可审计运行记录，但不直接绑定二者内部实现。

## 非目标

- 不在本轮引入新依赖、插件市场、云同步、团队账号或完整浏览器自动化。
- 不直接复制 Craft Agents 的 Electron/Bun 架构，也不绑定 Codex 未稳定内部协议。
- 不实现完整沙箱/审批系统；先做可解释的权限字段、命令可见记录、日志和风险提示。

## 成功标准

- Agent 配置支持添加、编辑、删除、启用/禁用、能力标签和权限字段，并能被计划与 todo 执行阶段复用。
- 计划阶段能保留多 Agent 输出、失败诊断、最终计划文档和 `docs/PLANS.md` 索引。
- 实施阶段能选择主 Agent 执行 todo，展示实时输出、命令记录和验证证据。
- Review 阶段能选择协作 Agent，记录阻塞问题、建议修改和可接受风险。
- 调试/修复阶段能从失败命令、日志和人工反馈生成修复上下文，并支持至少两轮“验证 -> 修复 -> 再验证”。
- 任务结束时能生成包含修改范围、关键决策、验证结果和剩余风险的总结。

## 当前状态

- `src/components/Settings/SettingsPage.tsx`：已有 Agent 列表、启用/禁用和新增 Agent 表单；缺少编辑、删除、默认阶段偏好。
- `src-tauri/src/agents.rs`：已有默认 Agent 发现、`create_agent`、`set_agent_enabled`、多 Agent planning 调用、计划文件输出和 `docs/PLANS.md` 更新。
- `src/components/Workspace/PlanningPane.tsx`：已有 @mention、多 Agent planning stream、最终计划确认入口。
- `src-tauri/src/tasks.rs`：已有 task 持久化、confirm plan 生成 todos、start/complete todo、feedback 和 repair context。
- `src/components/Workspace/ImplementationPane.tsx`：已有 todo 列表、实现 Agent 选择器和通过 command runner 启动 Agent。
- `src/components/Workspace/ImplementationOutputPane.tsx`：已有 selected todo handoff、review checklist、最近 Agent run 和日志展示。
- `src/components/Workspace/DebugPane.tsx` 与 `src-tauri/src/command_runner.rs`：已有命令启动、停止、stdout/stderr 事件、错误摘要和日志持久化。
- `docs/ref.md`：已有 Craft Agents / Codex 参考包，可作为剩余功能边界依据。

## 里程碑

### M1 — Agent 配置闭环
**Outcome**: 用户可以完整管理多个 Agent 配置，并在 planning/todo/review 阶段复用同一套能力与权限信息。

| # | Task | Files / Symbols | Depends on | Verification |
|---|------|-----------------|------------|--------------|
| 1.1 | 增加 Agent 编辑与删除 Tauri commands | `src-tauri/src/agents.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/models.rs` | — | `cargo test` 覆盖更新、删除、默认 Agent 保护 |
| 1.2 | 前端 Settings 支持编辑/删除 Agent | `src/components/Settings/SettingsPage.tsx`, `SettingsPage.css`, `src/hooks/useAgentBridge.ts`, `src/state/reducer.ts` | 1.1 | `npm run build`，手动检查表单状态 |
| 1.3 | 增加阶段默认 Agent 偏好模型 | `src/domain/agent.ts`, Rust models/storage | 1.1 | 单元测试默认偏好序列化 |

### M2 — Review Agent 流程
**Outcome**: 实施后能选择协作 Agent Review，并把结果分类持久化到任务记录。

| # | Task | Files / Symbols | Depends on | Verification |
|---|------|-----------------|------------|--------------|
| 2.1 | 增加 review run 数据模型与 Tauri command | `src-tauri/src/models.rs`, `agents.rs`, `tasks.rs` | M1 | Rust 测试 Review 记录持久化 |
| 2.2 | 实施输出区增加 Review Agent 选择、触发和结果分类 | `ImplementationOutputPane.tsx`, `Workspace.css` | 2.1 | `npm run build`，手动检查阻塞/建议/风险展示 |
| 2.3 | Review 结果进入任务事件流和 repair context | `tasks.rs`, `DebugPane.tsx` | 2.1 | Rust 测试 repair context 包含 review |

### M3 — 调试验收与自动修复循环
**Outcome**: 失败命令、日志错误、人工反馈可以稳定生成 Agent 修复任务，并支持重复验证。

| # | Task | Files / Symbols | Depends on | Verification |
|---|------|-----------------|------------|--------------|
| 3.1 | 增强 project analyzer 的建议命令和脚本选择 | `src-tauri/src/projects.rs`, `src/domain/project.ts` | — | Rust 测试 Node/Rust/Tauri 脚本识别 |
| 3.2 | DebugPane 支持历史 run、日志过滤、失败摘要选择 | `DebugPane.tsx`, `Workspace.css` | 3.1 | `npm run build`，手动日志筛选 |
| 3.3 | 自动修复 command：用主 Agent 消费 repair context 并启动 tracked run | `tasks.rs`, `agents.rs`, `ImplementationPane.tsx` | M1, 3.2 | Rust + 前端构建，失败命令 smoke |

### M4 — 权限、审计与安全提示
**Outcome**: 所有 Agent 与命令运行都有可见权限边界、工作目录、脱敏日志和风险提示。

| # | Task | Files / Symbols | Depends on | Verification |
|---|------|-----------------|------------|--------------|
| 4.1 | 将 Agent 权限映射到 planning/todo/review 提示和命令参数 | `agents.rs`, `ImplementationPane.tsx` | M1 | 测试只读/写入模式参数 |
| 4.2 | 高风险命令提示与阻止策略 MVP | `command_runner.rs`, `DebugPane.tsx` | 4.1 | Rust 测试危险命令识别 |
| 4.3 | 审计视图展示 Agent run、command run、feedback、review | 新建/扩展 Workspace 视图 | M2, M3 | 手动检查事件顺序和证据引用 |

### M5 — 任务总结与完成闭环
**Outcome**: 一个任务能从计划、实施、Review、调试、修复到完成总结形成完整记录。

| # | Task | Files / Symbols | Depends on | Verification |
|---|------|-----------------|------------|--------------|
| 5.1 | 增加 task summary 数据与生成 command | `models.rs`, `tasks.rs` | M2, M3 | Rust 测试 summary 包含文件、验证、风险 |
| 5.2 | UI 增加完成/阻塞/导出总结入口 | `Header.tsx`, Workspace 组件 | 5.1 | `npm run build`，手动完成流程 |
| 5.3 | 端到端 smoke：planning -> todo -> agent run -> debug -> repair -> summary | `docs/plans` 验证记录 | M5 | `npm run build`, `cargo test`, 手动记录 |

## 风险

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| 不同 CLI Agent 非交互模式差异大 | H | H | Adapter 先保存 command/args/protocol/权限，失败输出进入证据，不阻断其它 Agent |
| 前端直接拼接 prompt 可能导致命令记录过长 | M | M | 逐步迁移到 Rust 端 prompt 文件，UI 仅展示摘要 |
| 删除默认 Agent 影响自动发现 | M | M | 内置 Agent 只允许启用/禁用；删除仅开放给自定义 Agent 配置 |
| 自动修复执行高风险命令 | M | H | MVP 先做能力/权限字段和危险命令提示，后续扩展审批策略 |
| 状态机事件越来越分散 | M | H | 任务事件作为单一审计流，新增 run/review/summary 都写入 task events |

## 待确认问题

- [ ] 自定义 Agent args 是否要统一支持 `{prompt}`、`{promptFile}`、`{projectPath}` 三种占位符，还是 planning 与 implementation 分开配置。
- [ ] Review Agent 是否默认使用只读模式，还是允许用户为某些 Review Agent 开写权限。
- [ ] 自动修复循环是否默认自动重跑上一条失败命令，还是要求用户选择验证命令。
- [ ] 任务总结是否只写入 `.loom/tasks/*.json`，还是同步生成 Markdown 文档。

## 验证策略

- 每个 Rust command 或状态机变更都补单元测试，至少覆盖成功路径和错误路径。
- 每个前端流程变更至少运行 `npm run build`，关键交互用本地 Vite/Tauri 手动检查。
- 每个跨阶段功能都检查 task JSON 是否包含事件、证据引用和状态变化。
- 回滚策略：每个里程碑保持小 diff；若某个阶段失败，可保留已有数据模型并回滚 UI 入口，不破坏已持久化任务。
