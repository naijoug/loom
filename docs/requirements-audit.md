# Loom 完整版本需求审计

审计基线：`docs/requirements.md`，2026-07-24。状态 `完成` 表示存在代码与自动测试证据；MVP 明确排除项不计为缺项。

| 需求域 | 状态 | 直接证据 |
|---|---|---|
| Agent CRUD、3+ profile、能力与读写/命令权限 | 完成 | `agents.rs`, `SettingsPage.tsx`, agent tests |
| Agent 可执行/版本/认证提示诊断 | 完成 | `agent_diagnostics.rs`, Settings |
| 阶段默认 Agent 与项目偏好 | 完成 | `project_preferences.rs`, `.loom/agent-preferences.json` tests |
| Codex、Claude、自定义 CLI adapter | 完成 | `agent_adapter.rs`, `docs/agent-adapter.md`, adapter tests |
| 项目目录、技术栈、Git、脚本/常见命令 | 完成 | `projects.rs`, `terminals.rs`, project/terminal tests |
| 多 Agent 并行计划、交叉 Review、合成 | 完成 | `agents.rs`, planning timeline, parallel/review/synthesis tests |
| 人工补充与最终计划文档 | 完成 | planning decisions, `plan_html.rs`, `docs/plans` + index |
| 主 Agent Todo 实施、实时输出、中断/重试/session | 完成 | `SessionPane.tsx`, command runner, adapter/session tests |
| 主 Agent 切换原因 | 完成 | `switch_primary_agent`, backend test |
| 独立实施 Review 与分类 finding | 完成 | `implementation_review.rs`, `ImplementationReviewPanel.tsx` |
| blocker Testing 门禁、修复/重审/接受风险 | 完成 | `ensure_review_gate`, review decision tests, UI gate tests |
| 命令/PTY 实时 stdout、stderr、退出码 | 完成 | `command_runner.rs`, `pty.rs`, xterm `TerminalCard.tsx` |
| 项目验证命令发现 | 完成 | `terminals.rs`, Node/Rust/Go/Python/Flutter fixtures |
| URL、端口、警告、堆栈、测试失败解析 | 完成 | `analyze_output`, cross-stack parser tests |
| 日志搜索、过滤、折叠、片段引用、历史 | 完成 | `TerminalCard.tsx`, `read_command_run_logs`, testing cycles |
| 自动发现→修复→再验证循环 | 完成 | `TestingPane.tsx`, `loopPolicy.ts`, two-repair command smoke |
| 自动循环安全预算、停止与升级 | 完成 | `loopPolicy`, execution policy, timeout/escalation tests |
| 人工问题、复现、期望、日志、截图/文件 | 完成 | `UserFeedback`, `attachments.rs`, feedback composer |
| 反馈绑定 run 并进入 repair context | 完成 | `append_feedback`, `context_builder.rs`, task events |
| 暂停、恢复、阻塞、取消 | 完成 | `task_state.rs`, lifecycle commands/tests, Header controls |
| 重启恢复与 interrupted 对账 | 完成 | `run_recovery.rs`, recovery tests |
| 全阶段历史与证据 | 完成 | Task JSON、`taskTimeline.ts`, timeline tests |
| Git baseline、真实 diff/numstat 与原有脏改动归因 | 完成 | `project_git.rs`, dirty/untracked fixture |
| 完成门禁只接受最新成功 validation | 完成 | `apply_complete_task`, task tests |
| JSON + Markdown 总结、打开、定位、导出 | 完成 | `task_summary.rs`, `DonePane.tsx`, redaction test |
| 命令审计、危险审批、cwd 边界 | 完成 | `execution_policy.rs`, command/PTY integration, policy tests |
| Token/凭据/Agent 输出脱敏 | 完成 | `redact_sensitive_text`, CLI/review/log/summary tests |
| 原子持久化与日志非阻塞 UI | 完成 | `storage.rs`, Tokio readers/events, reducer log cap |
| 四阶段 IA、状态与当前活动可见 | 完成 | Header/Workspace/Planning/Session/Testing/Done UI |
| 深浅主题、960–1440 响应式与可访问交互 | 完成 | 26 屏 visual smoke、960px DOM 溢出/控制可见性断言、发布版桌面 AX 检查 |
| 一键本地质量门禁和 CI | 完成 | `scripts/check.sh`, `pnpm check`, `.github/workflows/ci.yml` |
| 可运行发布产物与真实桌面健康检查 | 完成 | `Loom.app`、有效 arm64 DMG、发布版 `tauri://localhost` 启动与 `health_check: OK` |

## MVP 明确排除

插件市场、云同步、团队账号/实时多人协作、完整 GUI 自动操作与可视化录制不属于 `docs/requirements.md` 的第一版强制范围。本版本保留 adapter 扩展点，但不为这些能力提供假入口。

## 发布验收记录

最终命令、桌面 smoke、release build、截图与版本风险记录在 `docs/dogfood/complete-version-2026-07-24.md`。该报告的强制门禁均已通过；Apple Developer ID 签名与公证是外部分发条件，未被误记为本地功能完成证据。
