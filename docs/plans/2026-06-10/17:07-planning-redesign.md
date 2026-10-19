# 计划功能重设计：多 Agent 候选计划 + 可视化 HTML — Plan

- **日期**: 2026-06-10
- **作者**: Claude (Fable 5)
- **状态**: ready-for-implementation（已按 Review 意见修订；2026-06-10 二次 Review 吸收 2 条 minor：并行执行改零依赖方案、capabilities 文件降级为确认项）

## 背景与问题

当前计划页（Planning Room）可用性差，根因在后端产出方式，不只是 UI：

1. **最终计划是 Rust 字符串模板拼接的**（`src-tauri/src/agents.rs:987` `render_final_plan`）：把各 Agent 的原始 stdout 串接进固定章节，todo 靠启发式提取（`agents.rs:1117`），提取不到时填通用占位文案——截图中"Review successful Agent output / Persist planning runs…"那种模板感计划就是这么来的。它不是基于项目代码的真实实施计划。
2. **Agent 串行执行、无进度反馈**（`agents.rs:205` 循环，单 Agent 预算 240s），UI 只有一个"Running…"占位，多 Agent 时用户要盲等几分钟。
3. **计划只有 Markdown 一种形态**，前端用 `<pre>` 裸渲染（`src/components/Planning/PlanningChat.tsx:466`），内容一长可读性急剧下降，无法支撑 Review。

## 目标

1. 每个本地 Agent（Codex、Claude Code、Amp）基于**项目现有代码**独立产出一份结构完整、可独立执行的候选实施计划（Markdown）。
2. 多份候选计划由"合成 Agent"融合为最终计划，写入 `docs/plans/YYYY-MM-DD/HH:mm-<topic>.md` 并同步 `docs/PLANS.md`（沿用现有约定）。
3. 每份最终计划同时生成一份**自包含的可视化 HTML 文档**（目录导航、章节折叠、任务表格、风险徽章），与 MD 同目录同名，便于浏览器中 Review。
4. 计划页 UX 重构：候选计划分 Tab 展示、Agent 级实时进度、最终计划渲染预览替代 `<pre>`。

## 非目标

- 不做实施（implementing）阶段的改动；只覆盖 planning / plan_review 闭环。
- 不做计划版本 diff、多轮计划历史对比 UI（保留 evidence 文件即可）。
- 不让 Agent 直接生成 HTML（慢、不稳定、不可控；HTML 由 Loom 确定性渲染）。
- 不引入重量级前端 Markdown 渲染栈；渲染统一走 Rust 单一路径。
- 不改互评（plan review）的两两评审机制本身，只让其结果进入 HTML。

## 成功标准

- 选 2+ 个真实 Agent 发起规划，每个 Agent 的候选计划包含具体文件路径引用（非泛泛而谈），并以独立 MD 落盘可查。
- 最终计划由合成 Agent 产出（或单 Agent 时直接采用其候选），不再出现"Review successful Agent output…"通用占位 todo。
- `docs/plans/.../HH:mm-<slug>.md` 旁生成同名 `.html`，双击可在浏览器离线打开，含 TOC、可折叠章节、任务表、风险标注；计划被互评/人工决策更新后 HTML 同步刷新。
- 计划页能看到每个 Agent 的 queued/running/succeeded/failed 状态与耗时；多 Agent 并行执行，总耗时≈最慢者而非求和。
- `cargo test`、`pnpm test`、`cargo clippy`、`pnpm build` 通过；真实 CLI 手测一轮闭环（规划 → 互评 → 确认生成 todos）。

## 当前状态（代码盘点）

| 模块 | 现状 | 本次处置 |
|------|------|----------|
| `agents.rs:182` `run_planning_discussion` | 串行调 Agent → 字符串拼最终计划 → 写 MD + PLANS.md | 重构：并行调用 + 合成步骤 + HTML 生成 |
| `agents.rs:716` `render_planning_prompt` | 只要求 5 个粗章节（Goal/Approach/Files/Risks/Verification） | 升级为完整计划模板，强制引用具体文件 |
| `agents.rs:756` `build_cli_profile` / `default_profile_args` | codex `exec --sandbox read-only`、claude `-p --output-format text`、amp `-x` | 保留（claude 严禁 `--permission-mode plan`，见风险） |
| `agents.rs:987` `render_final_plan` | 拼接 + 启发式 todo | 降级为合成失败时的 fallback |
| `agents.rs:1195` `next_project_plan_path` / `update_project_plans_index` | 计划路径与 PLANS.md 索引维护 | 复用 |
| `.loom/planning/<task>/<run>/` evidence 目录 | prompt/stdout/stderr 落盘 | 候选计划即 `<agent>.stdout.md`，升格为一等公民 |
| `models.rs:225` `AgentInvocation` | 已有 raw_output / evidence_ref | 新增 `plan_path` 字段（candidate MD 路径） |
| `tasks.rs:142` `confirm_plan` / `tasks.rs:627` `derive_plan_todos` | 从最终计划提取 todo，空则报错 | 复用；模板化输出后提取更稳 |
| `PlanningChat.tsx` | 单文件：对话流 + `<pre>` 计划 + 右侧摘要 | 拆组件：进度卡、候选 Tab、HTML 预览 |
| 可复用 | tauri-plugin-opener（lib.rs:42 已注册）、Tauri event 机制（command_runner 已用） | 打开 HTML / 进度事件直接复用 |
| 需新增 | Rust `pulldown-cmark` 依赖（确定性 MD→HTML）；`plan_html.rs` 模块。M4 并行执行**不加新依赖**：复用 `tauri::async_runtime::spawn`（agents.rs:609 已在用），spawn 全部 agent future 后依次 await JoinHandle；仅当该方案受阻才退回 `futures::join_all` | 均有明确 MVP 理由 |

