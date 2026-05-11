# 真实 CLI Agent Adapter — 实施计划

- **日期**：2026-05-11
- **作者**：Codex
- **状态**：已实施；本地构建、Rust 检查和 adapter 单元测试通过
- **最近更新**：2026-05-12 修正真实接入链路：Codex 使用当前 CLI 参数、Amp 使用 execute mode、Settings 可查看和启停真实 CLI profile

## 目标

把当前确定性 planning mock 升级为真实可用的 CLI Agent adapter。第一阶段面向三个现实可用的本地 Agent 工具：**Codex CLI**、**Claude Code CLI**（实际命令名为 `claude`）与 **Amp CLI**。Loom 需要能把需求、项目摘要和约束传给这些命令行工具，捕获 stdout/stderr、退出状态、耗时和 evidence，并把结果纳入计划讨论记录。

本轮同时明确 dummy Agent 的边界：dummy 只作为测试 fixture 和无外部 CLI 环境下的开发自测工具，不作为产品能力展示。

## 非目标

- 不实现交互式 PTY、长期会话或流式 Agent 对话。
- 不实现代码实施、Review Agent、自动修复执行或 Git 提交编排。
- 不新增第三方依赖。
- 不把 dummy 输出伪装成真实 Agent 能力。
- 不接入 OpenClaw、Hermes 或其它 Agent；这些留到 Codex / Claude Code / Amp 路径稳定后再扩展。
- 不支持所有 CLI 的完整协议差异；第一版只支持 Codex CLI、Claude Code CLI 与 Amp CLI 的非交互 planning 调用。

## 成功标准

- Codex CLI、Claude Code CLI 与 Amp CLI 都有明确 adapter profile、命令探测规则、参数构造规则和失败提示。
- 至少一个真实 CLI Agent 可以从 Planning composer 被选中并执行；在同时安装 Codex CLI、Claude Code CLI 与 Amp CLI 的环境中，三者都能参与同一轮 planning discussion。
- `run_planning_discussion` 不再只调用确定性输出；真实 CLI Agent 会收到结构化 prompt，并产出真实 stdout/stderr 记录。
- 每次 Agent invocation 保存：输入摘要、原始 stdout、stderr 摘要、退出状态、开始/结束时间、evidenceRef。
- 单个 Agent 失败不阻断其它 Agent；最终计划能标记部分失败和可用输出。
- dummy Agent 在 UI 和数据层被明确标记为 mock/test，且仅用于测试或显式选择。
- 无真实 CLI 环境下，自动化测试仍可通过 dummy fixture 验证任务流。
- `pnpm build`、`cargo check --manifest-path src-tauri/Cargo.toml`、`cargo test --manifest-path src-tauri/Cargo.toml` 通过。

## 当前状态

- `src-tauri/src/agents.rs` 的 `run_planning_discussion` 已通过 `run_planning_agent` 调用统一 adapter 契约；真实 CLI 路径不再调用 deterministic 输出。
- 自动发现默认 Agent 包含 Codex CLI（`codex`）、Claude Code CLI（binary 为 `claude`）、Amp CLI（`amp`）和默认禁用的 Dummy Agent (test)。
- CLI profile 已固化：Codex 使用 `codex exec --cd {projectPath} --sandbox read-only -`，Claude Code 使用 `claude -p --permission-mode plan --output-format text`，Amp 使用 `amp -x` execute mode。
- 每次 invocation 会写入 `.loom/planning/<task-id>/<planning-run-id>/` 下的 prompt、stdout、stderr evidence，并记录 `stderrTail`、`exitCode`、`timedOut`、开始/结束时间和 `evidenceRef`。
- `src/components/Workspace/PlanningPane.tsx` 已显示真实 Agent / dummy test chip、adapter label、evidence path 和失败 stderr 摘要。
- `src/components/Settings/SettingsPage.tsx` 已显示 Codex / Claude Code / Amp / Dummy profile 的 Available/Missing 状态，并支持启停可用 Agent。
- 旧 `run_dummy_planning` bridge 和 Tauri command handler 已移除；dummy 仅保留为 adapter 内部测试 fixture。

