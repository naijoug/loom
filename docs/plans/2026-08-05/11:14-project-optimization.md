# Loom 项目稳定化与可维护性优化 — Plan

- **Date**: 2026-08-05
- **Author**: Codex
- **Status**: in-progress
- **Progress**: M0–M4 已完成；M5 实施中

## 目标

在不扩张产品范围的前提下，把当前“功能完整但大规模未提交”的 Loom 版本固化为可追溯、可迁移、可持续演进的工程基线；降低进程编排、任务持久化、前后端契约和复杂页面继续迭代时的回归风险。

## 非目标

- 本轮不新增插件市场、云同步、团队账号、多人协作或远程服务。
- 不整体重写 Tauri、React、现有四阶段工作流或 `.loom/` 数据格式。
- 不在没有指标证据前做微观性能优化或替换现有技术栈。
- 不把 Apple Developer ID 签名、公证和公开分发纳入工程稳定化的完成条件；如要公开发布，另立发布计划。
- 不替用户决定当前工作区 55 个已修改条目和 30 个未跟踪条目的最终提交边界。

## 成功标准

- 当前完整版本被拆成可审查的功能提交，并有明确的基线 commit/tag；工作区不再承担唯一版本载体。
- 所有持久化任务都有显式 schema version、逐版本迁移和旧版本 fixture；升级失败能备份并给出可恢复提示。
- Rust/TypeScript command、event 和持久化模型契约有自动一致性验证，新增或改名接口时 CI 能直接失败。
- Agent、Review、普通命令和 PTY 共享统一的进程生命周期边界；停止、超时、重启对账和日志元数据不再在多个模块重复实现。
- `agents.rs`、`tasks.rs` 和四个超大 React 容器完成按职责拆分；核心业务模块和页面容器有明确体积预算与依赖方向。
- 核心四阶段至少有组件级交互测试和真实 Tauri 集成路径；视觉 smoke 能发现像素级回归，而不只检查文案和截图大小。
- PR CI 保持快速，主干或定时任务覆盖浏览器 smoke、真实桌面健康检查和兼容性 fixture。
- 关键页面统一中英文、主次操作和阻塞原因表达，并能导出脱敏诊断包。

## 当前状态

### 已具备的能力

- `docs/requirements-audit.md` 已把需求逐项映射到实现与测试，四阶段闭环、独立 Review、安全执行、任务恢复、附件证据和交付总结均有真实 Rust/React 实现。
- `src-tauri/src/task_state.rs` 已提供 Rust 权威状态机，`src-tauri/src/execution_policy.rs` 已集中项目边界、危险命令和审批策略。
- `src-tauri/src/agent_adapter.rs` 已隔离 Codex、Claude 和 custom CLI 参数映射，核心流程没有直接依赖单一 Agent。
- `scripts/check.sh` 已统一 TypeScript 构建、Rust fmt、严格 Clippy 和 Rust 测试；M2 后前端/领域测试为 90/90，Rust 为 187 项通过（另 1 项真实凭据测试按设计忽略）。
- `pnpm smoke`、`pnpm smoke:interaction`、`pnpm smoke:visual` 本次评审全部通过，30 张 1440×940 深浅主题截图生成成功。
- M0 已将完整版本拆成四个功能提交并建立 `stabilization-baseline-2026-08-05` 标签；release/DMG checksum 见 `docs/dogfood/stabilization-baseline-2026-08-05.md`。
- M1 已为五类 store 增加版本信封、v0→v1 原子迁移、未来/损坏版本备份；`contracts/tauri-contract.json` 同时约束 56 个 command、5 个 event、状态枚举、schema 版本和关键 wire model sample，前端 Tauri 调用已集中到 `src/api/`。
- M2 已用 `ProcessSupervisor` 统一 Agent/command/Review/PTY 的 run metadata、进程组终止、timeout/stop reason 与恢复判定；`TaskRepository` 统一任务锁、原子 read-modify-write、append-only evidence merge 和锁回收。`agents.rs`/`tasks.rs` 生产根模块分别降至约 878/946 行，拆出的职责模块均低于 1,000 行。
- M3 已将单体 reducer 拆为项目、任务、命令、规划等 8 个 domain 模块；Testing 的终端网格、验收门禁、反馈编辑器、修复历史和证据模型已进入 `src/features/testing/`，旧容器由 1,372 行降至 886 行。Planning/Settings 公共入口均降为 1 行，feature 主模块分别为 844/932 行，最终计划视图、Agent draft 与通用控件已独立；Planning/Settings CSS 已随 feature 共置。95 项前端测试、生产构建、interaction smoke 与 30 屏深浅主题 visual smoke 均通过。
- M4 已加入 jsdom React 交互层、typed Tauri invoke/listen transport 测试和 Rust command/service 集成 harness；四阶段、blocker、两轮 repair、暂停/仓储重载可 headless 重放。release app 两轮启动/退出 smoke 通过；30 屏视觉 smoke 已升级为固定 8×6 感知采样基线、阈值判定与 diff artifact。CI 已拆为 PR `quality-gate`、主干浏览器回归和定时/手工桌面 release 兼容性三层。当前前端测试为 99 项。

