# 计划功能 v2：时间线工作台 + 自动互评 + 失败重试 + 应用内计划查看器 — Plan

- **日期**: 2026-06-11
- **作者**: Claude (Fable 5)
- **状态**: implemented（2026-06-11，M1-M5 已实施；真实 CLI 端到端手测待用户在桌面端验证）

## 实施记录（2026-06-11）

- M1-M5 全部完成；`cargo test`（85 通过）、`cargo clippy`（无警告）、`pnpm test`（17 通过）、`pnpm build` 通过；preview 页可视验证时间线各状态（成功/失败/重试入口/互评徽章/合成来源/最终计划工具栏）。
- 与计划的偏差：
  - 5.1 `PlanningWizard.tsx` / `PlanningPane.tsx` 全仓库无引用（含 preview），直接删除。
  - 失败 Agent 的 Retry 按钮对 `not_retryable` 也保留（同时显示修复提示）：用户修复配置（如 Amp credits）后需要重试入口。
  - 待确认问题"计划文件按任务复用"按倾向方案实施：同一任务后续轮覆盖首轮 `docs/plans/` 文件。
  - `pnpm smoke:interaction` 在干净工作树上同样失败（Chrome CDP fetch 环境问题），与本次变更无关。
- 待人工验证：桌面端真实 CLI 跑一轮 起草→自动互评→合成；"Open in Loom" 查看器窗口在 dev 与打包两种模式下打开。

## 背景与问题（用户反馈驱动）

`2026-06-10/17:07-planning-redesign.md` 实施后，用户实测反馈计划功能仍不可用，问题归纳为五条：

1. **右侧摘要面板没有价值**（`PlanningChat.tsx:633` 的 `planning-summary` aside）：Consensus 重复 `discussionSummary`、Risks 依赖手动点 "Run reviews"、Human decisions 几乎不用、"Create tasks from plan" 埋在最底部。四张卡占据 1/3 宽度却不承载主流程。
2. **主区内容杂乱不可读**：`planning-thread` 把所有历史 `agentInvocations` 平铺（`PlanningChat.tsx:489`），失败 Agent 的完整 stderr 以 `<pre>` 大段插入聊天流；候选计划、进度卡、Summary 全部顺序堆叠，多轮讨论后没有任何分组与折叠。
3. **计划 HTML 只能外部浏览器打开**：`open_plan_html`（`plan_html.rs:62`）走 `tauri-plugin-opener` 调系统浏览器；应用内只有 `sandbox=""` 的小 iframe 预览，无法作为正式阅读界面。
4. **Agent 失败无重试**：`run_planning_agent_with_status`（`agents.rs:503`）失败即终态。截图中 Codex/Claude/Amp 三个全挂、最终计划落到 deterministic fallback，用户只能重新发一整轮讨论。也不区分"可重试失败"（超时、偶发非零退出）和"不可重试失败"（Amp 无 credits、CLI 不存在、认证失败）。
5. **互评（cross-review）不在主流程里**：`run_plan_reviews`（`agents.rs:404`）需要用户手动点右侧 "Run reviews"，且合成（synthesis）发生在互评之前，互评结论无法影响最终计划——互评变成了事后装饰。

## 目标

1. 计划页重构为**单列时间线工作台**：每轮规划（PlanningRun）是时间线上的一个回合，回合内按"需求 → 起草 → 互评 → 合成 → 最终计划"分阶段展示；历史回合默认折叠；右侧摘要面板整体移除，其功能收编进时间线节点。
2. 互评纳入自动流水线：起草成功候选 ≥2 时自动两两互评，**合成 prompt 吸收互评结论**后再产出最终计划。
3. Agent 失败有分类、有重试：可重试失败自动重试 1 次；时间线上失败节点提供手动 Retry；不可重试失败给出明确修复提示（如 Amp credits）。
4. 最终计划与候选计划支持**应用内独立窗口阅读**（Tauri WebviewWindow + 现有安全渲染管线）与外部浏览器打开两条路径。