## 实施结果（2026-05-11）

- **M0-M3**：已完成真实 CLI profile、prompt 渲染、stdin / promptFile 参数替换、stdout/stderr evidence、失败隔离、部分成功汇总和全失败状态处理。
- **M4**：已完成 Planning UI 的真实/dummy 区分、失败诊断展示、Settings profile 说明和左侧宽度一致性调整。
- **M5**：已更新 README、`docs/PLANS.md` 和 `docs/plans/2026-05-09/15:24-planning-discussion-mvp.md`。
- **验证已通过**：`cargo check --manifest-path src-tauri/Cargo.toml`、`cargo test --manifest-path src-tauri/Cargo.toml`、`pnpm build`、`bash -n scripts/debug.sh`，以及 `scripts/debug.sh` 短启动 smoke（Vite 监听 `127.0.0.1:11420`，Tauri 启动 `target/debug/loom`）。
- **已知验收缺口**：未在本轮发送真实 Codex / Claude / Amp 模型请求；当前完成的是本地 adapter 执行路径、fixture 覆盖和桌面启动 smoke 验证。

## 接入修正（2026-05-12）

- Codex CLI 0.130.0 已不支持旧 `--ask-for-approval` 参数；默认 profile 已移除该参数，避免真实 planning invocation 启动即失败。
- Amp CLI 必须使用 execute mode 才能非交互退出；默认 profile 已从空参数改为 `amp -x`，并继续通过 stdin 传入 planning prompt。
- 默认 Agent 同步逻辑会修正旧本地 `agent-dummy` 配置，把它恢复为禁用的 `Dummy Agent (test)`，避免旧 Dummy Planner 被误认为真实接入能力。
- Settings 页面现在读取真实 `list_agents` 结果，显示 CLI 可用性，并通过 `set_agent_enabled` 启停 Agent。
- Planning composer 支持 `@claude-code`、`@claude`、`@codex`、`@codex-cli`、`@amp`、`@amp-cli` 等明确别名，不再依赖模糊字符串包含。
- 后端在用户指定 Agent 时只运行指定 Agent；如果指定 Agent 不可用，不再静默回退到其它启用 Agent。
- Amp Free 环境下 `amp -x` 会返回 stderr-only 402 错误且 exit code 为 0；后端现在把 stderr-only `Error:` / `error:` 归类为 failed invocation。
- Planning discussion 成功后现在进入 `plan_review`，前端展示最终计划预览和 `Confirm Plan`；只有用户确认后才派生 todo 并进入 `ready_to_implement`。
- 新增 `confirm_plan` Tauri command，从最终计划的 `## Implementation Todo` 段落提取 todo；失败计划没有 todo 时不能确认。
- 最终计划文档主路径改为选定项目的 `docs/plans/YYYY-MM-dd-xxx.md`；`.loom/planning` 仅保存 prompt/stdout/stderr evidence。
- Planning composer 默认使用全部可用真实 planning Agent（Codex CLI、Claude Code CLI、Amp CLI）；使用 `@codex` / `@claude-code` / `@amp` 可收窄到指定 Agent。
- 真实 CLI smoke 结果：Codex CLI 返回 `LOOM_CODEX_OK`，Claude Code CLI 返回 `LOOM_CLAUDE_OK`，Amp CLI 启动成功但因当前账号缺少 paid credits 返回 402。
- 新增 Rust 单元测试覆盖 Codex 当前参数、Amp execute mode、旧 dummy 配置归一化、指定 Agent 不回退、stderr-only 错误判失败、最终计划 todo 提取。

## 夜间验证记录（2026-05-12 02:50）

- 本轮未覆盖用户未提交代码改动，只做验证与计划记录补充。
- `npm run build` 通过，确认当前 Planning review / Confirm Plan 前端改动可完成 TypeScript 与 Vite 生产构建。
- `cargo test --manifest-path src-tauri/Cargo.toml` 通过：19 个 Rust 单元测试全部通过，覆盖真实 CLI profile、失败判定、计划路径和 Confirm Plan todo 提取。
- `cargo check --manifest-path src-tauri/Cargo.toml` 通过，确认当前 Tauri/Rust 代码可编译。