### 主要差距

- **交付基线已固化（M0）**：完整闭环已进入 `codex/project-stabilization` 的可审查提交与稳定化标签；公开分发仍缺 Developer ID 签名、公证和多平台矩阵。
- **前后端巨型模块已拆分（M2–M3）**：Agent/Task 生产模块与前端 Testing/Planning/Settings/reducer 均已按职责下沉；公共入口与各核心生产模块均低于 1,000 行，后续继续以 feature 为依赖边界。
- **进程运行时已统一（M2）**：四类进程共享 supervisor metadata、进程组终止与恢复语义；PTY 只保留 transport 特有句柄和读写线程。
- **持久化迁移已显式化（M1）**：五类 store 已有独立 schema version 和 v0→v1 入口；下一次字段重命名或语义变化必须新增逐版本迁移函数与 fixture。
- **前后端契约已有门禁（M1）**：canonical fixture 与 typed client 已覆盖 command/event/schema/status/关键模型；command payload 的进一步细化随 M2 service 拆分继续收紧。
- **测试层已补齐（M4）**：纯逻辑、React DOM 交互、typed invoke/listen、Rust command/service、浏览器和 release 桌面生命周期均有独立证据；真实 Agent 凭据 canary 继续保留为手工路径，不进入确定性 CI。
- **视觉回归已有门禁（M4）**：30 个深浅主题页面同时执行 DOM、尺寸与感知采样差异检查；失败会保留截图、DOM、Chrome 日志和 diff artifact。基线需在明确接受设计变化时人工更新。
- **界面信息密度较高**：Planning、Testing、Done 页面已经具备完整证据，但中英文混用、右侧驾驶舱纵向堆叠和同级操作过多，首次使用时需要更明确的“当前阻塞原因 / 下一步动作”。

### 可复用部分

- 复用 `task_state.rs` 的状态转换和 gate，不在前端复制权威规则。
- 复用 `execution_policy.rs`、`agent_adapter.rs`、`storage::atomic_write_*` 和现有脱敏能力。
- 复用当前 Node `node:test` 纯函数测试、Rust 单元/集成测试与 Chrome smoke；新增测试层，而不是推倒现有门禁。
- 复用 `PlanningPreviewApp.tsx` 作为纯视觉 fixture，但把它明确限定为 UI fixture，不作为桌面端到端证据。

## 里程碑

### M0 — 固化可追溯基线

**Outcome**: 完整版本不再只存在于脏工作区中，评审、回滚和后续拆分都有稳定起点。

| # | Task | Files / Symbols | Depends on | Verification |
|---|---|---|---|---|
| 0.1 | 按“后端能力、前端接线、测试/文档、设计审计”生成当前 86 个工作区条目的归属清单，标出用户原有改动和完整版本改动 | `git status`, `git diff`, `docs/dogfood/complete-version-2026-07-24.md` | — | 清单覆盖每个 modified/untracked 条目，无未归属文件 |
| 0.2 | 在用户确认提交边界后建立 `codex/` 前缀稳定化分支，将现状拆成可独立回滚的功能提交 | repository history | 0.1 | 每个提交 `pnpm check` 通过；最终 `git status` 仅保留用户明确不提交的内容 |
| 0.3 | 校准 README、需求审计、计划索引和实际测试计数，记录 commit、构建环境和未覆盖项 | `README.md`, `docs/requirements-audit.md`, `docs/PLANS.md`, `docs/dogfood/` | 0.2 | 文档中的测试数、版本号、commit 与实跑输出一致 |
| 0.4 | 为稳定基线生成可复现的本地 release 并记录 checksum；公开分发条件继续单列 | `src-tauri/tauri.conf.json`, `docs/dogfood/` | 0.3 | clean checkout 可重建 app/DMG，checksum 和启动健康检查有记录 |