## 里程碑

依赖顺序：M1 → M2 → M3 独立可并行于 M4 → M5 依赖 M3/M4。每个里程碑独立可交付、可回滚。

### M1 — 结构化候选计划

**Outcome**: 每个 Agent 产出按统一模板组织、引用具体代码路径的候选计划 MD，并作为独立产物落盘与建模。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 1.1 | 重写规划 prompt：完整模板（目标/非目标/现状盘点/技术方案/文件影响清单/里程碑任务表/风险/验证策略/Implementation Todo），明确要求"先阅读项目代码，引用真实文件路径，禁止泛泛而谈" | `agents.rs:716` `render_planning_prompt` | — | 单测断言 prompt 含各章节标记 |
| 1.2 | 候选计划建模：`AgentInvocation` 增 `plan_path: Option<String>`（serde default 兼容旧数据），`run_planning_agent` 成功时填 stdout MD 路径 | `models.rs:225`、`agents.rs:496` | 1.1 | 单测：成功 invocation 带 plan_path |
| 1.3 | 候选文件命名升级：`<agent>.stdout.md` → 同时写 `<agent>.plan.md`（含 frontmatter：agent、时间、需求摘要），stdout 原样保留为证据 | `agents.rs:496` `run_planning_agent` | 1.2 | 文件存在性 + frontmatter 单测 |
| 1.4 | 前端 domain/类型同步 `planPath` | `src/domain`、`useTaskBridge.ts` | 1.2 | `pnpm build` 类型通过 |

### M2 — 最终计划合成

**Outcome**: 最终计划是真实计划而非拼接物；多 Agent 时由合成 Agent 融合候选，单 Agent 时直接采用其候选；拼接逻辑降级为 fallback。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 2.1 | 新增 `render_synthesis_prompt`：输入需求 + 全部候选计划全文，要求输出融合后的单一最终计划（同 M1 模板，含 Implementation Todo）；注意合成发生在 `run_plan_reviews` 之前，不依赖互评结果 | `agents.rs` 新函数 | M1 | 单测断言 prompt 不要求 review 输入 |
| 2.2 | `run_planning_discussion` 增合成步骤：成功候选 ≥2 → 调合成 Agent（默认取候选中第一个成功的 claude，否则第一个成功者；后续可配置）；=1 → 直接采用该候选；=0 或合成失败 → fallback `render_final_plan` | `agents.rs:182` | 2.1 | 单测覆盖合成 / 直采 / fallback 3 分支（dummy adapter） |
| 2.3 | 合成调用本身作为一条 `AgentInvocation` 记录（`prompt_summary: "Synthesize final plan"`），证据落盘 `synthesis.prompt.md` / `synthesis.stdout.md`；直采/fallback 时在 `discussion_summary` 写明最终计划来源 | `agents.rs` | 2.2 | evidence 文件 + source summary 断言 |
| 2.4 | `derive_plan_title` 改为优先从最终计划（而非首个候选）取题 | `agents.rs:916` | 2.2 | 既有单测更新 |

### M3 — 可视化 HTML 生成

