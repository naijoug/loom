# Agent 调用层（P8 实现，2026-11-03 补模型来源与 `loom-agent`）

`src-tauri/src/agent/mod.rs` 是 Chat、Learn、Research、Discuss 与无窗口 [`loom-agent`](../agent-cli.md) 的公共调用入口。功能模块决定业务流程、数据与提示词；Agent 层负责 CLI 配置/能力、参数、启动检查、执行、流解析和生命周期接口。底层 `process_supervisor` 继续是全应用进程登记与信号处理的唯一实现。

## 公共接口与数据流

```text
功能命令 → load_agents / capabilities / check_startable
         → prepare_invocation(AdapterInvocationRequest)
         → PreparedAgentInvocation → run(RunSpec, cancelled, emit, observe)
         → AgentEvent / AgentOutcome → 功能自己的存储与 IPC 事件
```

| 接口 / 类型 | 实际职责 |
|---|---|
| `load_agents` / `capabilities` | 保存的配置与本机探测；结构化输出、工具事件、模型选择、原生续聊、权限能力 |
| `check_startable` / `forget_startable` | 启动前验证及缓存失效；不证明登录、额度或模型可用 |
| `AdapterInvocationRequest` / `prepare_invocation` | 输入 cwd、prompt、stage、可选模型/原生续聊、`chat_permission_mode`，生成按数组传递的 CLI 参数与 stdin/环境配置 |
| `verify_evidence_profile` / `EvidenceProfile` | 新 Research 的无工具执行配置：真实版本探测后生成不经 IPC 构造的凭据，绑定可执行文件及 Agent 配置；`execution_profile` 为 Some 时只接受 fresh/stdin/planning/explore，拒绝续聊、额外 argv 和版本／配置变化。当前仅验收 Claude 2.1.311；原有功能传 None |
| `PreparedAgentInvocation` | program、args、cwd、stdin_prompt、output_mode、resumed、environment；不是 shell 命令字符串 |
| `RunSpec` | 已准备调用、run_id、ProcessOwner/Kind、stdin_text、Limits、可选 `LineEnricher`；不隐式准备参数 |
| `run` | 真实进程启动、登记、双路排流、取消/超时/输出限额、进程组清理；返回 `Result<AgentOutcome, String>` |
| `AgentEvent` | `Text`、`Part`、`Done`；功能将它们投影到自己的事件 |
| `Observation` | `Started(pid)` 与 `Log { stderr, line }`，供功能保存执行/日志证据 |
| `AgentOutcome` | content、parts、status、exit_code、error_summary、termination_reason、resume_command；退出码 0 不覆盖协议层失败 |
| `begin_operation` / `request_stop` | 应用关闭时拒绝新 admission，按当前登记 run_id 请求停止；不信任历史 PID |
| `begin_shutdown` / `shutdown_snapshot` / `force_stop` / `is_shutting_down` | 对 supervisor 的薄封装；不改变信号、超时、锁或退出语义 |

`AgentStage` 仍是参数准备的一部分，权限仍通过 `chat_permission_mode` 传入；没有另建计划中的 `Permission / AgentRunSpec / AgentErrorKind`。P8 按既有已验证接口记录契约，不为类型命名重写成功路径。公共接口是 Rust crate 内的分层边界，不是独立 SDK 或第三方 ABI。

## 执行器 × 模型来源

一个 Agent 由两维组成：执行器（`adapterType`：claude / codex / opencode / grok / 自定义 CLI / `api`）与可选的模型来源（`providerId`，指向私有 `providers.json` 的一项）。

| 模式 | 组合 | runner |
|---|---|---|
| A 自带登录 | CLI 执行器，无 `providerId` | `agent/runner.rs`：启动并监督 CLI 进程 |
| B CLI + provider | CLI 执行器 + `providerId` | 同上；`agent/injection.rs` 在 `prepare_invocation` 后按单次调用加入 Base URL、模型和 key（key 只放进 `secret_environment`），并清除继承的同族环境变量 |
| C 直连 API | `adapterType = "api"` + `providerId` | `agent/api/`：Anthropic Messages、OpenAI Responses、OpenAI Chat Completions 流式请求；不登记进程，沿用操作守卫、停止原因、超时、输出预算和 `AgentOutcome` 语义 |