## 夜间验证记录（2026-05-12 03:00）

- 当前仓库仍存在未提交实现改动，本轮未覆盖代码文件，只做只读验证与计划证据补充。
- `npm run build` 通过，确认 PlanningPane / useTaskBridge / Workspace 样式当前改动仍能完成 TypeScript 与 Vite 生产构建。
- `cargo test --manifest-path src-tauri/Cargo.toml` 通过：19 个 Rust 单元测试全部通过，继续覆盖真实 CLI profile、stderr-only 失败判定、项目计划路径和 Confirm Plan todo 提取。
- `cargo check --manifest-path src-tauri/Cargo.toml` 通过，确认当前 Tauri command / storage / tasks / agents 改动可编译。

## 关键设计

### 第一阶段支持矩阵

| Agent | 产品显示名 | 命令候选 | 本轮要求 | 备注 |
|------|------------|----------|----------|------|
| Codex CLI | Codex | `codex` | 必须支持真实 planning invocation | 参数协议先通过本机 `--help` / smoke probe 固化，不在计划中硬编码未经验证的旧参数 |
| Claude Code CLI | Claude Code | `claude` | 必须支持真实 planning invocation | CLI binary 名就是 `claude`；UI 显示 Claude Code，配置保存实际 command |
| Amp CLI | Amp | `amp` | 必须保留在本轮真实 CLI 支持范围 | 若 profile 协议不同，仍走同一个 invocation/evidence 契约 |
| dummy | Dummy Agent (test) | 内置 fixture | 仅测试 / 显式 mock | 不计入真实 Agent 能力 |

### Adapter 分类

- `adapterType: "codex_cli"`：Codex CLI profile。负责 Codex 命令发现、参数构造、prompt 交付和输出捕获。
- `adapterType: "claude_code_cli"`：Claude Code CLI profile。负责 `claude` 命令发现、参数构造、prompt 交付和输出捕获。
- `adapterType: "amp_cli"`：Amp CLI profile。负责 `amp` 命令发现、参数构造、prompt 交付和输出捕获。
- `adapterType: "cli"`：保留为后续通用 CLI profile，不作为第一阶段主要验收路径。
- `adapterType: "dummy"`：测试 fixture。输出固定、可预测，只用于测试、开发演示或用户显式选择。

### Prompt 交付策略

第一版采用 prompt 文件优先，但最终调用参数必须由 M0 的 CLI protocol probe 确认：

```text
.loom/
  planning/
    <task-id>/
      <planning-run-id>/
        <agent-id>.prompt.md
        <agent-id>.stdout.md
        <agent-id>.stderr.log
```

CLI 调用规则：

- Codex CLI、Claude Code CLI 与 Amp CLI 各自有默认 profile 参数模板。
- 如果用户自定义 `args` 中包含 `{promptFile}`，替换为 prompt 文件路径。
- 如果用户自定义 `args` 中包含 `{projectPath}`，替换为当前项目路径。
- 如果 profile 支持 stdin 且没有 `{promptFile}`，则把 prompt 内容写入 stdin。
- `cwd` 按 `workingDirectoryPolicy` 解析，第一版支持 `project_root` 和 `app_data`，默认 `project_root`。

### 失败隔离

- 每个 Agent invocation 独立执行和记录。
- Agent 非零退出时，该 invocation 记为 `failed`，保存 stderr tail 和 evidenceRef。
- 只要至少一个 Agent 成功，仍生成 final plan，并在讨论摘要中列出失败 Agent。
- 全部失败时任务保留在 `planning` 或 `plan_review`，不进入 `ready_to_implement`。

### Dummy 边界

- dummy 配置使用 `adapterType: "dummy"`，名称显示为 `Dummy Agent (test)` 或等价标签。
- UI 中 mock/test Agent chip 和真实 CLI Agent chip 视觉区分。
- README 和计划文档明确 dummy 不是任务处理能力。

## 里程碑

### M0 — Codex / Claude Code / Amp CLI 协议确认