### M1 — 持久化与前后端契约安全

**Outcome**: 数据和 Tauri API 的演进不再依赖隐式兼容与人工记忆。

| # | Task | Files / Symbols | Depends on | Verification |
|---|---|---|---|---|
| 1.1 | 为 Task、Agent 配置、Settings、终端槽位和项目偏好定义独立 schema version 与兼容策略 | `src-tauri/src/models.rs`, `storage.rs`, `settings.rs`, `agents.rs`, `terminals.rs`, `project_preferences.rs` | M0 | 新旧 JSON fixture 都能读取；未来版本被明确拒绝并备份 |
| 1.2 | 新建显式迁移入口，先实现 v0→v1 规范化；读取不再只依赖 `serde(default)` | new `src-tauri/src/migrations.rs`, `tasks::load_task` | 1.1 | 旧任务 fixture 迁移后字段、状态、证据引用无损；迁移幂等 |
| 1.3 | 用 Rust 序列化生成/校验一组 canonical contract fixtures，并由 TypeScript 测试消费 | `src-tauri/tests/fixtures/`, `tests/contracts/`, `src/domain/*.ts` | 1.1 | 字段名、枚举值、可选性漂移会让 `pnpm check` 失败 |
| 1.4 | 集中 Tauri command/event 名称与 payload 声明，桥接层只通过 typed client 调用 | new `src/api/`, `src/hooks/use*Bridge.ts`, `src-tauri/src/lib.rs` | 1.3 | 56 个注册 command 与实际调用/允许未调用清单一致；事件名有双端契约测试 |
| 1.5 | 将计划、评审证据和索引写入切换为原子写，并设计多文件产物失败后的恢复顺序 | `agents.rs` plan/evidence writers, `implementation_review.rs`, `storage.rs` | 1.2 | 注入中途失败后不会出现半份 JSON/Markdown 或索引悬空引用 |

### M2 — 统一进程监督与任务仓储

**Outcome**: 所有本地进程共享一致的停止、超时、日志、恢复和审计语义，任务更新由单一仓储处理。

| # | Task | Files / Symbols | Depends on | Verification |
|---|---|---|---|---|
| 2.1 | 提取普通子进程的 `ProcessSupervisor`，统一 process group、timeout、stop reason、registry 和退出结果 | new `src-tauri/src/process_supervisor.rs`, `command_runner.rs`, `agents.rs`, `implementation_review.rs` | M1 | 相同 fixture 对 command/agent/review 产生一致终止结果；无孤儿子进程 |
| 2.2 | 为 PTY 保留 transport 特性，但复用统一 run metadata、停止语义与 recovery contract | `pty.rs`, `process_supervisor.rs`, `run_recovery.rs` | 2.1 | PTY 与非 PTY 的 cancelled/interrupted/timeout 证据一致 |
| 2.3 | 把任务文件锁、read-modify-write、append-only evidence merge 抽成 `TaskRepository` | new `src-tauri/src/task_repository.rs`, `tasks.rs` | M1 | 并发 run/feedback/review 测试通过；锁注册表能在任务结束/删除后释放 |
| 2.4 | 将任务 mutation 改成显式 command/service，避免整份 stale `Task` 回写覆盖标量状态 | `tasks.rs`, `task_state.rs`, `task_repository.rs` | 2.3 | 并发暂停、run 完成、Review 决策和 summary 生成无丢更新 |
| 2.5 | 按职责拆分 Rust 巨型模块 | `agents/{config,planning,orchestrator,stream,artifacts}.rs`, `tasks/{commands,context,lifecycle,testing}.rs` | 2.1, 2.4 | 原公共 Tauri API 不变；核心生产模块目标 ≤1,200 行，循环依赖为 0 |

### M3 — 前端按流程边界拆分

**Outcome**: 页面组件只负责展示和用户意图，编排状态、桥接调用和纯决策逻辑可独立测试。

