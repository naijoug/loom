# Loom 完整版本实施 — Plan

- **Date**: 2026-07-24
- **Author**: Codex
- **Status**: completed
- **Progress**: M0–M6 已完成；完整门禁、发布构建与真实桌面验收通过

## 目标

将 Loom 从可 dogfood 的功能型 Alpha 推进到满足 `docs/requirements.md` 明确功能与非功能要求的完整本地开发闭环：多 Agent 规划、主 Agent 实施、独立协作 Agent Review、调试与修复循环、人工证据介入、可恢复任务、可审计安全执行和可导出的交付总结全部由真实后端能力支撑，并完成真实桌面端验收后再通知用户测试。

## 非目标

- 不在本轮实现插件市场、云同步、团队账号、多人实时协作或完整 GUI 自动操作；这些能力已在 `docs/requirements.md` 的 MVP 非强制范围中明确排除。
- 不为了架构纯度整体重写 Tauri、React 或现有 `.loom` 持久化格式；采用兼容迁移和可逆的小里程碑。
- 不引入远程服务作为本地闭环的必需依赖；Codex、Claude Code 和自定义 CLI 继续通过本地 adapter 工作。
- 不覆盖或丢弃开始实施前已有的未提交改动；当前项目移除、新任务入口和终端发现修改作为在途基线保留并验证。

## 成功标准

- 用户可登记项目、配置至少三个 Agent profile，并为 planning、implementation、review、debugging/testing 阶段设置项目默认 Agent；能力、可用状态、文件写入与命令执行权限均真实生效。
- 多 Agent 规划可并行起草、互评、合成、人工补充并生成最终计划文档，现有证据、实时日志、失败重试和 session resume 能力不回退。
- 主 Agent 可按 Todo 实施；至少一个不同的协作 Agent 能基于计划、Git diff、执行日志和验证证据完成结构化 Review，并记录 blocker、建议、可接受风险以及采纳状态。
- 存在未解决 Review blocker 时不能进入 Testing；修复后可重新 Review，修复失败可升级给另一 Agent 或人工处理。
- 调试命令、PTY、日志、错误摘要、人工反馈和自动修复循环可持续运行至少两轮；截图、日志片段、复现步骤和期望行为可作为持久化附件绑定到调试会话。
- 所有命令统一通过 Rust 执行策略：规范化 cwd、验证项目边界、执行 Agent 权限和危险命令审批、脱敏命令与日志，并记录开始/结束/退出码/终止原因。
- 应用重启后任务状态、证据和历史可恢复；孤立的运行中命令会被明确标记为 interrupted，用户可安全重试或通过支持的原生 Agent session 恢复。
- 任务支持暂停、恢复、取消和明确阻塞；状态转换由 Rust 权威状态机校验，前端不能绕过后端门禁。
- 完成任务时生成并持久化 JSON + Markdown 总结，包含完成需求、真实修改文件与 diff 统计、关键决策、Review 结果、验证证据、剩余风险和后续建议，并可从 UI 打开或导出。
- `pnpm test`、`pnpm build`、`cargo fmt --check`、严格 Clippy、`cargo test`、浏览器交互/视觉烟测和真实 Tauri 端到端验收全部通过；完成审计逐项覆盖 `docs/requirements.md` 后才通知用户测试。

## 当前状态

