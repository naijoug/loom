# 2026-08-05 稳定化基线清单

- **Branch**: `codex/project-stabilization`
- **Base commit**: `5d34892`
- **Snapshot**: 55 modified + 31 untracked（创建本清单前；较首次评审多出本轮优化计划文件）
- **Policy**: 不重置、不丢弃；标为“混合”的文件包含完整版本开始前已经存在的项目移除、新任务入口或终端发现改动，后续拆分时按整文件保留。

## 归属规则

- Rust 后端：Tauri command、持久化、安全策略、进程、Agent、Review 和任务闭环。
- 前端：四阶段 UI、桥接、状态与领域类型。
- 测试与工程门禁：测试、smoke、CI、脚本和构建配置。
- 文档：架构、需求审计、计划、测试与 dogfood 证据。
- 设计审计：规划流程优化前后截图。

## 文件清单

| 状态 | 路径 | 归属 | 来源/处理 |
|---|---|---|---|
| modified | `README.md` | 文档 | 完整版本 / 审计改动 |
| modified | `docs/PLANS.md` | 文档 | 完整版本 / 审计改动 |
| modified | `package.json` | 测试与工程门禁 | 完整版本 / 审计改动 |
| modified | `scripts/README.md` | 测试与工程门禁 | 完整版本 / 审计改动 |
| modified | `scripts/interaction-smoke.mjs` | 测试与工程门禁 | 完整版本 / 审计改动 |
| modified | `scripts/visual-smoke.sh` | 测试与工程门禁 | 完整版本 / 审计改动 |
| modified | `src-tauri/capabilities/default.json` | Rust 后端 | 完整版本 / 审计改动 |
| modified | `src-tauri/src/agents.rs` | Rust 后端 | 完整版本 / 审计改动 |
| modified | `src-tauri/src/command_runner.rs` | Rust 后端 | 完整版本 / 审计改动 |
| modified | `src-tauri/src/context_builder.rs` | Rust 后端 | 完整版本 / 审计改动 |
| modified | `src-tauri/src/lib.rs` | Rust 后端 | 完整版本 / 审计改动 |
| modified | `src-tauri/src/models.rs` | Rust 后端 | 完整版本 / 审计改动 |
| modified | `src-tauri/src/projects.rs` | Rust 后端 | 既有在途基线 + 完整版本（混合，整文件保留） |
| modified | `src-tauri/src/pty.rs` | Rust 后端 | 完整版本 / 审计改动 |
| modified | `src-tauri/src/storage.rs` | Rust 后端 | 既有在途基线 + 完整版本（混合，整文件保留） |
| modified | `src-tauri/src/tasks.rs` | Rust 后端 | 完整版本 / 审计改动 |
| modified | `src-tauri/src/terminals.rs` | Rust 后端 | 既有在途基线 + 完整版本（混合，整文件保留） |
| modified | `src-tauri/tauri.conf.json` | Rust 后端 | 完整版本 / 审计改动 |
| modified | `src/App.css` | 前端 | 完整版本 / 审计改动 |
| modified | `src/App.tsx` | 前端 | 既有在途基线 + 完整版本（混合，整文件保留） |
| modified | `src/components/Board/NewTaskModal.tsx` | 前端 | 既有在途基线 + 完整版本（混合，整文件保留） |
| modified | `src/components/Header/Header.css` | 前端 | 完整版本 / 审计改动 |
| modified | `src/components/Header/Header.tsx` | 前端 | 完整版本 / 审计改动 |
| modified | `src/components/Planning/Planning.css` | 前端 | 完整版本 / 审计改动 |
| modified | `src/components/Planning/PlanningChat.tsx` | 前端 | 完整版本 / 审计改动 |
| modified | `src/components/Planning/PlanningTimeline.tsx` | 前端 | 完整版本 / 审计改动 |
| modified | `src/components/Planning/roundSummary.ts` | 前端 | 完整版本 / 审计改动 |
| modified | `src/components/Settings/SettingsPage.css` | 前端 | 完整版本 / 审计改动 |
| modified | `src/components/Settings/SettingsPage.tsx` | 前端 | 完整版本 / 审计改动 |
| modified | `src/components/Sidebar/Navigation.tsx` | 前端 | 既有在途基线 + 完整版本（混合，整文件保留） |
| modified | `src/components/TaskDetail/DonePane.tsx` | 前端 | 完整版本 / 审计改动 |
| modified | `src/components/TaskDetail/SessionPane.tsx` | 前端 | 完整版本 / 审计改动 |
| modified | `src/components/TaskDetail/TaskDetail.css` | 前端 | 完整版本 / 审计改动 |
| modified | `src/components/TaskDetail/TerminalCard.tsx` | 前端 | 完整版本 / 审计改动 |
| modified | `src/components/TaskDetail/TestingPane.tsx` | 前端 | 完整版本 / 审计改动 |
| modified | `src/components/Workspace/WorkspaceSplit.tsx` | 前端 | 完整版本 / 审计改动 |
| modified | `src/domain/agent.ts` | 前端 | 完整版本 / 审计改动 |
| modified | `src/domain/command.ts` | 前端 | 完整版本 / 审计改动 |
| modified | `src/domain/index.ts` | 前端 | 完整版本 / 审计改动 |
| modified | `src/domain/task.ts` | 前端 | 完整版本 / 审计改动 |
| modified | `src/hooks/useAgentBridge.ts` | 前端 | 完整版本 / 审计改动 |
| modified | `src/hooks/useCommandBridge.ts` | 前端 | 完整版本 / 审计改动 |
| modified | `src/hooks/useProjectBridge.ts` | 前端 | 完整版本 / 审计改动 |
| modified | `src/hooks/usePtyBridge.ts` | 前端 | 完整版本 / 审计改动 |
| modified | `src/hooks/useTaskBridge.ts` | 前端 | 完整版本 / 审计改动 |
| modified | `src/main.tsx` | 前端 | 完整版本 / 审计改动 |
| modified | `src/preview/PlanningPreviewApp.tsx` | 前端 | 完整版本 / 审计改动 |
| modified | `src/state/reducer.ts` | 前端 | 既有在途基线 + 完整版本（混合，整文件保留） |
| modified | `src/styles/theme.css` | 前端 | 完整版本 / 审计改动 |
| modified | `src/utils/agentRun.ts` | 前端 | 完整版本 / 审计改动 |
| modified | `src/utils/taskTimeline.ts` | 前端 | 完整版本 / 审计改动 |
| modified | `tests/unit/reducer.test.cjs` | 测试与工程门禁 | 完整版本 / 审计改动 |
| modified | `tests/unit/roundSummary.test.cjs` | 测试与工程门禁 | 完整版本 / 审计改动 |
| modified | `tests/unit/taskTimeline.test.cjs` | 测试与工程门禁 | 完整版本 / 审计改动 |
| modified | `tsconfig.test.json` | 测试与工程门禁 | 完整版本 / 审计改动 |
| untracked | `.github/` | 测试与工程门禁 | 完整版本 / 审计改动 |
| untracked | `designs/audits/` | 设计审计 | 完整版本 / 审计改动 |
| untracked | `docs/agent-adapter.md` | 文档 | 完整版本 / 审计改动 |
| untracked | `docs/architecture.md` | 文档 | 完整版本 / 审计改动 |
| untracked | `docs/dogfood/complete-version-2026-07-24.md` | 文档 | 完整版本 / 审计改动 |
| untracked | `docs/plans/2026-07-24/` | 文档 | 完整版本 / 审计改动 |
| untracked | `docs/plans/2026-08-05/` | 文档 | 完整版本 / 审计改动 |
| untracked | `docs/requirements-audit.md` | 文档 | 完整版本 / 审计改动 |
| untracked | `docs/security-policy.md` | 文档 | 完整版本 / 审计改动 |
| untracked | `docs/task-state-machine.md` | 文档 | 完整版本 / 审计改动 |
| untracked | `docs/testing.md` | 文档 | 完整版本 / 审计改动 |
| untracked | `scripts/check.sh` | 测试与工程门禁 | 完整版本 / 审计改动 |
| untracked | `src-tauri/src/agent_adapter.rs` | Rust 后端 | 完整版本 / 审计改动 |
| untracked | `src-tauri/src/agent_diagnostics.rs` | Rust 后端 | 完整版本 / 审计改动 |
| untracked | `src-tauri/src/attachments.rs` | Rust 后端 | 完整版本 / 审计改动 |
| untracked | `src-tauri/src/execution_policy.rs` | Rust 后端 | 完整版本 / 审计改动 |
| untracked | `src-tauri/src/implementation_review.rs` | Rust 后端 | 完整版本 / 审计改动 |
| untracked | `src-tauri/src/project_git.rs` | Rust 后端 | 完整版本 / 审计改动 |
| untracked | `src-tauri/src/project_preferences.rs` | Rust 后端 | 完整版本 / 审计改动 |
| untracked | `src-tauri/src/run_recovery.rs` | Rust 后端 | 完整版本 / 审计改动 |
| untracked | `src-tauri/src/task_state.rs` | Rust 后端 | 完整版本 / 审计改动 |
| untracked | `src-tauri/src/task_summary.rs` | Rust 后端 | 完整版本 / 审计改动 |
| untracked | `src/components/Planning/participantSelection.ts` | 前端 | 完整版本 / 审计改动 |
| untracked | `src/components/TaskDetail/ImplementationReviewPanel.tsx` | 前端 | 完整版本 / 审计改动 |
| untracked | `src/hooks/useImplementationReviewBridge.ts` | 前端 | 完整版本 / 审计改动 |
| untracked | `src/hooks/useProjectPreferencesBridge.ts` | 前端 | 完整版本 / 审计改动 |
| untracked | `src/utils/executionPolicy.ts` | 前端 | 完整版本 / 审计改动 |
| untracked | `src/utils/implementationReview.ts` | 前端 | 完整版本 / 审计改动 |
| untracked | `tests/e2e/` | 测试与工程门禁 | 完整版本 / 审计改动 |
| untracked | `tests/unit/implementationReview.test.cjs` | 测试与工程门禁 | 完整版本 / 审计改动 |
| untracked | `tests/unit/participantSelection.test.cjs` | 测试与工程门禁 | 完整版本 / 审计改动 |

## 提交边界

1. Rust 后端能力与 Tauri 配置。
2. 前端领域、桥接、流程 UI 与状态。
3. 测试、smoke、CI 和构建门禁。
4. 文档、计划和设计审计证据。

每个边界提交后都从该 commit 的独立 worktree 运行相应门禁；若中间提交无法独立构建，则合并到最近的依赖提交，不保留破损历史。