| # | Task | Files / Symbols | Depends on | Verification |
|---|---|---|---|---|
| 3.1 | 将 `TestingPane` 拆为 controller hook、terminal grid、validation gate、feedback composer 和 repair-cycle history | `src/components/TaskDetail/TestingPane.tsx`, new `src/features/testing/` | M1 | Auto/Manual、验证 gate、附件和停止流程行为不变；各子模块有测试 |
| 3.2 | 拆分 Planning 时间线的 round、draft、cross-review、synthesis、final-plan 视图 | `PlanningTimeline.tsx`, new `src/features/planning/` | M1 | 失败重试、日志展开、重审和确认计划场景通过 |
| 3.3 | 将 Settings 按 tab 拆分，Agent 表单和终端槽位编辑独立建模 | `SettingsPage.tsx`, new `src/features/settings/` | M1 | 保存、失败回滚、项目切换和诊断状态有组件测试 |
| 3.4 | 按 domain 拆分 reducer/action/selectors，保持单一 AppStateContext 以控制迁移风险 | `src/state/reducer.ts`, new `src/state/reducers/` | 3.1–3.3 | 现有 24 个 reducer 测试迁移后全部通过；新增跨项目缓存测试 |
| 3.5 | 清理 7,566 行主样式中的历史/跨页面选择器，样式随 feature 共置并统一 token | `Workspace.css`, `TaskDetail.css`, `Planning.css`, `SettingsPage.css`, `theme.css` | 3.1–3.3 | 深浅主题 pixel diff 在阈值内；960/1280/1440 无新增溢出 |

### M4 — 补齐测试金字塔与持续交付

**Outcome**: 纯逻辑、React 交互、Tauri command、桌面生命周期和视觉回归各有清晰证据层。

| # | Task | Files / Symbols | Depends on | Verification |
|---|---|---|---|---|
| 4.1 | 引入最小 React 组件测试运行时，优先覆盖 Planning、Implementation Review、Testing gate、Done summary 和 Settings 保存 | `package.json`, `tests/components/`, M3 feature modules | M3 | 关键操作断言 typed client 请求、loading/error/disabled 状态和恢复路径 |
| 4.2 | 新增 command-level 集成 harness，用临时项目调用 Rust service/command 内核 | `src-tauri/tests/`, `task_repository.rs`, `process_supervisor.rs` | M2 | 四阶段 happy path、blocker、两轮 repair、暂停/重启均可 headless 重放 |
| 4.3 | 建立真实 Tauri 桌面 smoke，验证 invoke/listen、窗口重启和历史恢复；真实 Agent 凭据继续作为显式 canary | `tests/e2e/`, `scripts/start-local.sh` | 4.1, 4.2 | 无真实凭据可跑确定性路径；有凭据时可额外跑 Codex/Claude canary |
| 4.4 | 将视觉 smoke 升级为基线图差异与失败 artifact，保留 DOM/尺寸检查 | `scripts/visual-smoke.sh`, `designs/audits/` or test artifacts | 3.5 | 人为引入布局偏移、主题色回退时测试失败并输出 diff 图 |
| 4.5 | CI 拆成 fast check、browser smoke、兼容性/桌面矩阵；避免每个 PR 都重复构建 release | `.github/workflows/ci.yml` | 4.1–4.4 | PR 门禁时间可控；主干/定时任务保留完整 artifact 和失败日志 |

### M5 — 降低用户认知负担并补运行诊断

**Outcome**: 用户能迅速判断当前状态、阻塞原因和下一步，问题报告也包含足够且安全的证据。