- `src-tauri/src/agents.rs`：已有 Codex/Claude/custom CLI 配置、规划并行执行、互评合成、结构化事件解析、失败重试、证据落盘和 session 捕获；统一 invocation 参数与阶段权限已抽到 `agent_adapter.rs`，Agent 命令路径/版本诊断已抽到 `agent_diagnostics.rs`，后续随 M3 再拆 review orchestration 和 stream parser。
- `src-tauri/src/tasks.rs`：Task、CommandRun、Todo、TaskEvent 状态使用兼容旧 JSON 的 Rust enum，并由 `task_state.rs` 统一校验；任务写入锁与 append-only evidence merge 防止并发 run/feedback 丢失，暂停/恢复/阻塞/取消和重启对账已落地。
- `src-tauri/src/command_runner.rs` 与 `src-tauri/src/pty.rs`：已有进程组停止、超时、实时日志、PTY 和日志脱敏；cwd/项目边界、Agent 权限、危险命令审批均由 Rust 执行策略校验，应用重启会把孤立 running run 对账为 interrupted。
- `src/components/TaskDetail/SessionPane.tsx` 与 `TestingPane.tsx`：实施、独立 Review、验证、人工/自动修复和终端驾驶舱均由真实后端证据驱动；结构化反馈、附件、日志搜索/过滤/折叠和重启历史读取已完成。
- `src/components/TaskDetail/DonePane.tsx`：读取持久化 TaskSummary，展示 Git baseline 归因、numstat、Review、验证、风险与建议，并支持重新生成、打开、定位和导出 JSON/Markdown。
- `src/components/Settings/SettingsPage.tsx`：已有主题、Agent CRUD、终端槽位、安全提示和 About；项目级 planning/implementation/review/debugging/testing/documentation 默认 Agent 会保存到 `.loom/agent-preferences.json`，并显示后端探测的可执行路径、版本和认证提示。
- `scripts/` 与 `tests/`：`pnpm check` 和 GitHub Actions 共用质量门禁；最终门禁包含 85 项前端/领域测试和 166 项 Rust 测试（另 1 项真实 Claude 凭证测试按设计忽略），主 bundle 为 390.18KB；流程、交互、960px 响应式和 26 屏深浅主题视觉冒烟均通过。
- 发布构建已生成可运行的 `Loom.app` 和校验有效的 arm64 DMG；真实发布版桌面已验证项目加载、旧任务兼容降级、技术栈识别和 Tauri 后端健康。完整证据见 `docs/dogfood/complete-version-2026-07-24.md`。
- 开始实施前已有的用户未提交改动未被重置或覆盖；最终工作树仍保持未提交，交由用户决定后续提交边界。

## 里程碑

### M0 — 基线、权威状态机与工程门禁

**Status**: completed（2026-07-24）

**Outcome**: 当前在途改动被安全纳入，任务/命令状态和转换有 Rust 单一真相源，基础质量门禁可持续执行。

| # | Task | Files / Symbols | Depends on | Verification |
|---|------|-----------------|------------|--------------|
| 0.1 | 完成并验证当前项目移除、新任务入口和终端扫描在途改动，补齐 Rust/前端回归测试 | `src-tauri/src/projects.rs`, `storage.rs`, `terminals.rs`, `src/App.tsx`, `Navigation.tsx`, `reducer.ts` | — | `pnpm test`, `pnpm build`, targeted `cargo test` |
| 0.2 | 将 Task/Command/Todo 状态改为兼容旧 JSON 的 Rust enum，并集中合法状态转换 | `src-tauri/src/models.rs`, 新建 `src-tauri/src/task_state.rs`, `tasks.rs` | 0.1 | 旧 fixture 反序列化、每条合法/非法转换 Rust 测试 |
| 0.3 | 用 request/update 结构体收敛 command/task 多参数接口并修复严格 Clippy | `command_runner.rs`, `tasks.rs`, `models.rs` | 0.2 | `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings` |
| 0.4 | 建立 CI 与一键完整检查入口 | `.github/workflows/ci.yml`, `package.json`, `scripts/check.sh` | 0.3 | 本地运行完整 check；工作流 YAML 可解析 |

### M1 — 安全、可恢复的执行内核

**Status**: completed（2026-07-24）

**Outcome**: 普通命令、Agent、Review 和 PTY 都经过同一后端安全策略；应用重启后不会留下伪运行状态。