## 非目标

- 不改 implementing / testing 阶段。
- 不做多轮计划 diff 对比视图（历史回合可展开查看即可）。
- 不做互评结论的逐条采纳/驳回交互（互评结论作为合成输入与时间线展示，v2 不做 accept 工作流）。
- 不引入前端 Markdown 渲染依赖，HTML 渲染继续唯一走 Rust `plan_html.rs`。
- 不做 Agent 配置自动修复（如自动禁用无 credits 的 Amp），只给提示。

## 成功标准

- 选 2+ Agent 发起规划，时间线依次出现：需求节点 → 各 Agent 起草卡（实时状态/耗时）→ 互评节点（reviewer→target 结论 + severity 徽章）→ 合成节点（注明来源）→ 最终计划节点；全程无需点击 "Run reviews"。
- 人为造一个超时失败：该 Agent 自动重试 1 次（事件流可见 `retrying`）；重试仍失败后卡片出现 Retry 按钮，点击后仅重跑该 Agent 并重新互评+合成。
- Amp 无 credits 场景：卡片标注"不可重试：需要付费 credits"且不自动重试，错误详情默认折叠为一行。
- 最终计划节点上 "Open in Loom" 打开应用内独立窗口完整阅读 HTML，"Open in browser" 打开系统浏览器，两者内容一致。
- 右侧 aside 在代码与 CSS 中删除；记录决策、Create tasks 在最终计划节点上可用且行为不变。
- 多轮讨论后旧回合折叠为单行摘要，页面不再随轮次线性变长。
- `cargo test`、`cargo clippy`、`pnpm test`、`pnpm build` 通过；真实 CLI 手测一轮闭环。

## 当前状态（代码盘点）

| 模块 | 现状 | 本次处置 |
|------|------|----------|
| `agents.rs:195` `run_planning_discussion` | 并行起草 → 合成（互评前）→ 写 MD/HTML | 重构为 起草 → 自动互评 → 合成（吸收互评）流水线 |
| `agents.rs:404` `run_plan_reviews` | 手动命令，互评后只把结论附进 MD | 抽出内部函数 `run_cross_reviews` 供流水线复用；命令保留作"重新互评"入口 |
| `agents.rs:503` `run_planning_agent_with_status` | 失败即终态，无分类 | 包一层重试循环 + 失败分类 |
| `agents.rs:1028` `render_synthesis_prompt` | 只输入候选计划 | 增加互评结论段 |
| `models.rs:227` `AgentInvocation` | 无 attempt / 失败分类 | 增 `attempt`、`failure_kind`（serde default） |
| `models.rs:252` `PlanningAgentStatusEvent` | phase: planning/synthesis | 增 `review` phase 与 `retrying` status |
| `plan_html.rs:40/62` `read_plan_html` / `open_plan_html` | iframe 预览 + 系统浏览器 | 复用白名单，新增应用内窗口命令 |
| `capabilities/default.json` | 仅 `windows: ["main"]` | 增 viewer 窗口 label 匹配 |
| `PlanningChat.tsx`（712 行） | 平铺聊天流 + 右侧 4 卡 aside | 主区重写为时间线；aside 删除 |
| `PlanningProgress.tsx` / `CandidatePlans.tsx` | 独立进度卡/候选 Tab 区块 | 吸收进时间线回合的起草阶段节点 |
| `reducer.ts` `planningProgress` | 按 runId+phase+agentId 索引事件 | 复用，扩展 review/retrying |
| `Workspace/PlanningWizard.tsx`(1100 行) / `PlanningPane.tsx` | 不在生产路由（`WorkspaceSplit.tsx:180` 只用 PlanningChat），仅 preview 引用 | 移除或降级为 preview-only，消除双实现 |
| 可复用 | `tauri::WebviewWindowBuilder`（核心能力，零新依赖）、Vite 多入口（preview 已有先例）、`plan_html.rs` 渲染与路径白名单 | 应用内查看器零新依赖 |