| # | Task | Files / Symbols | Depends on | Verification |
|---|---|---|---|---|
| 5.1 | 统一核心流程语言策略，消除同一区域中非必要的中英文混用 | Planning/TaskDetail/Settings components, copy constants | M3 | 中英文术语表审查；截图中状态/动作/证据用语一致 |
| 5.2 | 为每阶段增加单一 primary action 和结构化“为什么不能继续”，次要证据默认折叠 | `Header.tsx`, Planning/Session/Testing/Done feature modules | 5.1 | 新用户可在不查看文档时完成 happy path；所有 gate 都显示原因和修复入口 |
| 5.3 | 收敛 Testing 右侧驾驶舱的信息层级，区分验收门禁、最近失败、修复动作和历史 | testing feature modules, `TaskDetail.css` | 5.2 | 960/1440 下主要动作首屏可见，键盘顺序与视觉层级一致 |
| 5.4 | 新增脱敏诊断包：版本、系统、task id、run metadata、策略判定和用户选择的日志片段 | new `src-tauri/src/diagnostics.rs`, Settings/Done UI | M2 | secret fixture 不泄露；报告问题所需信息可一键导出 |
| 5.5 | 建立本地性能预算和测量脚本：冷启动、项目扫描、1 万行日志、长任务内存、timeline 渲染 | `scripts/bench-*`, relevant Rust/React modules | M2, M3 | 形成基线报告；只有超预算项进入后续优化，不凭感觉改写 |

## 优先级建议

1. **P0：M0 + M1**。先解决版本唯一载体、数据迁移和契约漂移；这是继续开发前的硬前置。
2. **P1：M2 + M4.2/4.3**。统一最容易产生高影响故障的进程与持久化路径，并用真实集成证据锁住。
3. **P2：M3 + M4.1/4.4/4.5**。降低前端变更半径，补组件和视觉回归门禁。
4. **P3：M5**。在工程底座稳定后优化认知负担、诊断和有证据的性能问题。

## 风险

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| 拆分当前脏工作区时误混入用户原有改动 | H | H | M0 先做逐文件归属清单；用户确认后才创建提交，不执行 reset/checkout |
| Task migration 损坏真实 `.loom` 历史 | M | H | 只迁移副本；原文件版本化备份；golden fixture + 幂等测试；失败时只读降级 |
| 统一进程 supervisor 改变 PTY 或 session resume 行为 | M | H | 先统一 metadata/stop contract，PTY transport 分阶段迁移；保留旧路径开关直至 smoke 通过 |
| 大模块拆分引发纯移动噪音，掩盖行为变化 | H | M | “机械移动”和“行为修改”分提交；每步跑现有门禁；先建 characterization tests |
| 新测试依赖增加安装与 CI 成本 | M | M | 只引入覆盖 JSX/桌面边界所需的最小工具；先记录当前门禁耗时和依赖大小 |
| pixel diff 跨平台不稳定 | M | M | 固定 Chrome、字体、DPR 和 viewport；按平台维护阈值，只对稳定页面设硬门禁 |
| 过度重构延迟真实用户反馈 | M | H | 每个里程碑必须可单独发布；M0/M1 后继续允许小型用户验证，不等待全部完成 |

## 待确认问题

- [x] 当前 86 个工作区条目已按整文件保留并固化到稳定化分支；混合来源记录在 `docs/dogfood/stabilization-baseline-2026-08-05.md`。
- [ ] 下一阶段目标是“个人 dogfood 稳定版”还是“可公开分发的 Beta”？后者需要另加签名、公证、升级与多平台矩阵。
- [ ] 是否承诺读取 2026-07-24 以前的所有 `.loom` 任务；如果只承诺最近一个 schema，迁移范围可明显收窄。
- [ ] Windows/Linux 是否属于近期支持范围；若否，进程 supervisor 可先以 macOS/Linux 为验收矩阵。
- [ ] UI 默认语言是中文、英文，还是正式做 i18n；当前建议先统一为一种默认语言，不直接引入完整国际化框架。
- [ ] 真实 Agent canary 可以使用哪些本机 CLI/账号，哪些必须保持手工触发以避免消耗额度？

## 验证策略

- **每个任务**：先补 characterization/contract test，再做移动或行为修改；同一提交只承担一种风险。
- **每个里程碑**：运行 `pnpm check`；涉及 UI 时运行 `pnpm smoke:interaction` 和升级后的 visual diff；涉及进程/持久化时运行对应 Rust integration fixtures。
- **端到端**：在临时 Git 项目完成“规划→实施→独立 Review→失败验证→两轮修复→完成→重启恢复→导出总结”，并保留脱敏证据。
- **发布前**：从 clean checkout 构建 Tauri release，验证 app 启动、项目注册、旧任务迁移、任务恢复和 DMG checksum。
- **回滚**：按里程碑独立提交；数据迁移保留原版本备份；进程 supervisor 和新前端 feature 在稳定前保留可切换旧路径。