| # | Task | Files / Symbols | Depends on | Verification |
|---|------|-----------------|------------|--------------|
| 1.1 | 实现 Rust `ExecutionPolicy`：canonical cwd、项目根边界、危险分类、权限、审批凭证、环境变量/日志脱敏 | 新建 `src-tauri/src/execution_policy.rs`, `models.rs`, `settings.rs` | M0 | policy 表驱动测试覆盖删除、reset、安装、生产目标和路径逃逸 |
| 1.2 | 将 command runner、PTY、planning/implementation/debug/review Agent 调用统一接入策略 | `command_runner.rs`, `pty.rs`, `agents/*`, 前端 bridges | 1.1 | 绕过前端直接 invoke 仍被拒绝；正常验证与 Agent run 通过 |
| 1.3 | 持久化 run lifecycle 并在启动/加载时 reconcile 孤立的 `running` run 为 `interrupted` | `command_runner.rs`, `pty.rs`, `tasks.rs`, 新建 `run_recovery.rs` | 1.2 | 模拟重启测试；interrupted run 可重试且保留历史证据 |
| 1.4 | 实现任务暂停、恢复、取消、阻塞及运行中进程联动 | `task_state.rs`, `tasks.rs`, `useTaskBridge.ts`, Header/TaskDetail controls | 1.3 | Rust 状态机 + UI 交互测试；取消会停止关联进程并记录原因 |
| 1.5 | 收紧 Tauri CSP 与 capability，只开放主窗口/计划查看器需要的权限 | `src-tauri/tauri.conf.json`, `capabilities/default.json`, plan viewer | 1.2 | Tauri dev/build 正常；危险外链/内联脚本被拒绝 |

### M2 — 可扩展 Agent 与项目阶段偏好

**Status**: completed（2026-07-24）

**Outcome**: Agent adapter 位于稳定接口之后，所有阶段共用 invocation 服务；项目可以持久化阶段默认 Agent 和权限偏好。

| # | Task | Files / Symbols | Depends on | Verification |
|---|------|-----------------|------------|--------------|
| 2.1 | 从 `agents.rs` 抽出稳定 adapter trait、Codex/Claude/custom CLI 参数映射和诊断模块；stream parser/evidence writer 在 M3 按 review service 边界继续拆分 | `agent_adapter.rs`, `agent_diagnostics.rs`, `agents.rs` | M1 | 原有 planning/real CLI fixture 与新增 adapter/diagnostic 测试全过 |
| 2.2 | 统一 planning/implementation/review/debug 的 invocation request、能力校验、权限映射、session resume 和结构化结果 | `agent_adapter.rs`, `agents.rs`, `useAgentBridge.ts`, TaskDetail panes | 2.1 | Codex sandbox、Claude permission mode、自定义占位符、无权限失败契约测试 |
| 2.3 | 新增项目级阶段默认 Agent 与运行偏好持久化 | `project_preferences.rs`, Settings/Agent UI, NewTask/TaskDetail panes | 2.2 | 偏好原子落盘；重复、禁用、无能力 Agent 正规化；新任务和调试选择继承默认值 |
| 2.4 | 强化 Agent 可用性诊断：命令路径、版本、启用状态和登录状态提示 | `agent_diagnostics.rs`, Settings | 2.1 | 缺失命令与版本输出 fixture；真实诊断 5 秒超时且不阻塞主流程 |

### M3 — 真实实施 Review 与修复闭环

**Status**: completed（2026-07-24）

**Outcome**: 独立协作 Agent 对实现结果进行结构化 Review；blocker 会阻止进入 Testing，并能驱动修复与重新 Review。

| # | Task | Files / Symbols | Depends on | Verification |
|---|------|-----------------|------------|--------------|
| 3.1 | 新增 `ImplementationReviewRun/Finding/Decision` 模型、状态与持久化命令，和 planning `PlanReview` 明确分离 | `models.rs`, `implementation_review.rs`, TS domain | M2 | JSON 兼容默认、finding 分类、决策与恢复测试 |
| 3.2 | 构建 Review context：最终计划、Todo、Git status/diff、实施/验证日志、用户决策和约束 | `implementation_review.rs`, project git command helpers | 3.1 | 上下文预算截断、无 Git 降级、结构化 JSON 解析测试 |
| 3.3 | 实现协作 Agent 选择、串行 Review、持久化原始输出/退出码/stderr 和结果分类 UI | `ImplementationReviewPanel.tsx`, bridge, adapter process runner | 3.2 | 真实进程 capture smoke；blocker/risk/suggestion/info 展示与前端 gate 测试 |
| 3.4 | 将 blocker 修复、重新 Review、升级另一主 Agent 和人工接受风险接入循环；主 Agent 切换强制记录原因 | review panel/service、`tasks.rs`, command runner | 3.3 | blocker repair handoff、pending re-review、accepted risk 和 switch reason 测试 |
| 3.5 | 收紧进入 Testing 门禁：至少一个不同协作 Agent Review，且无未解决 blocker | `implementation_review::ensure_review_gate`, `tasks.rs`, UI gate | 3.4 | 直接 Tauri command 不能绕过；缺 Review、自审、失败 Review、open blocker 均拒绝 |