## 总体设计

### 时间线信息架构（替代聊天流 + aside）

```
┌──────────────────────────────────────────────────────────┐
│ Planning · <task title>                    <plan 路径徽章> │
├──────────────────────────────────────────────────────────┤
│ ▸ Round 1 · 2026-06-10 17:32 · 3 agents · fallback  (折叠)│
│ ▾ Round 2 · 2026-06-11 10:05 · 2 agents                  │
│   ● 需求      "这个项目需要重构……"                          │
│   ● 起草      ✓ Codex 2m13s [查看]   ✗ Amp 不可重试:需要    │
│   │           ✓ Claude 1m48s [查看]    付费credits [详情▸]  │
│   ● 互评      Codex→Claude: risk "迁移顺序有依赖问题"        │
│   │           Claude→Codex: info "建议补充回滚策略"          │
│   ● 合成      由 Claude 融合 2 份候选 + 2 条互评结论          │
│   ● 最终计划   [内嵌预览(可折叠)]                            │
│               [Open in Loom] [Open in browser]            │
│               [Record decision ▸]  [Create tasks from plan]│
├──────────────────────────────────────────────────────────┤
│ <composer：@mention + agent chips + Send>（保持现状）       │
└──────────────────────────────────────────────────────────┘
```

- 回合 = `PlanningRun`；阶段节点由 `agentInvocations`（promptSummary 区分 planning/synthesis）、`planReviews`、实时 `planningProgress` 事件按 `planningRunId` 聚合推导，**不需要新的回合数据结构**。
- 失败卡片：一行分类文案 + 可展开的 stderr 详情（`<details>`），不再平铺 `<pre>`。
- 运行中回合：阶段节点随状态事件实时点亮（queued → running → retrying → succeeded/failed）。

### 自动流水线（后端）

```
并行起草(各自动重试≤1) ──候选≥2──▶ 并行两两互评 ──▶ 合成(候选+互评结论, 自动重试≤1) ──▶ 写 MD+HTML
                       └─候选=1──────────────────▶ 直采该候选 ─────────────────────────┘
                       └─候选=0──────────────────▶ deterministic fallback ─────────────┘
```

互评任何一条失败只丢弃该条结论，不阻塞合成。

### 失败分类与重试

| failure_kind | 判定 | 自动重试 |
|--------------|------|----------|
| `timeout` | `timed_out == true` | 是（1 次） |
| `empty_output` | 退出 0 但 stdout 空 | 是（1 次） |
| `nonzero_exit` | 非零退出且不匹配下行模式 | 是（1 次） |
| `not_retryable` | stderr 匹配已知配置错误模式：`paid credits`/`command not found`/`not logged in`/`authentication` 等 | 否，展示修复提示 |

重试的 evidence 文件带 attempt 后缀（`<agent>.attempt-2.stdout.md`），invocation 记录 `attempt` 字段；手动 Retry 复用同一执行函数，成功后重跑互评与合成并刷新 MD/HTML。

### 应用内计划查看器

- 新增前端入口 `plan-viewer.html`（Vite 多入口）：极简页面，读取 query 中的 `projectPath` + `mdPath`，调既有 `read_plan_html`（白名单/转义全部复用）后整页渲染。
- 新增命令 `open_plan_viewer(app, project_path, md_path, title)`：先用与 `read_plan_html` 相同的白名单校验路径，再 `WebviewWindowBuilder::new(..., WebviewUrl::App("plan-viewer.html?...".into()))` 开新窗口；同一 md 路径复用已开窗口（label 用路径 hash）。
- `capabilities/default.json` 的 `windows` 增加 `"plan-viewer-*"`。
- 外部浏览器路径 `open_plan_html` 保持不变；候选计划与最终计划共用这两个入口。

## 里程碑

依赖：M1 → M2；M3 独立；M4 依赖 M1/M2（数据与事件）+ M3（查看器入口）；M5 收尾。每个里程碑独立可交付、可回滚。

