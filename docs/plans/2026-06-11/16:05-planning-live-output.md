# Planning 执行过程可视化、失败可读化与原生 Session 互通 — Plan

- **日期**: 2026-06-11
- **作者**: Claude (Fable 5)
- **状态**: implemented（v3：已修正 session 策略，不预生成 Claude UUID；实现双 CLI 结构化事件流、planning live log、session/resume、失败原因与 evidence 打开入口）

## 背景与问题（实测反馈）

时间线工作台（`11:15-planning-v2-timeline.md` 已实施）真实使用后暴露三个问题：

1. **成功行没有执行过程**：起草 Agent 是单次阻塞调用——`run_cli_profile` 用 `read_to_end` 等进程退出才拿到全部输出，过程中前端只有一个 "running" 状态事件。
2. **失败看不到真正的错误**：失败行的 "Error details" 原样贴 stderr 尾部 20 行。实测截图中 Codex "failed · 4m 4s" 实际是被 `PLANNING_TIMEOUT_MS = 240_000` 超时杀掉的（codex 往 stderr 写工作日志，详情里看到的是计划片段和用户自己 hook 的回显，而不是失败原因）；partial 输出已落盘但 UI 不暴露。
3. **中间过程与原生工具割裂**：用户期望每次调用既能在 Loom 看到过程，也能在 codex / claude 原生工具里找到同一个 session 继续操作。

### 方案选型结论（v2 讨论）

| 通道 | 结论 | 理由 |
|------|------|------|
| 官方结构化事件流（`claude -p --output-format stream-json`、`codex exec --json`） | **采用**，进度与最终产物的唯一来源 | 双 CLI 官方 headless 集成路径；含增量文本、工具调用、session id；本机已验证 flags 存在 |
| Hook（claude hooks / codex notify） | **排除** | 定位是生命周期拦截与通知，需注入用户配置 + 自建 IPC 回传，信息量反而少；用户 stderr 里的 `hook: Stop` 噪音即其副作用 |
| 刮 stderr 文本（v1 计划中 codex 的做法） | **放弃** | `--json` 提供结构化等价物，无需解析非契约文本 |
| Session 绑定 | **采用** | claude `-p` session 默认持久化（`--no-session-persistence` 为反向开关），用 `--name` 标记 Loom 会话并从 `system/init` 捕获 `session_id`，不预生成 UUID；codex session 默认落盘，`codex resume <id>` 可恢复。Loom 记录 id 即天然"两边都在" |

## 目标

1. 起草/互评/合成执行期间，agent 行内可见**实时输出 tail**（来自官方事件流的增量文本与动作事件），完成后保留为可折叠执行日志。
2. 每次调用绑定原生 session：claude 以 `--name "Loom · <任务>"` 发起并从 `stream-json` init 事件捕获 session id，codex 从 `--json` 事件捕获 session id；时间线提供 "Resume in Claude Code / Codex"（v1 展示并复制 resume 命令）。
3. 失败行第一行给出**具体原因**（Timed out after Ns / Exited with code N / 配置错误命中行）；关键错误行置顶；完整日志一键打开；超时 partial 输出可查看。
4. 切换事件流后，最终计划 stdout 与现状等价（不丢内容——见 claude plan-mode 历史教训）。

## 非目标

- 不做 hook 集成（见选型结论）。
- 不在 Loom 内嵌交互式恢复（v1 只给 resume 命令；终端唤起留待后续）。
- 不做日志搜索/过滤 UI；不做工具调用的语义化卡片（只做文本行流）。

## 成功标准

- 真实跑一轮 Codex+Claude：两个 agent 行运行中都有滚动实时输出；完成后日志可回看；每行 invocation 显示 session id 与可复制的 `claude --resume <id>` / `codex resume <id>` 命令，且在对应原生 CLI 中能真实恢复该 session。
- 人为缩短超时复现：失败行显示 "Timed out after Ns"，"View partial output" 打开已生成的部分计划；错误详情第一行是关键错误行而非 hook 噪音。
- claude stream-json 与 codex --json 路径的最终 stdout 与 text/裸模式产出等价（真实 CLI smoke）。
- `cargo test` / `cargo clippy` / `pnpm test` / `pnpm build` 通过。

## 当前状态（代码盘点）