**Outcome**: 每份最终计划旁有一份自包含 HTML；计划任何更新（互评、人工决策）都会同步重渲染。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 3.1 | 新增 `pulldown-cmark` 依赖；新建 `plan_html.rs`：`render_plan_html(markdown, meta) -> String`，产出内联 CSS/JS 的单文件 HTML——固定 TOC 侧栏、`<details>` 章节折叠、表格样式、风险/严重度徽章、元信息头（任务、日期、参与 Agent、互评统计）、print 样式；渲染器必须把 Agent Markdown 当作不可信输入，转义 raw HTML，过滤 `javascript:` / `data:` 等危险链接 | `src-tauri/Cargo.toml`、新文件 `src-tauri/src/plan_html.rs` | — | 单测：TOC 锚点、HTML 转义、raw `<script>` 被转义、`<img onerror>` 不执行、危险链接被移除、章节折叠结构 |
| 3.2 | 写最终计划的所有路径同步写 `.html`：`run_planning_discussion`、`run_plan_reviews`（agents.rs:393 重写处）、`record_planning_decision` | `agents.rs`、`tasks.rs` | 3.1, M2 | 单测：MD 更新后 HTML mtime/内容同步 |
| 3.3 | Task 增 `final_plan_html_path: Option<String>`（serde default）；前端 `Task` 类型增 `finalPlanHtmlPath`；新增 command `open_plan_html(project_path, html_path)`（tauri-plugin-opener 打开系统浏览器）与 `read_plan_html(project_path, md_path)`（返回安全 HTML 字符串供 iframe 预览）；注册到 `lib.rs`，并在 `useTaskBridge.ts` 暴露调用。`capabilities/default.json` 预期无需改动（capabilities 只管插件权限，`generate_handler` 注册的自定义 command 不走 capability，Rust 侧调 opener 也不需要前端权限，`opener:default` 已存在），实施时确认即可 | `models.rs:147`、`src/domain/task.ts`、`useTaskBridge.ts`、`agents.rs` 或 `tasks.rs`、`lib.rs:46` | 3.2 | 手测：浏览器打开 + 应用内预览一致；`pnpm build` 类型通过 |
| 3.4 | 候选计划 Tab 预览也走同一渲染：`read_plan_html(project_path, md_path)` 只接受当前项目 `.loom/planning/` 与 `docs/plans/` 下的 `.md` 路径（canonicalize 白名单校验，拒绝目录穿越、软链接逃逸和非 `.md` 后缀） | 同 3.3 | 3.3 | 单测：白名单外路径、软链接逃逸、非 `.md` 路径均报错 |

### M4 — 并行执行与进度事件

**Outcome**: 多 Agent 并行规划，前端实时看到每个 Agent 的状态与耗时。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 4.1 | `run_planning_discussion` 中 Agent 循环改并行：用 `tauri::async_runtime::spawn`（agents.rs:609 已在用，零新依赖）spawn 全部 agent future，再按入参顺序依次 await JoinHandle（spawn 即开始并发执行；evidence 文件按 agent.id 命名，天然无冲突；invocation 顺序按入参排序保持稳定）；每个 future 内部把启动/执行错误转成 failed invocation（当前 agents.rs:209 的 `await?` 会让单 Agent 失败中止整轮，需移除），不让单个 Agent 失败取消其它候选。若 spawn 方案受阻（如 `'static` 约束改造成本过高）才退回新增 `futures` 依赖用 `join_all` | `agents.rs:205`（备选：`src-tauri/Cargo.toml`） | M1 | `cargo build` 通过；dummy 多 Agent 单测：结果顺序稳定；一个失败 Agent 不影响其它成功候选 |
| 4.2 | 新增 Tauri 事件 `loom://planning-agent-status`：`{taskId, planningRunId, agentId, agentName, phase, status, startedAtMs, endedAtMs, elapsedMs}`，每个 Agent 开始/结束各发一次；合成步骤同样发事件 | `agents.rs`（参考 `command_runner.rs` 的 emit 模式）、`models.rs` 新 `PlanningAgentStatusEvent` | 4.1 | 手测事件时序 |
| 4.3 | 前端监听：`useAgentBridge` 订阅事件，reducer 增 `planningProgress` 状态（按 `planningRunId + agentId/phase` 索引）；提交时先为选中 Agent 初始化 queued 状态，收到新 task 后清理旧 run 状态 | `src/hooks/useAgentBridge.ts`、`src/state/reducer.ts` | 4.2 | `tests/unit/reducer.test.cjs` 增用例 |

### M5 — 计划页 UX 重构