### M1 — 互评纳入自动流水线

**Outcome**: 一次 `run_planning_discussion` 自动完成 起草 → 互评 → 合成，互评结论影响最终计划。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 1.1 | 从 `run_plan_reviews` 抽出内部函数 `run_cross_reviews(...) -> Vec<PlanReview>`：两两互评并行执行（`tauri::async_runtime::spawn`，同起草模式），单条失败转 failed review 不冒泡 | `agents.rs:404/628` | — | 单测：dummy 多 agent 互评条数 = n*(n-1)；一条失败不影响其余 |
| 1.2 | `run_planning_discussion` 流水线重排：候选 ≥2 时先 `run_cross_reviews` 再合成；互评结果写入 `task.plan_reviews`（按 run 替换） | `agents.rs:195` | 1.1 | 单测：自动流水线后 plan_reviews 非空且 planning_run_id 正确 |
| 1.3 | `render_synthesis_prompt` 增"互评结论"段（reviewer/target/severity/finding），无结论时省略该段 | `agents.rs:1028` | 1.2 | 单测：prompt 含/不含互评段两分支 |
| 1.4 | 互评阶段发状态事件：`phase: "review"`（每条互评 running/终态各一次，agent 字段用 reviewer） | `agents.rs:503`、`models.rs:252` | 1.1 | 单测/手测事件时序 |
| 1.5 | `run_plan_reviews` 命令改为薄封装（重新互评 + 重写 MD/HTML + 重新合成不做，仅更新结论与 HTML），语义改为 "Re-run reviews" | `agents.rs:404` | 1.1 | 既有单测更新 |

### M2 — 失败分类与重试

**Outcome**: 可重试失败自动重试一次；不可重试失败有明确提示；任意失败的起草 Agent 可单独手动重跑并重新进入互评+合成。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 2.1 | `AgentInvocation` 增 `attempt: u32`（default 1）、`failure_kind: Option<String>`；前端 domain 同步 | `models.rs:227`、`src/domain/task.ts`、`useTaskBridge.ts` | — | 旧任务 JSON 反序列化单测 |
| 2.2 | 失败分类函数 `classify_failure(timed_out, exit_code, stdout, stderr) -> FailureKind`，含 not_retryable stderr 模式表（paid credits / command not found / not logged in / authentication / ENOENT） | `agents.rs` 新函数 | — | 单测覆盖 4 类样本（含截图中 Amp credits 原文） |
| 2.3 | `run_planning_agent_with_status` 包重试循环：可重试失败自动重试 1 次，重试前发 `status: "retrying"` 事件；evidence 文件加 `.attempt-N` 后缀；合成阶段同样适用 | `agents.rs:503/705/792` | 2.1, 2.2 | dummy 单测：超时→2 attempts；not_retryable→1 attempt |
| 2.4 | 新命令 `retry_planning_agent(project_path, task_id, agent_id)`：仅允许针对最新 run 的失败起草 invocation；重跑该 Agent（attempt+1 新 invocation，旧记录保留）→ 重新互评 → 重新合成 → 重写 MD/HTML/PLANS.md 摘要；注册到 `lib.rs` 与 `useAgentBridge` | `agents.rs` 新命令、`lib.rs:46`、`useAgentBridge.ts` | 2.3, M1 | 单测：重试成功后最终计划来源更新；手测 Retry 按钮 |
| 2.5 | reducer `planningProgress` 支持 `retrying` 状态与重试后状态覆盖 | `src/state/reducer.ts`、`tests/unit/reducer.test.cjs` | 2.3 | 单测 |

### M3 — 应用内计划查看器窗口