| 模块 | 现状 | 处置 |
|------|------|------|
| `agents.rs` `run_cli_profile` | `read_to_end` 阻塞聚合，无中间事件 | 改逐行读取 + 批量 emit，聚合结果不变 |
| `command_runner.rs:49` `loom://command-log` | 已有逐行日志事件 + 前端环形缓冲（300 行）模式 | 模式照搬到 planning |
| `agents.rs` `default_profile_args` | codex `exec --sandbox read-only -`；claude `-p --output-format text` | codex 增 `--json`；claude 改 `--verbose --output-format stream-json --include-partial-messages --name "Loom · <task>"` |
| `models.rs` `AgentInvocation` | 无 session / stderr evidence 信息 | 增 `session_id`、`resume_command`、`stderr_ref`、`failure_detail`、`error_lines`（serde default），保留 `evidence_ref` 作为 stdout/primary evidence |
| `agents.rs` `classify_failure` / `failure_kind` | 已分类，UI 只显示通用文案 | 增 `failure_detail` 带具体参数 + 关键错误行提取 |
| `agents.rs:32` `PLANNING_TIMEOUT_MS` | 240s，实测真实重构规划被杀 | 默认 480s |
| 超时 partial stdout | 已写 `<agent>.stdout.md` evidence 且在 `raw_output`，UI 未暴露 | "View partial output" 复用 `open_plan_viewer`（白名单已含 `.loom/planning` 的 `.md`） |
| `reducer.ts` `commandLogs` | 环形 300 行模式 | 新增 `planningLogs` 同模式 |
| `PlanningTimeline.tsx` agent 行 | 状态/耗时/Retry/Error details | 增 live tail、执行日志、原因行、resume/日志/partial 按钮 |
| uuid 生成 | 无依赖 | 不引入 `uuid` crate，不传 `--session-id`；claude session id 从 init 事件捕获 |

## 里程碑

依赖：M1 → M2 → M4/M5；M3 独立可先行；M6 收尾。

### M1 — 后端流式日志管道

**Outcome**: 起草/互评/合成的输出逐行（节流批量）推送到前端，聚合行为与现状一致。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 1.1 | 新事件 `loom://planning-agent-log`：`PlanningAgentLogEvent { task_id, planning_run_id, agent_id, agent_name, phase, attempt, stream("stdout"/"stderr"), lines, timestamp_ms }`；行过 `redact_sensitive_text` | `models.rs`、`agents.rs` `PlanningEventEmitter` 增方法 | — | dummy emitter 单测 |
| 1.2 | `run_cli_profile` 改逐行读取（`BufReader::lines` + select 循环），每 100ms/32 行批量 emit；同时累积完整字符串，返回结构不变；超时分支保持 kill+聚合 | `agents.rs` | 1.1 | 既有单测全绿 + sh 脚本逐行流式单测 |
| 1.3 | 起草/互评/合成调用链透传日志上下文（互评用 pair 标识，与状态事件一致） | `run_planning_agent` / `run_synthesis_agent` / `run_plan_review_agent` | 1.2 | 互评日志 pair 单测 |

### M2 — 双 CLI 原生结构化事件流 + session 捕获

**Outcome**: claude 与 codex 的增量文本/动作进日志流，最终 stdout 等价，session id 落到 invocation。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 2.1 | claude profile 改 `-p --verbose --output-format stream-json --include-partial-messages`，调用时追加 `--name "Loom · <task title>"`（保留 text 解析为降级路径；注释链接 plan-mode 教训） | `default_profile_args` / profile builder | — | profile 单测 |
| 2.2 | `parse_claude_stream`：`system/init` → 捕获 `session_id`；`assistant` 文本增量 → 日志行；`result` → 最终 stdout；脏行原样进日志不 panic；无 result fallback 拼接 | `agents.rs` 新模块/函数 | M1 | 样例流单测（init/assistant/result/脏行/缺 result） |
| 2.3 | codex profile 增 `--json`；`parse_codex_stream`：`thread.started`（或等价事件）→ session id；agent message/command 事件 → 日志行；最终 agent message → stdout；脏行容错 | `default_profile_args`、新解析函数 | M1 | 样例流单测 |
| 2.4 | `AgentInvocation` 增 `session_id: Option<String>`、`resume_command: Option<String>`（serde default）：claude → `claude --resume <id>`，codex → `codex resume <id>`；前端 domain 同步 | `models.rs`、`src/domain/task.ts` | 2.2, 2.3 | 旧数据反序列化 + 字段单测 |
| 2.5 | claude 调用加 `--name "Loom · <task title>"`（截断 ≤40 字符）；session id 一律从 init 事件捕获（零依赖方案，已确认） | `agents.rs` | 2.2 | 手测 `/resume` 列表可辨识 |
| 2.6 | 真实 CLI smoke：双工具各跑一次，断言 stdout 章节完整 + session 可在原生 CLI resume | 既有 `#[ignore]` 测试扩展 | 2.1-2.5 | `cargo test -- --ignored` 手动 |

### M3 — 失败原因可读化