**Outcome**: 计划页从"日志流"升级为"计划工作台"：进度卡、候选 Tab、渲染预览、HTML 一键打开。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 5.1 | 拆出 `PlanningProgress.tsx`：每个选中 Agent 一张状态卡（queued/running/succeeded/failed + 耗时），替代单一"Running…"占位 | 新文件 `src/components/Planning/PlanningProgress.tsx` | M4 | 手测 |
| 5.2 | 拆出 `CandidatePlans.tsx`：只展示最新 planning run 的成功候选，按 Agent 分 Tab，内容用 sandboxed `<iframe srcdoc>` 渲染 `read_plan_html` 结果；失败 Agent 显示 stderr 摘要 | 新文件、`PlanningChat.tsx` | M3 | 手测 |
| 5.3 | 最终计划区：`<pre>` 替换为同款 iframe 预览 + "Open HTML" 按钮（调 `open_plan_html`）+ MD 路径展示 | `PlanningChat.tsx:459` | M3 | 手测 |
| 5.4 | 右侧摘要卡顺序与文案微调：Consensus 显示合成来源（哪个 Agent 合成/直采/fallback，来源从 M2 的 `discussion_summary` 与 synthesis invocation 推导）；样式更新 | `PlanningChat.tsx`、`Planning.css` | 5.1-5.3 | 手测 + 截图 |
| 5.5 | 若保留 `PlanningWizard.tsx` / `PlanningPane.tsx` 作为预览或历史入口，同步替换其最终计划 `<pre>` 或明确从生产路由移除，避免双入口 UI 行为不一致 | `Workspace/PlanningWizard.tsx`、`Workspace/PlanningPane.tsx` | 5.3 | `rg "<pre.*final-plan"` 无生产入口残留 |

## 风险

| 风险 | 可能性 | 影响 | 缓解 |
|------|--------|------|------|
| Agent 不遵守计划模板，章节缺失 | 中 | 中 | HTML 渲染不依赖模板（任意 MD 都能渲染）；todo 提取保留现有多标题别名启发式；fallback 拼接兜底 |
| Claude 用 `--permission-mode plan` 导致 stdout 近空 | 低 | 高 | 已有教训（见 `agents.rs:794` 注释与项目记忆）：保持 `claude -p --output-format text`，不动 profile |
| 合成步骤使规划总时长 +1 个 Agent 调用（~1-2min） | 高 | 中 | 并行化（M4）抵消大部分；单候选直采不合成；合成有独立进度事件，用户可感知 |
| 并行调用导致本机资源争抢 / API 限流 | 低 | 低 | 当前最多 3 个 Agent；保留每 Agent 240s 超时；如出问题再加并发上限（YAGNI） |
| `pulldown-cmark` 新依赖 | — | 低 | 纯 Rust、无重传递依赖，确定性 MD→HTML 是明确 MVP 理由 |
| 并行方案 `tauri::async_runtime::spawn` 受 `'static` 约束阻碍 | 低 | 低 | future 入参 clone 成 owned（agent、路径、prompt 均可 clone）；仍受阻则退回新增 `futures` 依赖用 `join_all`，并行执行是明确 MVP 理由 |
| `read_plan_html` 暴露任意文件读取 | 低 | 中 | 路径白名单（项目 `docs/plans/` 与 `.loom/planning/` 下、`.md` 后缀、canonicalize 防穿越）；iframe 加 `sandbox` 属性 |
| Agent Markdown 注入 HTML/JS | 中 | 高 | `plan_html.rs` 禁用 raw HTML、过滤危险链接；系统浏览器打开前也只使用安全渲染结果；补 XSS 单测 |
| 单个 Agent 启动失败取消整个并行规划 | 中 | 中 | future 内部捕获错误并转换为 failed invocation；最终计划仍基于成功候选合成或 fallback |
| 旧任务 JSON 缺新字段反序列化失败 | 中 | 高 | 新字段一律 `#[serde(default)]`，与现有 `final_plan_path` 同模式 |

## 已定策略

- 合成 Agent 选择策略：默认"候选中优先 claude，否则第一个成功者"；Settings 配置留后续。
- 候选计划不进入 `docs/plans/`，只留在 `.loom/planning/` evidence 目录；`docs/plans/` 只放最终计划，保持索引干净。
- 最终 HTML v1 嵌入候选计划对比折叠区；若 HTML 过大再裁剪。
- Amp 默认 `enabled=false`（付费 credits）保持现状。
- 本计划唯一新增依赖为 `pulldown-cmark`（安全、确定性的 Rust 侧 Markdown→HTML 渲染）；并行规划复用 `tauri::async_runtime::spawn`，不新增依赖，`futures` 仅作为受阻时的备选。

## 验证策略

- **每里程碑**：对应单测（`cargo test`、`tests/unit/reducer.test.cjs`）+ `cargo clippy` + `pnpm build`；M3 必须包含 raw HTML / dangerous link 安全用例。
- **端到端（M5 后）**：`scripts/start-local.sh` 启动桌面端，真实 CLI 跑一轮：选 Codex + Claude 发起规划 → 观察并行进度卡 → 查看候选 Tab → 确认合成最终计划 → 浏览器打开 HTML → Run reviews 后确认 HTML 已更新 → Create tasks from plan 进入看板。验证后 `scripts/preview.sh stop` 清理。
- **回滚**：M2 合成失败自动 fallback 旧拼接路径；M3/M5 为增量产物与 UI，可独立 revert；新模型字段全部 `serde(default)`，回滚不破坏已存任务数据。