- 能注入的协议：Claude Code 只接 Anthropic；Codex 只接 OpenAI Responses；OpenCode 三种都接；Grok 与自定义 CLI 不支持模式 B。诊断以 `providerKinds` 报告，Settings 的下拉据此禁用不兼容项。
- `api` 执行器没有工具、文件访问和原生续聊。功能在请求中声明 `needs_tools`；需要工具时拒绝 `api` Agent。当前 Chat、Learn 作业、Research 需要工具；Discuss 无项目轮次与任何讨论的汇总轮、`loom-agent run` 不需要。
- provider 的增删改与测试由 `agent/providers.rs` 负责（`provider_list/save/delete/test` 四个 IPC 命令，CLI 用同一组 `*_at(dir)` 函数）。写入持锁并校验 revision，被 Agent 引用的 provider 不能删除。测试只检查模型列表：`ok` 不证明对话可用，404/405/501 归为 `model_list_unavailable`，不据此禁用 provider 或自动发起可能收费的对话。key 的存储与 URL 规则见 [安全策略](../security-policy.md#模型来源与-api-key)。
- 续聊指纹包含 `providerId`；换 provider 视同换 Agent，清除原生句柄并重放有界历史。

## 调用方与状态归属

| 调用方 | 进程所有者 | 业务状态与持久化 |
|---|---|---|
| Chat | `ProcessOwner::Chat`，run_id=turn_id | `.loom/chat/v2` 的回合 lease、journal、幂等发送与原生续聊指纹；`chat/runtime.rs` 只映射事件和附加可写 Codex 文件 diff |
| Learn | `ProcessOwner::Learn` | 应用数据目录 study/job；串行阶段、JSON/引用校验、checkout 指纹 |
| Research | `ProcessOwner::Feature { feature: research }` | research/ 内每问报告/job；原生续接失败回放有预算历史，项目变化记警告 |
| Discuss | `ProcessOwner::Feature { feature: discuss }` | discuss/ 内轮次/尝试/汇总；每讨论一个编排、并行轮次屏障、显式重试/过时传播 |
| `loom-agent run` | `ProcessOwner::Feature { feature: cli }`，scope 为 Agent id | 只写 `<config>/cli/runs/<runId>.json` 脱敏执行记录；无会话存储 |

Agent 层不导入功能模块，不决定 Chat 会话、Wiki 页、研究轮次或讨论共识。Chat 的已持久化 `chat_abort / chat_timeout / app_shutdown` 原因字符串保留，其它功能复用这些底层原因并映射自己的状态。

## 错误、探测与缓存

- `AgentError { summary, details }` 转字符串为“摘要 + 空行 + 详情”。IPC 继续使用字符串，前端共享 `ErrorText` 展示可折叠详情，并兼容旧 Chat 文本。
- 诊断版本探测 2 秒；`check_startable` 对内建 CLI 的 `--version` 等待上限 15 秒，自定义 CLI 只检查可解析路径；Dummy 只用于 debug/test。
- 成功结果按 Agent id 缓存，键包含解析后的可执行文件路径与修改时间；失败不缓存。保存配置、重新检测、运行失败清除。`load_agents` 当前会保存合并后的配置，因此也会触发缓存失效；不要把缓存描述为所有连续调用都能命中。
- 版本成功只说明可启动，登录/模型/额度在真实调用中判定。不得把 `available` 或 `ready` 当作完整真实验收。
- `evidenceOnly` 诊断能力默认不声明；仅验证 Claude 2.1.311 且无额外 argv 时为 true。此能力是工具隔离配置，不保证所选模型可用；生产调用前还需 `verify_evidence_profile`，不可把前端布尔值当执行凭据。R0 真实测试与限制见 [验收记录](../dogfood/2026-10-28-research-targets-r0.md)。
- runner 对非零退出优先采用已解析且非空的协议错误摘要（如 `turn.failed`），没有协议错误时使用退出码/信号摘要，并附脱敏 stderr 最后 2000 字符；保留真实退出码和终止原因。退出码为 0 时，CLI 的 `turn.failed` / `result.is_error` 仍判失败。读取/写入/解析/取消失败不自动做通用重试；仅业务层定义的重试行为生效。

## 执行与输出边界

默认 timeout 10 分钟、输出合计 8 MiB、20,000 行、单行 512 KiB；超时/取消先 TERM、必要时 KILL，标准清理预算为 terminate 1 s、drain 300 ms、reap 2 s。Learn 按阶段覆盖为 30/15/10 分钟并使用 64 MiB/4 MiB/200k 行；Research 20 分钟及同大输出限额；Discuss 15 分钟及默认输出限额。

模型提示词尽量走 stdin；必要的长参数在执行记录中省略。结构化日志先保持协议 JSON，再脱敏字段；`loom-agent --verbose` 和执行记录 `stderrTail` 同样使用 JSON 感知脱敏，保留 `usage.input_tokens` 等非密钥字段。非 JSON 日志继续按文本脱敏；不保存完整进程环境。实际只读保障来自 CLI 的 sandbox/plan 策略，不来自提示词或 Loom 独立 OS 沙箱。具体参数映射见 [适配器](../agent-adapter.md)。

## 生命周期与明确例外

Chat 的停止、准入和全应用退出编排通过 `agent` 公共函数访问 supervisor；Learn/Research/Discuss 的关停判断及 Agent 进程归属也只经公共接口。应用级退出编排暂仍位于 `chat/shutdown.rs`，处理所有已登记的进程（不仅 Chat）；这是现有位置，不表示其他功能要引用 Chat。

保留以下非通用 Agent-runner 路径：

1. `learn/fetch.rs`：经加固 Git 拉取公开仓库，直接使用 supervisor 管理 Git 子进程；不启动 Agent CLI。分层检查有具名白名单。
2. `skills/translation/local.rs`：使用原始 stdout 做翻译，不走 Agent 配置；不能直接换为会逐行解析/脱敏内容的 runner。仍有 supervisor、时间/输出上限和取消。Skills 不在四功能 S1 runner 门禁范围。
3. `lib.rs`：Tauri `generate_handler!` 注册命令需要引用 `agent::registry` / `diagnostics` 所在模块；功能调用方只能用再导出。
4. 应用级 updater、非 Agent Git/项目分析和底层诊断使用自己的进程/生命周期能力；不因这次分层变为 Agent 调用。
5. 冻结 Workbench、SDK worker 与 `session_capture.rs` 不在本次搬迁范围；冻结代码不保证适配当前接口。
6. Rust 测试可直接检查 supervisor 的真实登记、信号与清理结果；生产功能代码不能依赖内部模块。

## 分层验证

`scripts/check-layers.mjs` 接入 `pnpm check`：

- Agent 不引用功能模块，Chat/Learn/Research/Discuss 不互相引用，功能生产代码不直接 spawn Agent、不访问 supervisor 或 Agent 私有模块（上面的 Learn Git 例外除外）。
- `agent_cli.rs` 只用 `crate::agent` 再导出与 `models` / `storage` / `projects`，不用 Agent 私有子模块；`src/bin/loom-agent.rs` 只调用 `agent_cli`；其它模块都不引用 `agent_cli`。
- 功能模块不直接用 `reqwest` 发 HTTP 请求，模型调用经 `crate::agent`。具名例外（`HTTP_ALLOWLIST`）：`learn/github.rs`（Learn 新建页 GitHub 搜索，Learn 发现功能实施中）、`skills/github_api.rs`（Skills GitHub 来源）、`skills/translation/remote.rs`（Skills 远程翻译，早于 Agent 层直接调模型；扩展前应迁到 `agent::api`）。
- 前端功能互不导入，共享组件/hooks/api 不导入功能，Settings→Skills 为产品结构例外。

`scripts/lib/rust-production.mjs` 屏蔽测试模块、注释和字面量，但保留测试模块之后的生产代码；这是针对仓库惯用源码形态的静态门禁，不是完整 Rust 语义解析器。相关回归在 `tests/unit/layerCheck.test.cjs`。runner 特征快照、取消/退出子进程测试与真实 CLI/桌面证据共同验证行为，详见 [P8 验收](../dogfood/2026-10-28-p8-acceptance.md)。