### M4 — 人工反馈、附件与调试证据

**Status**: completed（2026-07-24）

**Outcome**: 用户可用结构化问题、复现步骤、期望行为、截图和日志片段介入，并将证据可靠交给 Agent。

| # | Task | Files / Symbols | Depends on | Verification |
|---|------|-----------------|------------|--------------|
| 4.1 | 扩展反馈模型并实现附件复制/校验/元数据持久化，限制类型、大小和项目边界 | `models.rs`, `tasks.rs`, 新建 `attachments.rs`, Tauri commands | M1 | 图片/文本/超限/路径逃逸 fixture 测试 |
| 4.2 | Testing UI 支持问题、复现步骤、期望行为、截图/文件和选中日志统一提交 | `TestingPane.tsx` 拆分后的 FeedbackComposer、bridges | 4.1 | 组件交互 + 真实文件选择桌面 smoke |
| 4.3 | repair context 纳入附件摘要与可读引用，Agent 修复结果绑定验证证据 | `context_builder.rs`, agent invocation、task events | 4.2, M3 | prompt 预算与附件引用测试；修复事件可追溯 |
| 4.4 | 完成日志 URL/端口/警告/测试失败解析、搜索、过滤、折叠和历史 run 导航 | `command_runner.rs`, log parser、Testing components | M1 | 常见 Node/Rust/Go/Python/Flutter fixture + UI 交互测试 |

### M5 — 交付总结、导出与任务历史

**Status**: completed（2026-07-24）

**Outcome**: 完成页由真实、持久化的交付产物驱动，可导出并在重启后复现。

| # | Task | Files / Symbols | Depends on | Verification |
|---|------|-----------------|------------|--------------|
| 5.1 | 在实施开始时记录 Git baseline，在运行中收集真实文件变化和 diff/numstat；无 Git 项目使用文件证据降级 | project git service、`tasks.rs`, models | M3 | clean/dirty/untracked/无 Git fixture 测试 |
| 5.2 | 生成并持久化 `TaskSummary` JSON + Markdown，覆盖需求、文件、决策、Review、验证、风险、后续建议 | 新建 `summary.rs`, `.loom/tasks/<id>/summary.*` | 5.1, M4 | summary snapshot + 路径/内容完整性测试 |
| 5.3 | 完成页改为读取 summary，提供重新生成、打开与导出入口 | `DonePane.tsx`, task bridge, opener/dialog | 5.2 | 重启后展示一致；导出文件可读且不泄露 secret |
| 5.4 | 完整任务时间线合并 planning、implementation、review、debug、repair、feedback、lifecycle 和 summary | `taskTimeline.ts`, task events | 5.2 | 排序、去重、证据引用单元测试 |

### M6 — 结构收敛、真实端到端验收与发布准备

**Status**: completed（2026-07-24）

**Outcome**: 完整版本可维护、可重复验证，并具备明确的用户测试入口和剩余风险说明。