**结果**：不靠猜测接入 CLI。先用本机命令探测确认 Codex CLI、Claude Code CLI 与 Amp CLI 的可执行命令、非交互输入方式、退出码语义和输出位置。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 0.1 | 探测 Codex CLI：命令路径、版本、`--help`、是否支持 prompt 文件或 stdin 非交互调用 | 手动记录到计划实施日志或 `README.md` 当前状态 | 无 | 本机 `codex` probe 有记录，失败也记录原因 |
| 0.2 | 探测 Claude Code CLI：使用 `claude`；确认非交互调用方式 | 手动记录到计划实施日志或 `README.md` 当前状态 | 无 | 本机 Claude Code probe 有记录，binary 明确为 `claude` |
| 0.3 | 探测 Amp CLI：命令路径、版本、`--help`、非交互调用方式 | 手动记录到计划实施日志或 `README.md` 当前状态 | 无 | 本机 `amp` probe 有记录，失败也记录原因 |
| 0.4 | 根据 probe 结果固化三个 adapter profile 的默认参数模板 | `src-tauri/src/agents.rs` 或新增 `src-tauri/src/agent_adapter.rs` | 0.1, 0.2, 0.3 | Rust 测试断言默认模板不为空且命令候选正确 |
| 0.5 | 明确不可用提示：未登录、命令不存在、非交互模式不可用、执行超时 | `src-tauri/src/agents.rs`, `src/components/Workspace/PlanningPane.tsx` | 0.1, 0.2, 0.3 | 手动模拟 missing command 时 UI 显示可读错误 |

### M1 — Adapter 契约与数据边界

**结果**：真实 CLI 和 dummy 走同一个 planning invocation 契约，dummy 不再混在真实 CLI 逻辑里。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 1.1 | 新增内部 adapter 输入输出结构：`PlanningPrompt`、`PlanningInvocationResult`、`AgentCliProfile` | `src-tauri/src/agents.rs` 或新增 `src-tauri/src/agent_adapter.rs` | M0 | `cargo check` 通过 |
| 1.2 | 抽出 prompt 渲染函数，包含需求、项目摘要、约束、期望输出章节 | `src-tauri/src/agents.rs` | 1.1 | Rust 单元测试断言 prompt 包含 requirement、projectPath、输出格式 |
| 1.3 | 将 deterministic 输出移动到 dummy adapter 分支 | `src-tauri/src/agents.rs` | 1.1 | dummy invocation 仍生成固定输出，真实 CLI 路径不调用 deterministic 函数 |
| 1.4 | 明确 `adapterType` 常量或匹配逻辑：`codex_cli`、`claude_code_cli`、`amp_cli`、`dummy`、兼容 `cli` | `src-tauri/src/models.rs`, `src-tauri/src/agents.rs` | 1.1 | 未知 adapterType 返回可读错误 |

### M2 — 真实 CLI Invocation 执行

**结果**：Loom 可以启动 Codex CLI、Claude Code CLI 和 Amp CLI，传入 prompt，并捕获 stdout/stderr/退出状态。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 2.1 | 实现 prompt/stdout/stderr evidence 路径生成 | `src-tauri/src/agents.rs`, `src-tauri/src/storage.rs` | M1 | 执行后 `.loom/planning/<task>/<run>/` 下文件存在 |
| 2.2 | 实现 Codex CLI profile 参数构造和 `{promptFile}`、`{projectPath}` 替换 | `src-tauri/src/agents.rs` 或 `src-tauri/src/agent_adapter.rs` | M0, 2.1 | 单元测试覆盖 Codex 默认模板和用户自定义 args |
| 2.3 | 实现 Claude Code CLI profile 参数构造，命令固定为 `claude` | `src-tauri/src/agents.rs` 或 `src-tauri/src/agent_adapter.rs` | M0, 2.1 | 单元测试覆盖 Claude Code binary 和参数模板 |
| 2.4 | 实现 Amp CLI profile 参数构造和 `{promptFile}`、`{projectPath}` 替换 | `src-tauri/src/agents.rs` 或 `src-tauri/src/agent_adapter.rs` | M0, 2.1 | 单元测试覆盖 Amp 默认模板和用户自定义 args |
| 2.5 | 实现非交互 CLI 执行：profile 参数优先，必要时 stdin | `src-tauri/src/agents.rs` | 2.2, 2.3, 2.4 | 用 `sh -c 'cat "$1"'` 或等价 fixture 验证 stdout 捕获 |
| 2.6 | 捕获 stderr tail、exitCode、durationMs，映射 invocation `succeeded` / `failed` | `src-tauri/src/agents.rs`, `src-tauri/src/models.rs` | 2.5 | 失败命令不会 panic，invocation 记为 failed |
| 2.7 | Agent 逐个隔离执行，Codex / Claude Code / Amp 任一失败不影响其它 Agent | `src-tauri/src/agents.rs` | 2.6 | 单元测试或集成式测试覆盖一成功一失败 |