**Outcome**: 任何候选/最终计划既能在 Loom 内独立窗口完整阅读，也能在系统浏览器打开。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 3.1 | 新增 `plan-viewer.html` + `src/preview/PlanViewerApp.tsx`（或 `src/viewer/`）：解析 query → `read_plan_html` → 整页注入渲染结果（沿用 iframe sandbox 或直接受控 innerHTML，渲染器已转义）；配置 Vite 多入口 | `vite.config.ts`、新文件 | — | `pnpm build` 产出 viewer 入口 |
| 3.2 | 新命令 `open_plan_viewer(app, project_path, md_path, title)`：路径白名单校验（复用 `read_plan_html` 校验函数，先抽公共函数）→ 按路径 hash 生成 label `plan-viewer-<hash>`，已存在则 focus，否则 `WebviewWindowBuilder` 新窗口；注册 `lib.rs` | `plan_html.rs`、`lib.rs` | 3.1 | 单测：白名单外路径报错；手测开窗/复用 |
| 3.3 | `capabilities/default.json` `windows` 增 `"plan-viewer-*"`，确认 viewer 窗口能调用 `read_plan_html` | `src-tauri/capabilities/default.json` | 3.2 | 手测 viewer 窗口正常渲染 |
| 3.4 | `useTaskBridge` 暴露 `openPlanViewer`；dev（vite devUrl）与 build（dist）两种模式下 viewer URL 均正确 | `useTaskBridge.ts` | 3.2 | 手测 dev + `pnpm tauri build` 或 dev 双模式确认 |

### M4 — 时间线 UI 重构

**Outcome**: 计划页是单列时间线工作台；右侧 aside 删除；失败信息降噪；所有操作收编到对应节点。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 4.1 | 新建 `PlanningTimeline.tsx`：按 `planningRuns` 倒序分组渲染回合，最新回合展开、历史回合折叠为单行摘要（时间/agent 数/最终计划来源）；回合内聚合 invocations + planReviews + planningProgress 推导阶段节点 | 新文件 `src/components/Planning/PlanningTimeline.tsx` | M1, M2 | 组件级手测 + reducer 选择器单测 |
| 4.2 | 起草阶段节点：每 Agent 一行（状态图标/耗时/attempt 标记），成功→ [查看]（in-app viewer + 浏览器菜单），失败→ 分类文案 + Retry 按钮（not_retryable 时换为修复提示）+ `<details>` 折叠 stderr；吸收并删除 `PlanningProgress.tsx`、`CandidatePlans.tsx` | 同上、删除两旧文件 | 4.1, M3 | 手测三态 |
| 4.3 | 互评/合成/最终计划节点：互评条目带 severity 徽章；合成节点显示来源（合成/直采/fallback + 重新互评入口）；最终计划节点含可折叠 iframe 预览、Open in Loom / Open in browser、Record decision 内联折叠表单、Create tasks from plan 主 CTA | 同上 | 4.1, M3 | 手测闭环到看板 |
| 4.4 | `PlanningChat.tsx` 瘦身：删除 `planning-summary` aside、summaryCollapsed/ResizeObserver/headerSlot 折叠逻辑、平铺 invocation 聊天流，主区 = header + `PlanningTimeline` + composer（composer 与 @mention 逻辑不动） | `PlanningChat.tsx`、`Planning.css` | 4.1-4.3 | `rg "planning-summary"` 无残留；截图对比 |
| 4.5 | `Planning.css` 时间线视觉：轨道线/节点圆点/阶段间距/折叠动效，删除 aside 与旧进度卡样式 | `Planning.css` | 4.4 | 截图 |

### M5 — 清理与端到端验证

