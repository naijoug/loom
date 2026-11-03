# Agent 调用层：无窗口 CLI 与 API Key 模式计划

- 创建时间：2026-11-02 21:54，America/Los_Angeles；22:28 参考 magpie 修订（加入 API Key 模式，重排里程碑）。
- 状态：**待用户确认**，未开始实施。
- 代码基线：`main` `93c36f9`。2026-11-03 复核：`90cf9f9..93c36f9` 只有 Learn 前端重设计（`d59679c`）、本计划入库（`e62f8fb`）和 Skills 截图（`93c36f9`），`src-tauri/src` 无改动，下文行号仍有效。复核按代码修正了 A1 的文件写入依赖、A3 的取消方式与 HTTP 流读取，并补上 `api` 执行器的配置校验与启动检查（见第 2 节新增行）。
- 用户要求（2026-11-02）：
  1. 底层有一个 loom CLI，上层交互都能通过它与本机已安装的 Agent CLI 交互；
  2. 参考 [yetone/magpie](https://github.com/yetone/magpie) 重新思考实现；
  3. 除调用本地 Agent CLI 外，还要支持配置 API Key 直接调用的模式。
- 推荐方向：`crate::agent` 仍是唯一核心。它分出两个独立维度：**执行器**（哪个本地 CLI，或 Loom 直连 API）和**模型来源**（CLI 自己的登录，或用户配置的 API Key / Base URL）。桌面 UI 继续走 Tauri IPC → 同一个库；CLI 是同一个库的另一个入口，桌面不起 CLI 子进程。

## 0. 参考 magpie 的结论（2026-11-02 核对 `3e31962`）

magpie 是 Go 单二进制：本地模型网关（`127.0.0.1:3425`，在 OpenAI Chat / Responses、Anthropic Messages、Gemini 协议间互译）+ 改写各 Agent 自己配置文件的接线器 + 托盘/窗口/TUI/Web/CLI 多入口。它**不运行** Agent 回合，管的是“Agent 用哪个模型、走哪个账号”。

| magpie 做法 | 依据 | 对 Loom 的取舍 |
|---|---|---|
| 执行器与模型来源分离：任意 Agent 配任意 provider 的 `provider/model` | `README.md`、`docs/reference.md#agents` | **采用**：Agent 配置拆成“CLI 执行器 + 可选模型来源”，见 A2 |
| 一个 Go 核心，多入口：GUI 页面经同进程 `/api/*` JSON API，`magpie settings set` 调同一个 `settingsapi` | `docs/subsystems/gui-shell.md`、`main.go` → `internal/cli` | **采用**：印证“桌面与 CLI 共用库、桌面不调 CLI 子进程” |
| 持久改写 `~/.claude/settings.json`、`~/.codex/config.toml` 等全局配置，靠 stash/drift 记录与还原 | `docs/subsystems/agent-wiring.md` | **不采用**：Loom 只需按单次运行生效，用每次调用的环境变量 / CLI 参数注入，不改用户全局文件，也就不需要 drift/还原 |
| 只读环境变量的 Agent 用启动包装（`magpie agy run` 带 `GEMINI_API_KEY`、`GOOGLE_GEMINI_BASE_URL`、`--model`） | `agent-wiring.md` “Environment-only agents” | **采用**：这正是 Loom 的注入方式（`PreparedAgentInvocation.environment` 已存在） |
| Codex 走 `-c` 覆盖 provider（T3 Code 的 `launchArgs`） | `docs/reference.md` T3 Code 行 | **采用**：Codex 的 provider 绑定用 `-c model_providers.*` 参数，key 走环境变量 |
| provider 存 `providers.json`（0600，含 key），多 key、协议标注、无 key 的本机/内网地址 | `docs/subsystems/providers-accounts.md`、`reference.md#files` | **部分采用**：沿用 Loom Skills 翻译已有的私有版本化文件与“前端只见存在性 + 来源”约定；本机/内网地址允许 http |
| 网关做协议互译、多账号 failover、路由组、用量、隐私遮蔽 | `gateway-routing.md` 等 | **不采用**：体量大且非 Loom 范围。用户可把 Base URL 指向 magpie 网关得到这些能力 |
| 读取 Agent 的订阅登录去服务别的 Agent；部分订阅因违反厂商条款移出到插件 | `AGENTS.md`“Built-in subscriptions that a plugin serves” | **明确不做**：Loom 不读取 CLI 的登录凭据（`~/.codex/auth.json`、Keychain）来直连 API |

## 1. 目标、非目标与成功标准

目标：
1. **模式 A：本地 CLI，自带登录**（现状）。不变。
2. **模式 B：本地 CLI + API Key**。仍由 Claude Code / Codex / OpenCode 运行回合（工具、权限、沙箱都是 CLI 的），Loom 在单次调用中注入用户配置的 Base URL、Key 和模型。
3. **模式 C：直连 API**（无 CLI）。Loom 直接请求 Anthropic Messages / OpenAI Responses / OpenAI 兼容 Chat Completions，流式输出映射成同样的 `AgentEvent` / `AgentOutcome`。首版**不带工具**，只服务纯文本任务。
4. 无窗口 CLI（`loom-agent`）覆盖三种模式：`list`、`doctor`、`run`、`provider`（list/add/set-key/remove/test）。
5. 三种模式对功能模块是同一个接口：功能只说“用哪个 Agent 配置、需不需要工具”，不关心底下是哪种模式。

非目标：
- 不做本地网关、协议互译、多账号 failover、路由组、用量统计、隐私遮蔽（需要时把 Base URL 指向 magpie）。
- 不改写任何 Agent CLI 的全局配置文件；不读取 CLI 的订阅凭据。
- 模式 C 首版不实现工具调用循环（读文件 / 执行命令 / 写文件）。冻结的新内核和 SDK worker 仍保持冻结。
- 桌面 UI 不改为调用 CLI 子进程；已有 Tauri 命令与 IPC 契约保持兼容。
- 不修改或删除已有 Chat / Learn / Research / Discuss 数据。

成功标准：
- 模式 B：用一个 Anthropic 兼容 provider 跑 Claude Code、用一个 OpenAI 兼容 provider 跑 Codex，各一次真实调用成功；进程参数（`ps`）与执行记录中都没有 key；用户 shell 里已有的 `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` 不会混进来。
- 模式 C：Anthropic 与一个 OpenAI 兼容 provider 各一次流式调用成功，可中途停止，超时生效；Discuss 无项目讨论可以用模式 C 的参与者跑完两轮 + 汇总。
- 指向本机 magpie 网关（`http://127.0.0.1:3425`）的 provider 能用（无 key 或任意 key）。
- `loom-agent` 与桌面读到同一份配置；`pnpm check`（含分层门禁）通过；新增 Rust 测试通过。

## 2. 当前状态（已核实）

| 事实 | 位置 |
|---|---|
| 四个功能都经 `agent::prepare_invocation` + `agent::run` 启动 CLI | `chat/runtime.rs:451`、`learn/jobs.rs:507/534`、`research/jobs.rs:317/347`、`research/target_runtime.rs:415/443`、`discuss/jobs.rs:256/313` |
| `AgentConfig` 只有 command/args/adapter/权限字段，没有模型来源 | `models.rs:140` |
| `PreparedAgentInvocation.environment` 已存在（OpenCode 用它传 `OPENCODE_PERMISSION`），runner 用 `.envs()` 追加，**不清理**继承的环境 | `agent/invocation.rs:71`、`agent/runner.rs:316` |
| `load_agents` 需要 `AppHandle`，每次加载都会回写 `agents.json` | `agent/registry.rs:14, 171-197` |
| 配置目录：`LOOM_PROFILE_DIR` 优先，否则 `app_data_dir()/loom`；Skills CLI 用 `dirs` 无 `AppHandle` 复现同一路径 | `storage.rs:159`、`skills/runtime_paths.rs` |
| Skills 翻译已经用 API Key 直连 OpenAI Responses / OpenRouter：私有版本化配置、`KeyChange` keep/set/clear、`KeySource` Environment/Stored/None、前端只拿到“有没有 key、来自哪” | `skills/translation/config.rs`、`skills/translation/remote.rs`、`docs/security-policy.md:20` |
| `reqwest 0.13`（rustls）已是依赖，当前未开流式特性 | `src-tauri/Cargo.toml:36` |
| Research 目标运行已有“无工具执行配置” `EvidenceProfile`：材料由 Loom 提供，只接受 fresh/stdin/planning | `agent/evidence.rs`、`docs/architecture/agent-layer.md` |
| Discuss 无项目时、以及主持汇总轮，提示词只含主题与同伴答案，不依赖读文件 | `discuss/prompt.rs:4-40` |
| Chat 用 `permission_to_stage` 把 explore/ask/auto 映射到 `AgentStage` | `chat/mod.rs:117` |
| `lib.rs` 中 `mod agent` 私有；`pub mod skills` 供 `loom-skills` 使用；桌面主程序在 Windows release 下是 `windows_subsystem = "windows"`（无控制台） | `lib.rs:1, 22`、`main.rs:2` |
| 分层门禁禁止 `agent/` 引用任何功能模块，`skills` 也算功能模块；私有文件写入（目录 0700、文件 0600、原子替换）目前只在 `skills::files` | `scripts/check-layers.mjs:17, 47-52`、`skills/files.rs:56, 117-145` |
| Agent 配置校验要求非空 `command` 且 adapter 属于 CLI 列表；`check_startable` 先解析可执行文件路径再探测版本 | `agent/registry.rs:142-167`、`agent/diagnostics.rs:155-179` |
| 停止不靠进程登记：功能把自己的取消标志传给 runner 的 `cancelled` 闭包（Chat 用 `lease.stop_reason()`，Discuss 用 `lease.cancel`），`request_stop` 只对已登记的进程发信号，找不到 `run_id` 时返回 `false`；关停等待 `OperationGuard` 计数归零 | `chat/service.rs:22-24`、`chat/mod.rs:393-395`、`discuss/jobs.rs:46, 94`、`process_supervisor.rs:211-223, 340-365`、`chat/shutdown.rs:116-140` |
| reqwest 0.13.3 的 `Response::chunk()` 不需要 `stream` 特性（`bytes_stream()` 才需要） | `reqwest-0.13.3/src/async_impl/response.rs:315, 345-356` |
| 新 Tauri 命令要同时登记到 IPC 契约清单 | `contracts.rs` 的 `COMMANDS`、`src/api/contract.ts` |
| Discuss 参与者按 `agent_id` + `model` 去重；不同 provider 的同一 CLI 是不同 Agent 配置，不冲突 | 规则见 `docs/requirements.md:51`；字段见 `discuss/models.rs:21-24, 145-146` |

假设（实施前验证）：
- 新增 `[[bin]]` 会像 `loom-skills` 一样打进 `.app/Contents/MacOS/`。
- 各 CLI 当前版本的 provider 注入方式（A2 表格）需逐个用真实 CLI 核对，不以文档为准。

## 3. 目标结构

```text
功能（Chat / Learn / Research / Discuss）    loom-agent CLI      桌面 Settings
            │  只经 agent/mod.rs 公共接口         │                    │
            ▼                                    ▼                    ▼
  agent::resolve(profile, needs) ──► Invocation::Process(PreparedAgentInvocation)  ──► runner（进程）
                                └──► Invocation::Api(PreparedApiRequest)          ──► api runner（HTTP 流）
                                                    │
                       providers.json（私有，0600）：kind / base_url / model / key 或 env 名
```

- **Agent 配置**（`agents.json`）：执行器 = `cli`（现有 adapter）或 `api`；可选 `provider_id`。
  - `cli` + 无 provider = 模式 A；`cli` + provider = 模式 B；`api` + provider = 模式 C。
- **Provider 配置**（新 `providers.json`）：`id`、`kind`（`anthropic` / `openai-responses` / `openai-chat`）、`base_url`、默认 `model`、key 来源（存储的 key，或环境变量名）。
- 两个 runner 产出同一套 `AgentEvent` / `AgentOutcome`，共用 `Limits`、超时、取消、关停准入与 `run_id` 登记。

## 4. 里程碑与任务

### A0 无 `AppHandle` 的只读配置加载（纯重构）

- `storage.rs`：抽出 `system_config_dir()`（`LOOM_PROFILE_DIR` → `dirs::data_dir()/<tauri.conf identifier>/loom`），`global_config_dir(app)` 复用同一实现。
- `agent/registry.rs`：新增 `read_agents_at(path)`：文件不存在时返回内存中的默认项；存在时读取、合并默认项、刷新 `available`，**不回写**。`load_agents(app)` 行为不变。
- 测试：`system_config_dir()` 与 Tauri 路径相等；`read_agents_at` 不改文件、不创建文件。
- 验证：`cargo test --manifest-path src-tauri/Cargo.toml agent::registry storage`。

### A1 Provider 配置与密钥

- 先把私有文件写入（`private_directory`、0600 原子写）从 `skills::files` 移到共享的 `storage`，`skills::files` 改为调用它，行为不变。原因：`agent/` 不能引用 `skills`（分层门禁），而 `storage` 已经是 `agent/registry.rs` 的依赖。
- 新 `agent/providers.rs`（只经 `agent/mod.rs` 再导出）：版本化 `providers.json`，用上面的私有原子写；跨进程写用 `std::fs::File::try_lock`（同 `chat/repository.rs:285`、`skills/coordination.rs:174`）+ `revision` 乐观校验（照 Skills 翻译的 `expectedRevision`）。
- 密钥规则照 Skills 翻译：`KeyChange` keep/set/clear；`KeySource` stored/environment/none；IPC 与 CLI 输出只给“有无 key、来源”；输入结构不实现 `Debug`/`Serialize`；CLI 只从 stdin 读 key，不收命令行参数里的 key（防止进 shell 历史和 `ps`）。
- `base_url` 校验：必须 https；http 只允许 loopback / 私网 / `.local`（覆盖 magpie 网关、Ollama、LM Studio）；这类地址允许无 key。
- Tauri 命令：`provider_list` / `provider_save` / `provider_delete` / `provider_test`（`test` 只发一次最小请求，错误按分类返回，不回显上游正文），同时登记到 `contracts.rs` 的 `COMMANDS` 与 `src/api/contract.ts`。
- 测试：权限位、revision 冲突、key 不出现在任何序列化输出、URL 校验正反例。

### A2 模式 B：CLI + API Key 注入

- `AgentConfig` 增加可选 `provider_id`（schema 版本 +1，旧文件迁移为 `None`，行为不变）。
- `agent/capabilities.rs` 为每个内建 adapter 声明 `provider_override` 能力及支持的 `kind`；`prepare_invocation` 按 adapter 生成注入：

| CLI | 注入方式（待真实核对） |
|---|---|
| Claude Code | 环境 `ANTHROPIC_BASE_URL`、`ANTHROPIC_API_KEY`（或 `ANTHROPIC_AUTH_TOKEN`）；`--model` |
| Codex | `-c model_provider="loom"`、`-c model_providers.loom.base_url=…`、`-c model_providers.loom.wire_api=responses|chat`、`-c model_providers.loom.env_key="LOOM_PROVIDER_KEY"`；环境 `LOOM_PROVIDER_KEY`；`-m` |
| OpenCode | 环境注入 provider 配置（与已有 `OPENCODE_PERMISSION` 同路径）；具体变量名实施时核对 |
| Grok / 自定义 CLI | 首版不支持，`prepare_invocation` 明确拒绝 |

- `PreparedAgentInvocation` 增加 `environment_remove`：模式 B 运行时移除继承环境中同族变量（如 `ANTHROPIC_*`、`OPENAI_*`、`CODEX_*` 的 key/base），避免用户 shell 里的 key 混进来；runner 调 `env_remove`。
- 执行记录、`Observation::Log` 和错误里不出现 key；`args` 中只有 Base URL，没有 key。
- 不改写 `~/.claude`、`~/.codex` 等任何文件。
- 测试：每个 adapter 的注入快照；key 不在 args；`environment_remove` 生效；不支持的组合有明确错误。

### A3 模式 C：直连 API runner（无工具）

- `agent/api/{mod,anthropic,openai_responses,openai_chat,sse}.rs`：构建请求（system + 用户 prompt，或有界历史回放），SSE 流解析 → `AgentEvent::Text` / `Done`；错误按 HTTP 状态 / 协议错误分类为 `AgentOutcome`，不回显上游正文。
- `RunSpec.prepared` 改为 `Invocation::{Process, Api}`；`agent::run` 分派。API runner 和进程 runner 一样先拿 `begin_operation` 守卫（关停会等它释放），在读取响应时用 `select!` 同时等下一个 chunk 和一个短间隔，每次检查 `cancelled()` 与 `is_shutting_down()`，停止时丢弃请求并返回对应的 `STOP_REASON_*`。不改 `process_supervisor`：功能已经通过自己的取消标志停止运行，`request_stop` 对 API 运行返回 `false` 无副作用。
- `api` 执行器的配置与检查：`validate_input` 接受 `api` adapter，且此时不要求 `command`、改为要求 `provider_id`；`available` 和 `check_startable` 只检查 provider 存在、URL 合法、key 来源可用，不发网络请求、不解析可执行文件；`diagnose_agents` 给出同样的结果。
- 能力声明：`api` 执行器没有 tools / write / commands / 原生续聊；续聊只走 `chat_context` 的有界历史回放。
- 功能侧只加一个声明：调用时给出 `needs_tools`。`needs_tools = true` 时选到 `api` 执行器直接报“此功能需要本地 CLI”。首批开放：
  - `loom-agent run`；
  - Discuss：无项目讨论的参与者，以及任何讨论的主持汇总轮；
  - 其余（Chat、Learn、Research、有项目的 Discuss 参与者）首版仍要求 CLI，待确认问题 3。
- 用现有 `reqwest` 的 `Response::chunk()` 逐块读取 SSE，不改 Cargo 特性、不新增 crate；HTTP 超时、响应体上限与 `Limits` 一致。
- 测试：本地假 SSE 服务（测试内起 `127.0.0.1` 监听）覆盖文本流、停止、超时、截断、错误分类；与进程 runner 的 `AgentOutcome` 字段语义对齐。

### A4 `loom-agent` CLI

- `src-tauri/src/agent_cli.rs` + `src-tauri/src/bin/loom-agent.rs`（照搬 `loom-skills` 的输出与退出码处理），`[[bin]] name = "loom-agent"`。
- 子命令：
  - `list [--json]`、`doctor [<agent-id>] [--json]`；
  - `provider list|add|set-key|remove|test`（`set-key` 从 stdin 读）；
  - `run --agent <id> [--provider <id>] [--project <dir>] [--mode explore|ask|auto] [--model <name>] [--resume <session-id>] [--prompt … | --prompt-file … | stdin] [--json]`；不给 `--agent` 只给 `--provider` 时走模式 C。
- 参数手写解析（同 `skills/cli.rs`）；`--project` 用 `projects::canonical_project_path`；`--mode` 默认 `explore`，stage 复用移到 agent 层的 `permission_to_stage`。
- 输出：默认人读（正文到 stdout，工具与状态到 stderr）；`--json` 为 NDJSON：`started`（模式 A/B 带 pid）、`text`、`part`、`log`（`--verbose`）、`done`（status / exitCode / errorSummary / terminationReason / resumeSessionId）。DTO 定义在 `agent_cli`，不给内部类型加 `Serialize`。
- 退出码：0 完成；1 失败；2 用法错误；124 超时；130 中断。Ctrl-C 一次请求停止，两次 `force_stop`。
- 每次 `run` 写脱敏执行记录 `<config_dir>/cli/runs/<run_id>.json`：时间、Agent / provider id、模式、cwd、model、脱敏 args（长 prompt 省略）、结束状态与 stderr 尾部；不存 prompt 正文、环境变量和 key。
- 测试：参数表驱动；同参数下 CLI 与 Chat 生成的调用一致；子进程桩与假 SSE 服务覆盖三种模式的 NDJSON 形状。

### A5 桌面 Settings 与选择器

- 先在 `designs/` 出设计稿：Settings → Providers 列表与编辑（预设：Anthropic、OpenAI、OpenRouter、OpenAI 兼容、Anthropic 兼容、本机网关），Agent 编辑里增加“模型来源”下拉。
- `AgentPicker` 显示“执行器 · 模型来源”（如 `Codex · DeepSeek`、`API · Anthropic`）；`needs_tools` 的功能里 `api` 项置灰并说明原因。
- 视觉矩阵补 Providers 页（中英文 × 深浅 × 700px）。
- 依赖：A1–A3；设计稿经用户确认后再写代码。

### A6 门禁与文档

- `scripts/check-layers.mjs`：`agent_cli` 只用 `agent/mod.rs` 再导出及 `storage` / `projects` 公共函数；功能模块不引用 `agent_cli`、不直接用 `reqwest` 调模型；`tests/unit/layerCheck.test.cjs` 加正反例。
- 文档：新增 `docs/agent-cli.md`（用法、NDJSON、退出码、三种模式）；更新 `docs/architecture/agent-layer.md`（执行器 / 模型来源、两种 runner）、`docs/agent-adapter.md`（各 CLI 注入方式与核对版本）、`docs/security-policy.md`（provider key 存储、环境清理、URL 规则）、`docs/architecture.md`、`docs/testing.md` 验证矩阵、`docs/requirements.md`（用户确认后加入 API Key 模式范围）。

### A7 真实验收

- 临时 `LOOM_PROFILE_DIR` + 临时项目：模式 A（Codex）、模式 B（Claude Code + Anthropic 兼容 provider；Codex + OpenAI 兼容 provider）、模式 C（Anthropic、OpenAI 兼容各一）、指向本机 magpie 网关一次（若本机已装）、Discuss 无项目讨论用模式 C 跑完。
- 每次检查 `ps` 参数、执行记录、日志无 key；Ctrl-C 后无残留进程组。
- 结果写 `docs/dogfood/2026-11-xx-agent-modes.md`，注明 CLI 版本、provider、哪些是合成测试、哪些是真实模型；没有 key 的 provider 记 blocked，不写成通过。

依赖顺序：A0 → A1 → (A2 ∥ A3) → A4 → A5 → A6 → A7。A2 与 A3 改动不同文件，可以并行；A4 只依赖它们的公共接口。

## 5. 风险

| 风险 | 处理 |
|---|---|
| key 泄露到 argv、日志、执行记录或前端 | key 只走环境变量；CLI 只从 stdin 收 key；输入类型不实现 Debug/Serialize；执行记录不存环境；测试断言 |
| 用户 shell 里已有的 key 混入模式 B 运行 | `environment_remove` 清理同族变量，测试覆盖 |
| CLI 升级后注入方式失效 | 注入方式按 adapter 能力声明并记录核对版本；`doctor` 对模式 B 做一次最小探测 |
| 模式 C 被误用在需要读代码的功能上，给出无依据回答 | `needs_tools` 显式拒绝；UI 置灰说明 |
| providers.json 被桌面与 CLI 并发写 | 文件锁 + revision；`agents.json` 仍只由桌面写 |
| 直连 API 的 HTTP 取消与进程取消语义不一致 | 两种 runner 共用 `cancelled` 闭包、`OperationGuard` 与 `STOP_REASON_*`；测试对齐 `AgentOutcome`，并覆盖关停时 API 运行能在预算内结束 |
| 移动 `skills::files` 的私有写入函数影响 Skills | 只移动实现、保留原函数签名转调；跑 Skills 现有测试 |
| 走第三方中转时的数据去向 | Settings 与文档写明：请求直接发往用户配置的 Base URL，Loom 不做遮蔽 |

回滚：A0 纯重构可单独回退；A1–A4 以新增文件与可选字段为主（`provider_id` 为 `None` 时行为与现状一致）；A5 前端改动独立。

## 6. 待确认问题

1. **密钥存储**：推荐首版与 Skills 翻译一致，用私有 0600 文件 + 环境变量来源；macOS Keychain 需要新增依赖，放到后续。
2. **CLI 入口形态**：magpie 是一个二进制带子命令。推荐 Loom 保持独立 `loom-agent`：桌面主程序在 Windows release 下没有控制台输出，与 `loom-skills` 也一致。若只考虑 macOS，也可在 `main.rs` 按参数分派成 `loom agent …`。
3. **模式 C 首批开放范围**：推荐 `loom-agent run` + Discuss（无项目参与者与主持汇总）。Chat 的“纯对话、无工具”与 Research 无工具目标运行放到下一步。
4. 沿用上一版的问题：`loom-agent` 是否随 `.app` 打包（推荐是）；`run` 是否允许自定义 CLI（推荐允许，但模式 B 不支持）；`--timeout-secs`（推荐提供，上限 20 分钟）；执行记录保留（推荐首版不自动清理）。

## 7. 验证策略

- 每个里程碑跑相关 `cargo test --manifest-path src-tauri/Cargo.toml <模块>`。
- A4 后：`pnpm check`、`pnpm tauri build --no-bundle`。
- A5 后：`pnpm smoke:visual`（Providers 页与 AgentPicker）。
- A7：真实调用按上文记录；未运行或缺 key 的项写 blocked，不写成通过。