### M3 — 计划汇总与任务状态修正

**结果**：最终计划反映真实 Agent 输出和失败情况，任务状态只在有可用计划时推进。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 3.1 | 汇总函数区分成功 Agent notes 和失败 Agent diagnostics | `src-tauri/src/agents.rs` | M2 | final plan 包含成功输出摘要和失败列表 |
| 3.2 | 全部失败时不生成 ready todo，不进入 `ready_to_implement` | `src-tauri/src/agents.rs` | 3.1 | 全失败测试断言 task.status 不是 ready_to_implement |
| 3.3 | 至少一个成功时生成 final plan 和 todo，失败 Agent 作为风险项记录 | `src-tauri/src/agents.rs` | 3.1 | 部分失败测试断言 final plan 存在且包含 diagnostics |
| 3.4 | 将旧 `run_dummy_planning` 标记为兼容路径或移除前端引用 | `src-tauri/src/agents.rs`, `src/hooks/useAgentBridge.ts` | 3.1 | `rg run_dummy_planning src` 只剩兼容命令或完全移除 |

### M4 — UI 中区分真实 Agent 与 Dummy

**结果**：用户能清楚知道 Codex / Claude Code / Amp 是真实 Agent，dummy 只是测试 fixture。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 4.1 | Agent chip 显示 Codex、Claude Code、Amp、Dummy Test 四类身份 | `src/components/Workspace/PlanningPane.tsx`, `src/components/Workspace/Workspace.css` | M1 | UI 中 dummy 不会被误认为真实 Agent |
| 4.2 | Discuss 禁用态文案区分“未检测到 Codex / Claude Code / Amp”和“仅 dummy 可用” | `src/components/Workspace/PlanningPane.tsx` | 4.1 | 无 CLI 环境下显示可读提示 |
| 4.3 | Settings 或 Agent 创建入口支持 Codex / Claude Code / Amp profile 与 `args` 占位符说明 | `src/components/Settings/SettingsPage.tsx` 或后续 Agent 管理组件 | M2 | 用户能配置 `{promptFile}`、`{projectPath}` |
| 4.4 | 讨论流展示 raw output 摘要、失败状态和 evidence path | `src/components/Workspace/PlanningPane.tsx` | M3 | 成功/失败 Agent 消息都可见 |

### M5 — 测试、文档与验收

**结果**：真实 CLI adapter 有自动化覆盖，文档不再把 dummy 当产品能力。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 5.1 | 添加 Rust 单元测试覆盖 prompt 渲染、Codex profile、Claude Code profile、Amp profile、CLI 成功/失败、部分失败汇总 | `src-tauri/src/agents.rs` 或 `src-tauri/src/agent_adapter.rs` | M2, M3 | `cargo test --manifest-path src-tauri/Cargo.toml` 通过 |
| 5.2 | 更新 README 当前状态和 dummy 边界说明 | `README.md` | M4 | README 与真实能力一致 |
| 5.3 | 更新 `docs/plans/2026-05-09/15:24-planning-discussion-mvp.md` 实施状态 | `docs/plans/2026-05-09/15:24-planning-discussion-mvp.md`, `docs/PLANS.md` | M4 | 计划索引摘要不误导 |
| 5.4 | 手动验收：当前 Loom 项目 + Codex CLI + Claude Code CLI + Amp CLI；若某个未安装，记录缺失并用已安装的真实 Agent 完成降级验收 | 应用整体 | M1-M4 | 生成真实 Agent 输出、final plan、todo 和 evidence 文件 |
| 5.5 | 最终构建验证 | `package.json`, `src-tauri/Cargo.toml` | M1-M5 | `pnpm build`、`cargo check`、`cargo test` 通过 |

