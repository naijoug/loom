# 计划文档索引

## 2026-08-20

- 01:45-testing-pane-state-coverage.md
  > 规划把 validation evidence 的 intent / task scope 边界提升到 Testing Pane 组件级覆盖，补验证运行中、失败、通过待人工接收和 preview / unrelated task 不应解锁的 UI 断言，并以 `pnpm test` / `pnpm check` 收口。

## 2026-08-07

- 10:00-beta-release-readiness.md
  > 规划 Loom 从个人 dogfood 稳定版走向邀请制公开 Beta 的发布准备边界：先补 Beta scope、安全说明、反馈模板，再处理 macOS 可复现产物、安装/卸载、隐私披露、诊断包复核和小范围试用回收机制。

## 2026-08-05

- 11:14-project-optimization.md
  > （已完成，M0–M5）稳定化分支已固化版本/契约/进程仓储底座、前端流程拆分、分层测试、中文默认工作流、结构化 blocker、脱敏诊断包与性能预算；100 项前端、189 项 Rust 单元、2 项 Rust 集成、30 屏视觉差异、release/DMG 和两轮桌面重启均通过。

## 2026-07-24

- 08:55-complete-version.md
  > （已完成，M0–M6）完整闭环已覆盖权威状态机、安全执行/恢复、统一 Agent adapter、独立实施 Review、结构化反馈附件、日志洞察、Git baseline 归因及可导出 JSON/Markdown 总结；85 项前端测试、166 项 Rust 测试、完整 smoke、严格 dogfood、Tauri release/DMG 和真实桌面验收均通过。

## 2026-06-26

- 17:38-ui-2026-06-impl.md
  > （已完成，M0–M6）把 `designs/loom-ui-2026-06/` 深色技术工作台设计（5 个核心流程页面 + 共享 Shell + 设计令牌）落地到现有 Tauri + React 代码：先行落地 `--loom-*` 别名层与重构 AppLayout/Sidebar/Header miniStepper 外壳（M0），再逐页对齐规划讨论室/任务看板/执行中/调试验收/交付总结（M1–M5），仅覆盖视觉与交互层，后端能力与状态机保持现状。验证：`pnpm test` 81/81、`pnpm build`、`cargo test` 123/123、`pnpm smoke:visual` 26 屏全过。（2026-06-26 plan-review 修订：明确令牌为命名空间对齐而非值迁移，定方案 A 别名层、不强制改名存量约 1144 处旧引用；删除冗余 CDN 字体任务、确认 Geist Mono 已 `@fontsource` 自托管；miniStepper 已是 4 段，改为只调状态色 + 标签中文化并列入待确认。）

## 2026-06-25

- 19:35-settings-refactor.md
  > 规划 Settings 页面真实化重构：盘点当前假功能，删除无实现的通知/外观/更新等控件，保留并实现主题、Agent 管理、项目命令、安全护栏和真实 About 信息。（2026-06-26 plan-review 修订：补开工前置条件——须先落地在途 loop-engine 重构；改正 main.tsx 主题误读，主题改走 settings-backed 而非改 main.tsx；明确保留 `safety` tab 不新建 commands tab；SessionPane 纳入 slot 共享一致性；校准代码行锚。）

- 14:54-loop-engine-consolidation-fix.md
  > （已完成，方案 B）承接 10:21 的 code review：用 `loopPolicy` 作为 implement/testing 自动修复循环的单一策略源，已补 `SessionPane.startedAtMs`、TestingPane 有界自动修复、wall-clock timeout ticker、`terminationReason=timeout` 持久化通道，并删除测过但不跑的 `loop_engine.rs`/`validation.rs`。

- 10:21-loop-engineering-refactor.md
  > （实施中，M0-M4 与 M6 已落地）以 Loop Engineering 范式重构 Loom 编排核心：已拆分 `CommandRun` intent 与状态推进边界，落地 Rust Loop Engine、implement validation/repair 回路、context builder、session resume、compact memory 和统一 trace/timeline；M6 已完成 timeline canary、headless real-agent repair-loop canary、真实桌面 UI Auto repair canary 与 `pnpm dogfood:verify -- --strict` 机器校验。worktree 隔离 + verifier sub-agent + L1/L2/L3 自主化列为可选战略里程碑。

## 2026-06-24

- 17:56-testing-cockpit-redesign.md
  > （已实施 M0–M4）把 Testing 阶段改造为人机协同调试驾驶舱：dev-server 终端改用 PTY（portable-pty）+ xterm.js（真彩色/选区/复制），终端槽位每项目可配置 N 个并持久化（取代硬编码 frontend/backend）；选中终端文本一键投喂 agent；Debug agent 改为可在阶段内真正调起修复 run 的对话（复用 startCommandRun + append_feedback）；落地 Auto/Manual 与布局重排。已评估并排除 tmux。

## 2026-06-12

- 10:46-workflow-stage-navigation.md
  > （已实施）解耦"任务所处阶段"与"用户查看阶段"：新增前端 viewedStage 与统一 WorkflowStageId，让 header 步骤条可点击回看历史阶段；只读边界覆盖 PlanningTimeline / SessionPane / TestingPane 的所有状态变更入口，同时补看板 breadcrumb/sidebar 双入口并清理 Board 假数据。

## 2026-06-11

- 16:05-planning-live-output.md
  > （已实施，v3）针对时间线实测反馈：进度改走双 CLI 官方结构化事件流（claude stream-json + codex exec --json，hook 方案评估后排除），agent 行内嵌实时 tail 与执行日志；每次调用绑定原生 session（记录 session id 并提供可复制的 resume 命令，在 codex/claude 原生工具中可恢复同一会话）；失败行前置具体原因并支持打开完整日志与超时 partial 输出；超时预算 240s 提至 480s。

- 11:15-planning-v2-timeline.md
  > （已实施）基于实测反馈二次重构计划功能：计划页改为按轮次分组的单列时间线工作台并移除右侧摘要面板，互评自动纳入"起草→互评→合成"流水线且结论参与合成，Agent 失败分类后自动重试一次并支持单 Agent 手动重跑，计划 HTML 新增应用内 WebviewWindow 查看器与外部浏览器双入口。

## 2026-06-10

- 17:07-planning-redesign.md
  > 重设计计划功能：多本地 Agent 并行（复用 tauri::async_runtime::spawn，零新依赖）产出基于项目代码的结构化候选计划，由合成 Agent 在互评前融合最终计划，并用安全的 Rust 渲染器生成自包含可视化 HTML，同时重构计划页为进度卡 + 候选 Tab + 渲染预览的工作台。

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