| # | Task | Files / Symbols | Depends on | Verification |
|---|------|-----------------|------------|--------------|
| 6.1 | 按加载边界拆分 App/Preview、Board/Workspace/Settings，抽出 adapter、review、policy、summary 等高风险逻辑并保持页面契约 | `src/components`, `src/state`, `src/utils`, Rust services | M3-M5 | 85 项前端测试全过；最大 chunk 390.18KB，无 >500KB 警告 |
| 6.2 | 建立可重复的完整回归入口，并组合真实进程两轮 repair/summary Rust E2E、严格 dogfood task 证据与发布版桌面回放 | `tests/e2e`, scripts, dogfood docs | M5 | task JSON、日志、失败/修复 trace、桌面截图、summary 与严格 dogfood 校验全部通过 |
| 6.3 | 更新架构、adapter、状态机、安全策略、README 和计划索引 | `docs/architecture.md`, `agent-adapter.md`, `task-state-machine.md`, `security-policy.md`, `README.md`, `docs/PLANS.md` | M1-M5 | 文档与最终代码符号逐项核对 |
| 6.4 | 完成深浅主题、960–1440px 响应式、中英文一致性、键盘与可访问性检查 | UI/CSS, preview fixtures, visual scripts | 6.1 | 交互 smoke、26 屏视觉截图、960px 溢出断言和人工桌面检查 |
| 6.5 | 执行需求逐项 completion audit、完整检查、Tauri release build 和手工桌面验收；生成测试说明 | requirements matrix、`scripts/check.sh`, dogfood report | 6.2-6.4 | 所有需求有直接证据；无缺项后才通知用户测试 |

## 风险

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| 当前工作树已有未提交改动，与大规模重构冲突 | H | H | M0 先验证并保留现有 diff；后续按小文件/小里程碑修改，禁止 reset/覆盖 |
| `.loom` schema 从字符串状态迁移到 enum 破坏旧任务 | M | H | serde alias/default + migration fixture；不原地破坏不可解析文件，保留备份 |
| 后端审批策略过严导致正常 Agent/终端无法运行 | M | H | policy 返回可解释 finding；安全默认 + 显式一次性 approval；每类合法命令 fixture |
| Review Agent 与主 Agent 相同或共享偏见，门禁形同虚设 | M | H | 默认要求不同 agent id；记录 reviewer 与证据；允许第二 reviewer/人工决策但不能静默跳过 |
| Git diff 包含用户原有脏改动，summary 归因错误 | H | H | 实施前捕获 baseline；区分 pre-existing、task-introduced、unknown，并在 UI 明示 |
| 自动循环/重启并发更新 task JSON 造成事件丢失 | M | H | 单任务写锁、原子写、run id 幂等 finish/reconcile；并发测试 |
| 真实 CLI 测试依赖本机登录状态、耗时和额度 | H | M | deterministic fixture 保持默认；真实 canary 独立标记并保存失败证据，不以 Preview 替代 |
| 大文件拆分同时改变行为导致回归 | M | H | 先补契约测试再移动代码；每次拆分保持可编译、无功能变化 |

## 待确认问题

- [x] “完整版本”按 `docs/requirements.md` 明确需求验收；插件市场、云同步、团队账号和完整 GUI 自动化保持非目标。
- [x] Review Agent 默认只读，且默认必须与主实施 Agent 不同；人工接受风险需要明确理由并进入审计流。
- [x] 重启后不尝试无条件附着任意子进程；将孤立 run 标记为 interrupted，对支持的 Agent session 提供显式 resume。
- [x] 任务总结同时保存 JSON 和 Markdown，UI 展示持久化结果而不是临时推导。
- [x] worktree 隔离和 verifier sub-agent 不属于当前 `docs/requirements.md` 强制范围，保留为后续增强，不阻塞本版本完成。

## 验证策略

- 每个里程碑：先补契约/失败路径测试，再改实现；运行最小相关前端/Rust 测试、构建和严格静态检查。
- 每个跨阶段门禁：必须同时验证 UI 路径和直接 Tauri invoke 路径，避免只在前端禁用按钮。
- 持久化：所有新 schema 使用旧 fixture、损坏文件、重启和幂等更新测试；迁移失败保留可恢复备份。
- 安全：针对路径逃逸、危险命令、权限禁用、secret 脱敏和附件边界运行表驱动测试。
- 端到端：使用临时 Git 项目和真实桌面应用保存完整任务证据；Preview smoke 只作为布局/交互补充，不作为后端闭环证明。
- 最终验收：逐项映射 `docs/requirements.md` 和本计划成功标准到文件、测试、task JSON、日志、Review、summary、截图或 release build 证据；缺少直接证据即继续实施。
- 回滚：每个里程碑保持 schema 向后兼容和小范围提交；失败时回退新入口而保留旧数据与日志，禁止破坏用户工作树。