## 风险

| 风险 | 可能性 | 影响 | 缓解措施 |
|------|--------|------|----------|
| Codex CLI、Claude Code CLI 与 Amp CLI 参数协议变化 | 高 | 高 | M0 先 probe 当前命令；profile 测试覆盖默认模板；不要硬编码未经验证的旧参数 |
| Claude Code 产品名和 binary 名不一致 | 中 | 中 | UI 显示 Claude Code，配置和执行层固定使用 `claude` |
| CLI 未登录或需要交互授权 | 中 | 高 | 检测失败并给出可读错误；第一阶段不处理交互授权流程 |
| 真实 Agent 输出长、慢或失败 | 中 | 中 | 保存 stdout/stderr evidence，失败隔离，不阻断其它 Agent |
| CLI 调用可能修改用户项目 | 中 | 高 | Planning 阶段默认只读提示，真实写权限留到实施阶段；UI 显示 `canWriteFiles` |
| prompt 文件中包含敏感项目上下文 | 中 | 中 | 第一版只写用户需求、项目摘要和显式约束，不自动塞完整源码 |
| dummy 被误认为产品能力 | 中 | 中 | UI 与文档明确 Mock/Test 标识，验收标准要求至少一个真实 CLI Agent |
| 子进程执行逻辑与 `command_runner` 重复 | 中 | 中 | 第一版可局部实现短命令执行；后续评估是否抽为共享 process runner |

## 待确认问题

- [x] Amp planning profile 第一版沿用 `canWriteFiles: false` 策略；Planning 阶段默认只读。
- [x] Claude Code 在 UI 中显示为 “Claude Code”，实际命令保存为 `claude`；设置页同时展示产品名、adapter 和 binary。
- [ ] 对真实 Agent planning 调用是否允许访问网络，还是仅沿用用户本机 CLI 默认策略。
- [ ] Planning 阶段是否强制只读，还是允许配置 `canWriteFiles` 的 Agent 自行决定。
- [ ] Agent 输出摘要第一版是否继续 deterministic 提取首个计划条目，还是调用单独 summarizer Agent。

## 验证策略

- **M1-M3 后端验证**：使用 Rust 单元测试覆盖 prompt 渲染、参数替换、成功/失败 CLI、部分失败汇总、全部失败状态。
- **M4 前端验证**：运行 `pnpm build`，手动检查无 Codex/Claude Code/Amp、仅 dummy、Codex 成功、Claude Code 成功、Amp 成功、真实 Agent 失败六种状态。
- **端到端验收**：
  1. 启动 Loom。
  2. 登记当前 Loom 仓库为项目。
  3. 配置或自动发现 Codex CLI。
  4. 配置或自动发现 Claude Code CLI（命令名 `claude`）。
  5. 配置或自动发现 Amp CLI。
  6. 输入需求并同时 `@codex`、`@claude`、`@amp` 启动 planning discussion。
  7. 若本机只安装其中一部分真实 Agent，则记录缺失原因并用已安装的真实 Agent 完成降级验收。
  8. 确认 `.loom/planning/<task>/<run>/` 下存在 prompt、stdout、stderr evidence。
  9. 确认选定项目下的 `docs/plans/YYYY-MM-dd-xxx.md` 存在。
  10. 点击 `Confirm Plan` 后，确认任务进入 `ready_to_implement` 并显示 todo。
- **回滚策略**：保留 dummy adapter 和旧任务数据结构兼容；若真实 CLI adapter 不稳定，可临时关闭真实 Agent 入口，但不删除已生成 evidence。
