# 计划文档索引

## 2026-10-21

- 06:12-skills-module-migration.md
  > 2026-10-22 补充复核：共享注入块归属、跨客户端未完成事务发现、应用更新准入已列为必需验收项；契约、任务、V-16–18 与 G1/G2 门槛对应关系见计划 §9.1。文档补充不代表验收通过。
  > （实施中；v5 已完成 M1b API/SSH、M2b 源 8 项测试等价记录与 M5 内容独立出口；G1/G2 未整体验收）Settings → Skills 与 CLI/TUI/ng 共用服务；真实匿名 API 与原生临时安装/孤立卸载通过，SSH 用户配置保留，完整 check 前端 111/Rust 246、release app bundle 通过。继续 M3/M4、G1 视觉/无源码/外网 Git 余项及 M6/G2；源旧实现、管理测试与转发入口统一在 G2 后的 M7 移除。证据见 dogfood/2026-10-22-skills-migration-review.md，用法见 skills-cli.md。

## 2026-10-20

- 18:25-openui-inline-cards-experiment.md
  > （O0 可行性探针 passed，见 dogfood/2026-10-20-openui-probe.md：Claude Code 2.1.304（本机后端 GLM-5.3）的 `--append-system-prompt` 每轮须重传，3/3 有效回合产出可解析卡片，每次调用 +1048 token，卡片 chunk gzip 约 60 KB；回退改为依据解析错误；O1 已完成 `b6c4d47`：依赖、devtools 别名、组件库、提示词生成与 `--check`、单测 5/5，不设模型可触发的动作；O2 已完成：设置 `experimentalInlineCards`、能力 `inlineCards`（仅 Claude）、每轮 `--append-system-prompt` 注入、执行记录脱敏、仅生效时计入续聊指纹，`e391d51`；O3 已完成 `0dff105`：懒加载卡片 chunk（gzip 63 KB，主包 +0.55 KB gzip）、依据解析结果回退源码、Loom 自有查看源码/复制/插入按钮、设置“实验”分区，`pnpm check` 与 `smoke:visual` 通过；O4 已完成 `98fbd2f`：`smoke:visual` 覆盖卡片渲染/回退/插入不发送/深色中文/窄窗口，外部请求 0；O5 已完成，见 dogfood/2026-10-20-openui-acceptance.md：真实链路 20 回合，代码块 7/7 可解析、无误出卡片、项目写入 0、提示词脱敏，但出卡率 Research 5/7、Code 2/7 未达 S6（GLM-5.3 工具误判与偏好 Markdown 表格），每次调用 +1.05k token；结论继续实验、不转正，桌面端交互待人工验收）以默认关闭的实验开关试点 OpenUI 内联卡片：只引入 `@openuidev/react-lang` 和 `zod`，Loom 自有 4 个组件（文件清单、变更摘要、表格、检查清单）；Claude Code 通过 `--append-system-prompt` 注入编译期生成的内联模式提示词，并把开关状态计入续聊指纹；前端按 ` ```openui-lang ` 代码块懒加载渲染，不传 `toolProvider`，解析失败回退为源码；屏蔽 devtools CDN 与安装遥测；不改 v2 存储和 IPC。

- 05:55-chat-ui-polish.md
  > （代码完成，合成网页与 smoke:visual 已验证；桌面已由用户确认 Auto-run 与切换 Agent 提示条；已提交）按 Claude Design 画布打磨简化 Chat 的界面：lucide 图标替换文本字符，Research/Code 模式着色，Composer、斜杠菜单与 Agent 菜单补全信息，用应用内提示条替换 Auto-run 与切换 Agent 的 `window.confirm`，新会话页与设置页改为分区卡片；只改前端，不改存储与适配器。

- 01:01-simple-chat-implementation.md
  > （工程与资料收尾完成，待用户桌面验收；代码 `7e1dfcb`/`3dd9a9f`，首版资料 `0acbb10`；check 与 tauri no-bundle 通过；原生验收由用户接手，未合入 main）00:44 重构计划的执行拆解：10 个可独立 Review 的 PR（CLI 实测、入口收敛与 ErrorBoundary、Agent 探测与按项目聚合会话、OpenCode 适配与工具事件配对、会话界面（新会话只选模式）、Composer 斜杠命令与输入框内 Agent 选择、英文优先 i18n（可切简体中文）、设置精简、删除旧工作流、文档与真实验收），对应 Claude Design 初版 UI 画布与设计令牌。

- 00:44-simple-cli-chat-refactor.md
  > （工程交付完成，当前重构入口；R0–R4 代码与 R5 资料已提交，产品验收待用户执行；成功标准 1/7/8 passed，2–5/9/10 partial，Grok blocked）把 Loom 收敛为 Claude Code 式本机 CLI Chat：会话按项目分组，会话级「Research / Code」模式（复用 explore/ask/auto，不改 schema）；界面英文优先、可切简体中文，Agent 在输入框中选择；基于现有 Chat 运行链，调用已安装的 Claude Code / Codex / OpenCode / Grok；新增 OpenCode 适配与通用探测，重做单列转录、工具卡片和斜杠命令 Composer，移除旧 Task 工作流，冻结新内核。

## 2026-10-15

- 22:06-auto-optimization-experiments.md
  > （设计完成，待实施）借鉴 autoresearch 的基线—候选—固定评测—保留/丢弃循环，设计新内核自动优化实验；首版为 macOS 可信本地项目、单 Agent/单指标/串行 Node CPU 模板，覆盖受管快照、宿主评分、预算、暂停/停止/恢复、最佳补丁与报告。E0–E5 均未执行，不改变 Craft 重建主线状态。

- 10:51-rea-research-module.md
  > （R0–R1 已通过，R2 部分实施）研究 REA 6.5.0 CLI/MCP/Evidence 契约与安全边界，规划 Loom 以固定版本本地 MCP Source 试点接入调研/逆向工具；CLI、doctor、静态 fixture、错误、SIGINT partial Evidence、MCP 握手、207 工具 availability 与风险映射 fixture 已验证。R2 已接官方包路径验证、直接 `rea mcp` 探针、会话 Source 激活、`mcp__rea__*` 生产权限策略、串行 stdio MCP supervisor、Claude/Pi worker 动态外部工具发现与审批链，以及允许后回调到 `ReaMcpClient.call_tool` 的真实执行桥；进度事件、Evidence UI 仍待实施。

- 01:16-macos-app-update.md
  > （代码实施完成，分发待验收）已完成更新按钮、Rust 全局安装准入、草稿恢复与双架构 GitHub Releases 草稿流水线；完整检查和无 bundle 构建通过，签名/公证凭据已于 10-16 复用配置，真实 CI 构建与两版升级仍待验收。

## 2026-09-30

- 13:17-craft-parity-rebuild.md
  > （已暂停，由 2026-10-20 简化 Chat 取代；以下为冻结时历史状态。2026-10-10 正式 Claude/Pi worker 本机 HTTP 10/10、原生连接与 Claude SDK 拒绝/停止/再发/允许/重启已补验，环境阻塞解除；bundled SDK Bun 初始化与 inventory 解析已修复，定向回归 6/6。F04 已补列 M1 标记/归档与重启恢复证据；Pi 原生拒绝已测、停止/再发/允许因锁屏 blocked，机器摘要已同步。M0/M1 未收口、M2 未到出口、F01–F23 无 passed，真实账户/生命周期能力等继续）保留 Tauri/React/Rust，以 Craft v0.14.0 固定 SHA 完整复刻，按 M0–M8 验收。

## 2026-09-22

- 14:29-chat-progress-review-fixes.md
  > （本地修复完成，真实 CLI 验收待授权）已分离 Chat/Task 路由及 Planning 订阅，活动补流按字节索引读取，修正文档与调试进程误识别；前端 172/Rust 289/集成 2、默认 app 构建与原生切换/空会话导出/正常重启通过，代码清单与证据已保存。

## 2026-09-20

- 10:12-agent-guidance-optimization.md
  > （已完成，M1–M3）统一 Chat 范围、权限现状与验证矩阵，修复续聊历史重复与配置切换句柄，精简规划提示词并支持有证据的发布状态推进；前端/Rust/桌面构建通过，真实模型行为比较未运行。

## 2026-09-17

- 10:55-craft-chat-phase2.md
  > （历史切片，P2-M0–M1/M3 已合入 main；P2-M2 defer）Ask 确认门 + 超时指示；继续工作转 10:15 重构计划。

- 10:15-local-agent-chat-rebuild.md
  > （整体仍实施中；当前切片转 09-22 Review 修复）仓储/补流/幂等、回合日志、导出与正常退出接线已落地，Chat Shell 已分离。真实文件/停止验收因私人 skill 外发待授权；运行中/异常退出、剩余交互与发布候选门禁未完成。

- 09:03-craft-inspired-local-agent-chat.md
  > （已完成合入 main，M0–M5；后续入口转 10:15）Craft Phase 1 历史；见 `docs/dogfood/craft-chat-phase1-checklist.md`。

## 2026-09-14

- 20:06-chat-first-refactor.md
  > 规划把 Loom 重构为可调用本地 Codex / Claude Code / Grok CLI 的 Grok-Bot 式 chat-first UX：新增 ChatSession 与 Chat UI，复用 agent_adapter / command_runner / stream；Board/Planning 降为 advanced；不重写任务状态机，Cloud Agents 不可用故走本机实现。

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