**Outcome**: 失败一眼可见原因；完整日志一键打开；partial 输出可查看。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 3.1 | `AgentInvocation` 增 `failure_detail: Option<String>`：timeout → "Timed out after Ns"；nonzero_exit → "Exited with code N"；not_retryable → 命中的 stderr 关键行；另增 `stderr_ref` 指向完整 stderr/log 文件 | `models.rs`、`agents.rs` | — | 各分支单测 |
| 3.2 | `extract_error_lines(stderr)`（`Error:`/`error:`/`fatal:`/`panic` 前缀，≤5 行）存入 `error_lines`；hook 回显等噪音不入选 | `agents.rs` | — | codex 噪音样本单测 |
| 3.3 | 新命令 `open_planning_evidence(project_path, evidence_path)`：白名单限 `.loom/planning/`（任意后缀），opener 打开；注册 lib.rs + bridge | `plan_html.rs`、`lib.rs`、`useTaskBridge.ts` | — | 白名单负例单测 |
| 3.4 | `PLANNING_TIMEOUT_MS` 240s → 480s；超时文案含实际预算 | `agents.rs:32` | — | 超时单测更新 |

### M4 — 前端 Live output 与失败展示

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 4.1 | reducer 增 `planningLogs`（key `${runId}:${phase}:${agentId}`，环形 300 行）；`useAgentBridge` 订阅；新 run 清旧 key | `reducer.ts`、`useAgentBridge.ts`、domain | M1 | reducer 单测 |
| 4.2 | agent 行运行/重试中内嵌 live tail（约 8 行、自动滚底）；结束后 `<details>` "Execution log" | `PlanningTimeline.tsx`、`Planning.css` | 4.1 | preview + 手测 |
| 4.3 | 失败行重排：首行 `failure_detail` → 关键错误行块 → "Open log file"（`stderr_ref`）/ "View partial output"（`evidence_ref` 且 stdout 非空时）；成功行补 `outputSummary` | `PlanningTimeline.tsx` | M3 | preview 三态截图 |

### M5 — Session 互通 UI

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|------|-------------|------|------|
| 5.1 | agent 行（成功与失败均）显示 session 标识 + "Copy resume command"（剪贴板写 `resume_command`；用 navigator.clipboard，Tauri webview 支持） | `PlanningTimeline.tsx` | M2 | 手测复制 → 原生 CLI 恢复 |
| 5.2 | preview mock 补 sessionId/resumeCommand 数据 | `PlanningPreviewApp.tsx` | 5.1 | preview 截图 |

### M6 — 验证与收尾

| # | 任务 | 验证 |
|---|------|------|
| 6.1 | `cargo test` / `cargo clippy` / `pnpm test` / `pnpm build` | 全绿 |
| 6.2 | 真实端到端：Codex+Claude 一轮看双路 live 输出 → 临时调小 timeout 复现超时与 partial → 复制 resume 命令在原生 CLI 恢复 → `scripts/preview.sh stop` | 手测记录 |

## 风险

| 风险 | 可能性 | 影响 | 缓解 |
|------|--------|------|------|
| claude stream-json / codex --json 事件格式随版本变化 | 中 | 高 | 解析器宽容（未知行进日志不 panic）；缺终值 fallback 拼接增量；真实 smoke 把关；极端情况下整体回退 text/裸模式（丢中间过程不丢结果） |
| codex `--json` 标注 experimental | 中 | 中 | 同上 fallback；解析失败时原始行仍进日志，最终 stdout 退化为全文 |
| 日志事件风暴拖慢 UI | 中 | 中 | 后端 100ms/32 行批量；前端环形 300 行只渲染尾部 |
| `run_cli_profile` 签名改动波及面 | 中 | 中 | emitter/回调注入，dummy 路径不发日志；既有单测守护聚合等价 |
| resume 命令跨环境不可用（PATH/版本差异） | 低 | 低 | 只展示+复制，不代执行；命令旁标注 agent 名称 |
| 逐行 redact 漏检跨行 secrets | 低 | 中 | evidence 落盘仍全量 redact；事件行级 redact 覆盖单行模式；完整 stderr/log 通过 `stderr_ref` 打开前也只写入 redacted 内容 |

## 待确认问题

- [x] claude session id 采用"读 init 事件返回值"（零依赖）。已确认（2026-06-11，用户拍板）：不引入 uuid 依赖，不传 `--session-id`；`parse_claude_stream` 从 `system/init` 事件捕获 `session_id`，缺失时 `resume_command` 留空并降级为不展示 resume 入口。
- [ ] 超时默认 480s 是否合适（后续是否进 Settings 可配）？
- [ ] "Resume in tool" v2 是否要一键唤起终端（macOS `osascript` 打开 Terminal 跑命令）？v1 先复制命令。
- [ ] 互评/合成的 live 输出与起草同等展示（默认是；嫌噪可只展示起草）。

## 验证策略

- 每里程碑：对应单测 + `cargo clippy` + `pnpm build`；M2 必须有双 CLI 真实 smoke。
- 端到端见 6.2。
- 回滚：M2 的两个 profile 改动可各自独立 revert 回 text/裸模式；M1/M4/M5 为新增事件与 UI 可独立 revert；新模型字段全 `serde(default)`。