**Outcome**: 无双实现残留，全链路真实验证通过。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 5.1 | 处置 `PlanningWizard.tsx`(1100 行)/`PlanningPane.tsx`：确认仅 preview 引用后从生产构建移除（保留 preview 或一并删除，取决于 preview 是否还需要） | `Workspace/PlanningWizard.tsx`、`Workspace/PlanningPane.tsx`、`src/preview/*` | M4 | `pnpm build` 通过且产物不含 wizard |
| 5.2 | `PlanningPreviewApp.tsx` 适配时间线新结构（mock 数据补 reviews/attempt/failure_kind） | `src/preview/PlanningPreviewApp.tsx` | M4 | preview 页可视验证 |
| 5.3 | 全量检查：`cargo test` / `cargo clippy` / `pnpm test` / `pnpm build` | — | 全部 | CI 级通过 |
| 5.4 | 真实 CLI 端到端：`scripts/start-local.sh` 启动 → 选 Codex+Claude 发起规划 → 观察时间线各阶段实时点亮 → 互评自动出现 → 最终计划 in-app/浏览器双开 → 人为断网或杀进程验证 Retry → Create tasks 进看板 → `scripts/preview.sh stop` | — | 5.3 | 手测记录 |

## 风险

| 风险 | 可能性 | 影响 | 缓解 |
|------|--------|------|------|
| 自动互评 + 重试显著拉长单轮时长（最坏 2×起草 + 互评 + 2×合成） | 高 | 中 | 互评/起草全并行；重试上限 1；not_retryable 直接短路；时间线实时展示让等待可感知 |
| not_retryable stderr 模式表误判（把配置错误当可重试，反复烧时间） | 中 | 中 | 模式表保守（只收录确证样本）；误判最多多跑 1 次；分类记录进 invocation 便于事后修表 |
| `WebviewUrl::App` 在 dev/build 下 URL 形态不同导致 viewer 白屏 | 中 | 中 | 3.4 双模式显式验证；降级方案：viewer 改为主窗口内全屏模态（纯前端，无窗口） |
| viewer 窗口 capability 配置遗漏导致 invoke 被拒 | 中 | 低 | 3.3 显式手测；`plan-viewer-*` 通配 label |
| 手动 Retry 与正在运行的讨论并发冲突 | 中 | 中 | 命令入口校验：最新 run 存在运行中 invocation 时拒绝 Retry；前端运行中隐藏按钮 |
| 时间线一次性重写导致回归（composer/mention/创建任务） | 中 | 高 | composer 逻辑原样保留；M4 拆 4.1-4.5 小步提交；preview 页先行可视验证 |
| 旧任务 JSON 缺新字段 | 中 | 高 | `attempt`/`failure_kind` 一律 `#[serde(default)]`，沿用既有模式 |
| 互评结论注入合成 prompt 超长（候选全文 + 结论） | 低 | 低 | 结论只注入 summary 字段（finding），不注入互评 raw_output |

## 待确认问题

- [ ] 自动互评是否需要 Settings 开关（默认开）？v2 先不加开关，若实测时长不可接受再加。
- [ ] `PlanningWizard.tsx` / `PlanningPane.tsx` 是否可以连同 preview 一起删除？（5.1 实施时确认 preview 的实际用途）
- [ ] 历史回合折叠摘要里是否需要展示该轮最终计划路径（每轮都会写一个新 `docs/plans/` 文件，多轮后索引会变长——是否改为同一任务复用同一计划文件、按轮覆盖？）**倾向：同一任务复用首轮路径，后续轮覆盖更新**，避免 PLANS.md 被同一任务刷屏；实施 M1 时一并确认。
- [ ] 手动 Retry 成功后是否需要把旧失败 invocation 从时间线隐藏（保留数据仅展示最新 attempt）？倾向：起草行只展示最新 attempt，旧 attempt 收进详情。

## 验证策略

- **每里程碑**：对应单测（`cargo test`、`tests/unit/reducer.test.cjs`）+ `cargo clippy` + `pnpm build`；M2 必须含 not_retryable 分类用例（用截图中 Amp credits 真实报错文本）；M3 必须含路径白名单负例。
- **端到端（M5）**：见 5.4 真实 CLI 手测脚本；验证后 `scripts/preview.sh stop` 清理。
- **回滚**：M1/M2 纯后端且字段全 `serde(default)`，可独立 revert 不破坏已存任务；M3 是新增命令与入口，删除即回滚；M4 前保留旧组件文件直至 4.4 完成，单独 commit 便于整体 revert。
