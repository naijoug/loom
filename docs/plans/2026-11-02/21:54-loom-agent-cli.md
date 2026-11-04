# Agent 调用层：无窗口 CLI 与 API Key 模式计划

- 创建时间：2026-11-02 21:54，America/Los_Angeles；22:28 参考 magpie 修订（加入 API Key 模式，重排里程碑）。
- 状态：A0 已完成（2026-11-03 02:04，已提交）；A1 已完成（2026-11-03 06:34，已提交）；A2 已完成（2026-11-03 07:21，已提交）；A3 已完成（2026-11-03 08:28，已提交 `0bec0b3`）；A4 已完成（2026-11-03 08:47，已提交 `0bec0b3`）；A5 已完成（2026-11-03 15:15，已提交 `1c99b45`；设计稿 `.designs/agent-providers/README.md` 08:49 提交确认，用户回复“Approve, build it”，第 7 节按本稿默认）；A6 已完成（2026-11-03 18:20，未提交；`docs/requirements.md` 待用户确认）。2026-11-03 用户接受第 6 节全部推荐答案（“Yes, do A1”），A2 起可按推荐实施。
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
- 验证：`cargo test --manifest-path src-tauri/Cargo.toml --lib agent::registry`，再跑 `--lib storage::`（`cargo test` 只接受一个过滤词，原写法会报用法错误）。

完成记录（2026-11-03 02:04）：

- `storage.rs`：`global_config_dir(app)` 与新增 `system_config_dir()` 共用 `config_dir_in`（`LOOM_PROFILE_DIR` 分支不变）；无窗口路径由 `system_app_data_dir()` 按 `tauri.conf.json` 的 `identifier` 拼 `dirs::data_dir()`。`system_config_dir()` 暂无调用方，用 `#[expect(dead_code)]` 标注，A1 接入后编译器会要求删除。
- `migrations.rs`：与计划的差异——`read_versioned_json` 遇到旧格式（裸数组或无 `schemaVersion`）会就地升级回写，只读读取做不到“不回写”。拆出 `decode_versioned_json`，新增 `read_versioned_json_without_upgrade`（只在内存中升级），原函数行为不变。
- `agent/registry.rs`：新增 `read_agents_at(path)`；`load_agents_inner` 改为 `read_agents_at` + `save_agents`，行为与原来一致（旧格式升级仍由随后的 `save_agents` 落盘）。删除已无调用方的 `load_agent_store`。
- 测试：`storage::tests::system_app_data_dir_matches_tauri_resolution`（mock app 设成本仓库 identifier，比对 `app_data_dir()`；不调用会创建目录的 `system_config_dir()`，以免测试写真实数据目录）；`agent::registry::tests::read_agents_at_returns_defaults_without_creating_the_store`、`read_agents_at_merges_defaults_without_rewriting_a_legacy_store`（字节、mtime、目录内文件数均不变）。
- 证据：layer check、`cargo fmt --check`、`cargo clippy --all-targets --all-features -D warnings` 通过；`--lib` 下 registry 4 项、storage 6 项、migrations 4 项通过。全量 `cargo test` 两次各有 1 项 `tool_process::tests` 失败（不同用例），基线 `d1f700b` 全量两次中也有一次失败于 `tool_process::tests::deadlines_and_stderr_flood_are_bounded`，属既有负载下不稳定，与本次改动无关；单独重复运行均通过。
- 已知限制：存储文件损坏或版本过新时，`incompatible_store_error` 仍会在旁边复制一份备份，只读读取也会触发这一写入；A1 实现 CLI 时再决定是否需要完全无写入的报错路径。

### A1 Provider 配置与密钥

- 先把私有文件写入（`private_directory`、0600 原子写）从 `skills::files` 移到共享的 `storage`，`skills::files` 改为调用它，行为不变。原因：`agent/` 不能引用 `skills`（分层门禁），而 `storage` 已经是 `agent/registry.rs` 的依赖。
- 新 `agent/providers.rs`（只经 `agent/mod.rs` 再导出）：版本化 `providers.json`，用上面的私有原子写；跨进程写用 `std::fs::File::try_lock`（同 `chat/repository.rs:285`、`skills/coordination.rs:174`）+ `revision` 乐观校验（照 Skills 翻译的 `expectedRevision`）。
- 密钥规则照 Skills 翻译：`KeyChange` keep/set/clear；`KeySource` stored/environment/none；IPC 与 CLI 输出只给“有无 key、来源”；输入结构不实现 `Debug`/`Serialize`；CLI 只从 stdin 读 key，不收命令行参数里的 key（防止进 shell 历史和 `ps`）。
- `base_url` 校验：必须 https；http 只允许 loopback / 私网 / `.local`（覆盖 magpie 网关、Ollama、LM Studio）；这类地址允许无 key。
- Tauri 命令：`provider_list` / `provider_save` / `provider_delete` / `provider_test`（`test` 只发一次最小请求，错误按分类返回，不回显上游正文），同时登记到 `contracts.rs` 的 `COMMANDS` 与 `src/api/contract.ts`。
- 测试：权限位、revision 冲突、key 不出现在任何序列化输出、URL 校验正反例。

完成记录（2026-11-03 06:34）：

- 私有文件：新增 `storage/private.rs`（`resource_path`、`private_directory`、`regular_file`、0600 `atomic_bytes`、`sync_directory`，拒绝错误用 `Refusal` 标记）。`skills::files` 的同名函数改为包装它，`Refusal` 仍映射为 `InvalidInput`、系统错误映射不变；`storage::config_dir_in` 不再引用 `skills`。
- `agent/providers.rs`：`providers.json`（`schemaVersion`/`revision`/`providers[]`，每项 `id`、`name`、`kind`、`baseUrl`、`model`、`apiKey`、`keyEnv`），放在 `global_config_dir` 下，与 `agents.json` 同目录。写入持 `providers.lock`（0600，`File::try_lock`）完成“读取 → revision 校验 → 原子写”；读取不加锁。`list_at`/`save_at`/`delete_at`/`resolve_at` 以配置目录为参数，A4 的 CLI 可直接用 `system_config_dir()`。
- 密钥：`KeyChange` keep/set/clear；`KeySource` environment/stored/none。与第 3 节的“存储的 key，或环境变量名”二选一不同：两者可以同时配置，`keyEnv` 指向的变量有值时优先，否则回退到存储的 key（与 Skills 翻译的优先级一致）。`ProviderInput`、`KeyChange`、`ResolvedProvider` 不实现 `Debug`/`Serialize`；`Store` 的 `Debug` 只输出版本与 revision；存储解析失败只报“malformed”，不带 serde 原文。
- `baseUrl`：https；http 只允许 `localhost`/`*.localhost`/`*.local`、IPv4 回环/私网/链路本地、IPv6 回环/ULA/链路本地；拒绝凭据、query、fragment；保存时去掉末尾 `/`。本地地址无 key 也算 `configured`。约定：Anthropic 的 Base URL 不含 `/v1`，OpenAI 两种协议含 `/v1`（与各 CLI 的环境变量约定一致，A2 注入时直接使用）。
- `provider_test`：与计划“最小请求”的具体做法——发 `GET {base}/v1/models`（Anthropic，`x-api-key` + `anthropic-version`）或 `GET {base}/models`（OpenAI，Bearer），不消耗 token、不需要模型名；因此不验证所选模型是否存在。不跟随重定向，连接 10s、总 20s，响应上限 1 MiB；本地地址绕过 shell 代理。结果只有分类（ok / key_missing / auth_failed / not_found / rate_limited / http_error / invalid_response / timeout / unreachable）和 HTTP 状态码。
- 契约：`provider_list`/`provider_save`/`provider_delete`/`provider_test` 登记到 `lib.rs`、`contracts.rs` `COMMANDS`、`contracts/tauri-contract.json`、`src/api/contract.ts`；新增 `agentProviderList` 线上样例（Rust 往返 + 前端深比较）；`src/domain/agent.ts` 增加 `ModelProvider*` 类型，`src/api/agents.ts` 增加 `providersApi`（A5 的 Settings 使用，本轮无 UI）。
- 证据：`agent::providers` 9 项（权限位 0600、key 不出现在列表 JSON/`Debug`/错误、环境变量优先与回退、revision 冲突、持锁时写入被拒、非法输入不落盘、URL 正反例、本地 HTTP 假服务验证两种协议的请求头与失败分类）；`pnpm check` 通过（layer、Prettier、release docs、前端 150/150、build、fmt、clippy、Rust 395 passed / 35 ignored）。
- 未做：CLI 从 stdin 读 key 属于 A4；Settings UI 属于 A5；provider 被 Agent 引用时的删除保护等 A2 加 `provider_id` 后再补。

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

完成记录（2026-11-03 07:21）：

- 真实核对（本机假服务 `127.0.0.1:18731`，只记录 key 是否匹配和认证方式，不记录值）：

| CLI（版本） | 实际注入 | 支持的 `kind` |
|---|---|---|
| Claude Code 2.1.317 | 环境 `ANTHROPIC_BASE_URL`、`ANTHROPIC_API_KEY`（密钥环境）；`ANTHROPIC_DEFAULT_{OPUS,SONNET,HAIKU}_MODEL`、`ANTHROPIC_SMALL_FAST_MODEL` 设为模型；`ANTHROPIC_AUTH_TOKEN`、`ANTHROPIC_CUSTOM_HEADERS`、`CLAUDE_CODE_USE_{BEDROCK,VERTEX,FOUNDRY}` 显式设为空串；`--model` | `anthropic` |
| Codex 0.166.0 | `exec` 后插入 `--config model_provider="loom"`、`model_providers.loom.{name,base_url,wire_api="responses",env_key="LOOM_PROVIDER_API_KEY"}`；密钥环境 `LOOM_PROVIDER_API_KEY`；`--model` | `openai-responses` |
| OpenCode 2.1.2 | adapter 已固定 `run --standalone`；在 adapter 的 `OPENCODE_CONFIG_CONTENT` 中加入 `provider.loom`（`@ai-sdk/anthropic` 用 `{base}/v1`，`@ai-sdk/openai`、`@ai-sdk/openai-compatible` 用原 Base URL；`apiKey` 为 `{env:LOOM_PROVIDER_API_KEY}`）；密钥环境 `LOOM_PROVIDER_API_KEY`；`--model loom/<model>` | 三种 |
| Grok / 自定义 CLI | 拒绝：`agent '…' cannot use a … model provider` | 无 |

- 与上表“待核对”版本的差异：Codex 二进制含 “`wire_api = \"chat\"` is no longer supported”，只支持 Responses；Claude 用户 `settings.json` 的 `env` 只填补未设置的变量，实测它的 `ANTHROPIC_AUTH_TOKEN` 会作为额外 Bearer 发给 provider，所以注入时显式置空而不是仅移除；OpenCode 2.x 默认连共享后台服务，进程环境不生效，必须 `--standalone`。没有 key 的本地 provider：Claude 用占位 key `loom-no-key`，避免回退到 CLI 自己的登录并把它发给 provider；Codex/OpenCode 不配 `env_key`/`apiKey`。
- 代码：`AgentConfig`/`AgentConfigInput` 增加可选 `providerId`（`serde(default)`，为空时不序列化）；`agent/injection.rs` 生成各 adapter 的注入；`prepare_invocation` 在 `providerId` 有值时用 `system_config_dir()` + `providers::resolve_at` 解析（同时去掉 A0 的 `dead_code` 预期），先检查 `kind`，模型取“本次请求的模型 → provider 的模型”，都没有则报错。`ResolvedProvider` 增加 `model`；`ProviderKind::label`。`PreparedAgentInvocation` 增加 `environment_remove_prefixes`（序列化时为空省略）和 `secret_environment`（`serde(skip)`，`Debug` 只显示变量名）；runner 先按前缀 `env_remove` 继承变量，再应用 `environment` 与 `secret_environment`。`AgentCapabilities.providerKinds` 声明支持的 `kind`。Chat 续聊指纹在 `providerId` 有值时加入，换 provider 需开新会话，无 provider 的旧句柄不变。`create_agent`/`update_agent` 校验 provider 存在且 `kind` 受支持；`provider_delete` 拒绝删除仍被 Agent 引用的 provider。前端 `AgentConfig.providerId?`、`AgentCapabilities.providerKinds?`，`draftFromAgent` 保留 `providerId`，`agentConfig` 契约样例增加 `providerId`。
- 与本节原文的偏差：未升级 agents schema 版本——新字段带默认值，旧文件读出为 `None`、行为不变，无需迁移；`environment_remove` 实现为前缀列表；不移除 `CODEX_*`（会丢掉用户的 `CODEX_HOME`），Codex 只移除 `OPENAI_*`，OpenCode 不移除。`check_startable` 未加 provider 检查：`prepare_invocation` 已在启动前给出明确错误。
- 顺带修复：A1 的 `providers.lock` 在并行测试中偶发“busy”——其他测试 fork 出的子进程在 exec 前短暂继承锁描述符。写入者现在最多重试 500ms 再报忙。
- 证据：`provider_tests` 7 项（三种 CLI 注入快照；key 不出现在 args、公开环境、`Debug` 和序列化结果；运行模型覆盖 provider 模型；不支持组合与缺模型的错误；`providerId` 往返与缺省；runner 前缀移除与两种环境生效），另加删除保护、`kind` 校验、续聊指纹各 1 项；`pnpm check` 通过（前端 150/150、Rust 405 passed / 35 ignored、clippy `-D warnings`）。
- 未验证：没有通过 Loom 端到端跑一次真实 provider 对话（属 A7）；OpenCode 现有 adapter 传的 `--dir` 在 2.1.2 上报 “Unrecognized flag”，这是 A2 之前就有的问题。A2 提交后单独修复：2.1.2 还会忽略 `OPENCODE_PERMISSION`、从 `PWD` 取项目目录，adapter 现在固定 `--standalone`、设置 `PWD`、权限写入 `OPENCODE_CONFIG_CONTENT`，provider 注入合并进同一份配置（见 `docs/agent-adapter.md`）。
- 未做：Settings 里选择 provider 属于 A5；CLI 的 `--provider` 属于 A4。

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

完成记录（2026-11-03 08:28）：

- 代码：新增 `agent/api/{mod,sse,anthropic,openai_responses,openai_chat}.rs` 与测试 `agent/api/tests.rs`。请求只有 system（`system_append`）+ 一条用户 prompt：Anthropic `POST {base}/v1/messages`（`x-api-key`、`anthropic-version: 2023-06-01`、`max_tokens` 8192）；Responses `POST {base}/responses`（`instructions`、`input`、`store: false`）；Chat `POST {base}/chat/completions`。key 只在 `sensitive` 请求头里；不跟随重定向；本地/内网地址不走 shell 代理（与连接测试共用 `providers::http_client`）。SSE 解码器容忍任意分块边界与 `\r\n`，单行受 `max_line_bytes` 限制；总字节 `max_bytes`、事件数 `max_lines`。
- 运行：`PreparedAgentInvocation` 增加 `api: Option<ApiCall>`（`serde(skip)`，`Debug` 隐藏 key 与 prompt 正文），`agent::run` 见到它就交给 API runner。runner 先取 `begin_operation` 守卫；发送与读流都在 `select!` 里和 25ms 间隔竞争，每次检查 `cancelled()`、`is_shutting_down()` 和总超时，停止时丢弃请求，用 `stop_outcome` 返回 `chat_abort` / `chat_timeout` / `app_shutdown` 并保留已收到的正文。不注册进程、不改 `process_supervisor`。
- 结果分类（`status = error`，`termination_reason` 为下列代码，不读取非 2xx 响应体）：`api_bad_request`(400/422)、`api_auth_failed`(401/403)、`api_not_found`(404)、`api_request_too_large`(413)、`api_rate_limited`(429)、`api_server_error`(5xx)、`api_http_error`、`api_unreachable`、`api_stream_interrupted`、`api_invalid_stream`、`api_incomplete_stream`（没有收到结束事件）、`api_provider_error`（流内错误，只保留形如 `overloaded_error` 的短代码）；超出预算为 `chat_runtime_error` + `output_limit:`，与进程 runner 一致。模型因输出上限停止时 `status = complete`，另加 `api_output_truncated` 错误片段。成功结束和 provider 失败都会发 `AgentEvent::Done`，运行时错误不发，与进程 runner 一致。
- 配置与检查：`ADAPTER_API = "api"`。`validate_input` 对它只要求名称和 `providerId`；保存时清空 `command`/`args` 并强制 `canWriteFiles`/`canRunCommands = false`。`available`、`check_startable`、`diagnose_agents` 只检查 provider 存在且有 key（本地/内网地址可无 key），不发网络请求、不缓存。能力声明：无 tools / 原生续聊，支持模型选择，`providerKinds` 为三种，只有 `explore` 模式。
- 功能声明：`AdapterInvocationRequest` 增加 `needs_tools`。Chat、Learn、Research 和测试一律为 `true`；Discuss 为 `有项目 && 不是汇总轮`。`needs_tools`、写权限、`resume_command` 或执行档案任一出现时，`api` 执行器报 “… has no tools; this feature needs a local CLI agent” 等明确错误。Discuss 现在接受 `api` 参与者，但讨论带项目时拒绝它们。前端 `AgentAdapterType` 增加 `"api"`。
- 与本节原文的偏差：没有把 `RunSpec.prepared` 改成 `Invocation::{Process, Api}` 枚举，而是在 `PreparedAgentInvocation` 上加可选 `api`，这样 10 处功能调用和它们写执行记录用的 `program`/`args` 不用改——API 运行的 `program` 为 `api`、`args` 为 `POST <endpoint> --model <model>`，只含 Base URL。“任何讨论的主持汇总轮”在请求层已声明 `needs_tools = false`，但 Discuss 的主持人就是第一位参与者，还要答前两轮，所以带项目的讨论目前不能用 `api` 参与者；单独选主持人留到以后。“有界历史回放”没有做：首批开放的 Discuss 与 `loom-agent run` 都是单轮，Chat 仍要求 CLI。
- 证据：`agent::api` 12 项测试（本地假 SSE 服务：三种协议的路径、认证头、请求体与流式文本；本地网关无 key 不发认证头；需要工具/写权限/续聊被拒；停止保留部分正文且 2 秒内返回；总超时覆盖不应答的服务；模型截断与输出预算；401/404/429/503、流内错误、半截流、非 JSON 流和连不上的分类且不含上游正文；SSE 任意切分），另加 registry 1 项；`pnpm check` 通过（前端 150/150、Rust 418 passed / 35 ignored、clippy `-D warnings`）。
- 未验证：没有调用真实模型 provider（需要 key，并花费额度；属 A7）；协议事件形状按各家公开流式格式编写，未与真实响应逐字比对。

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


完成记录（2026-11-03 08:47）：

- 代码：新增 `src-tauri/src/agent_cli.rs`（测试 `agent_cli_tests.rs`）与 `src/bin/loom-agent.rs`，`Cargo.toml` 增加 `[[bin]] loom-agent`；可测核心是 `run_with(args, config_dir, Io, Interrupt)`，二进制只解析 `storage::system_config_dir()`（尊重 `LOOM_PROFILE_DIR`）、接真实 stdio 和 Ctrl-C。
- agent 层出口：`agent/mod.rs` 再导出 `read_agents_at`、新增的 `agents_file(dir)`、provider 的 `list/save/delete/resolve_at`、`ensure_unused`、`test_provider`、新增的 `diagnose(agent, dir)`（`api` 执行器从给定目录读 provider，`provider_ready_at`）、`prepare_invocation_with`、`resume_command`、`permission_to_stage`、`embeds_prompt`。Chat 的 `permission_to_stage` 改为委托 agent 层（ask/auto → `Debugging`，与原来一致）；Chat 与 Discuss 的 “prompt 走 argv 还是 stdin” 改用 `agent::embeds_prompt`。
- 子命令：`list`、`doctor [AGENT]`、`provider list|add|set-key|remove|test`、`run`，均有 `--json`。`run` 的请求与 Chat 一回合相同（`chat_permission_mode = mode`、stage 由 `permission_to_stage`、`embed_prompt` 由 `embeds_prompt`），只是 `needs_tools = false`、不包 Chat 的提示词外壳、不加内联卡片；`--provider` 与 `--agent` 同给时替换该 Agent 的 provider（模式 B），只给 `--provider` 时合成一个 `api` Agent（模式 C）。provider 从 CLI 自己的配置目录解析后交给 `prepare_invocation_with`；非 `api` Agent 先过 `check_startable`。读取 `agents.json` 后按本机 `PATH` 重算 `available`。
- 输出与退出码：人读模式正文到 stdout，工具/错误片段、状态和续聊 id 到 stderr；`--json` 为 NDJSON `started`（模式 A/B 带 `pid`；模式 C 在请求前发出，`pid: null`）、`text`、`part`（现有 `MessagePart` 序列化）、`log`（仅 `--verbose`，已脱敏）、`done`（`status`/`exitCode`/`errorSummary`/`terminationReason`/`resumeSessionId`）。退出码 0 完成、1 失败、2 用法、124 超时、130 中断。第一次 Ctrl-C 置取消标志（runner 走正常停止），第二次对本进程的运行 `force_stop`。
- 密钥与提示词：`provider set-key` 只从管道/文件形式的 stdin 读 key（≤16 KiB），stdin 是终端时拒绝并提示用管道；没有任何 `--key` 参数。prompt 来自 `--prompt`、`--prompt-file`（≤1 MiB）或非终端 stdin。
- 执行记录：每次 `run` 写 `<config>/cli/runs/<runId>.json`（目录 0700、文件 0600，原子写）：Agent / provider id、适配器、模式、model、program、cwd、是否续聊、prompt 字数、起止时间、结束状态、退出码、终止原因、错误摘要、最后 40 行 stderr（脱敏）。args 中含 prompt 的项记为 `<prompt>`、超过 256 字符的记为 `<N chars>`，其余经 `redact_sensitive_text`；不存 prompt 正文、环境变量和 key。runId 为 `cli-<ms>-<pid>-<序号>`。
- 门禁：`scripts/check-layers.mjs` 先加一条——`agent_cli.rs` 生产代码不得引用功能模块（A6 其余规则未做）。
- 与本节原文的偏差：`provider add` 只新建，修改已有 provider 的名称/URL/模型需在 Settings 或删除后重建；超时的 `errorSummary` 由 CLI 改写为实际秒数，因为 runner 的 `timeout_summary()` 固定按 10 分钟描述；“三种模式的 NDJSON 形状”中模式 B 没有单独的 CLI 测试（CLI 只把 provider 交给 A2 已测的注入，输出路径与模式 A 相同）。
- 证据：`agent_cli` 11 项测试（参数表与用法错误；explore/ask/auto 下 CLI 与 Chat 式请求生成的调用逐字段一致；只读 Agent 拒绝 `auto`；桩 CLI 的 NDJSON 含 pid、正文、stderr 日志与 `done`，执行记录 args 为 `<prompt>` 且不含 prompt；人读模式 stdout 只有正文；终端 stdin 不读 key/prompt；假 Chat Completions 服务上的模式 C：认证头带 key、模型与 prompt 在请求体、输出/错误流/记录/provider 列表都不含 key；被 Agent 引用的 provider 不能删；退出码映射；记录参数脱敏）。同一进程内连续运行曾因同毫秒 runId 冲突失败一次，已加序号修复并重复 6 次通过。`pnpm check` 通过（前端 150/150、Rust 429 passed / 35 ignored、clippy `-D warnings`、层检查）。二进制冒烟（临时 `LOOM_PROFILE_DIR`）：`list`/`doctor` 识别本机 codex-cli 0.166.0、Claude Code 2.1.317、opencode 2.1.2、grok 1.0.70；睡眠桩 Agent 上 SIGINT 后退出 130、`done.terminationReason = chat_abort`、无残留进程；`--timeout-secs 1` 退出 124；记录文件 0600。
- 未验证：没有用真实 CLI 或真实 provider 跑提示词（花费额度，属 A7）；没有执行 `tauri build` 确认 `.app` 内含 `loom-agent`（`docs/skills-cli.md` 写明 Tauri 打包全部 `[[bin]]`，本次未复核）。

### A5 桌面 Settings 与选择器

- 先在 `.designs/` 出设计稿：Settings → Providers 列表与编辑（预设：Anthropic、OpenAI、OpenRouter、OpenAI 兼容、Anthropic 兼容、本机网关），Agent 编辑里增加“模型来源”下拉。
- `AgentPicker` 显示“执行器 · 模型来源”（如 `Codex · DeepSeek`、`API · Anthropic`）；`needs_tools` 的功能里 `api` 项置灰并说明原因。
- 视觉矩阵补 Providers 页（中英文 × 深浅 × 700px）。
- 依赖：A1–A3；设计稿经用户确认后再写代码。

完成记录（2026-11-03 15:15，已提交 `1c99b45`）：

- 设计：按 `.designs/agent-providers/README.md` 实施，第 7 节三项取本稿默认（独立页签；自带登录只显示执行器名；预设即表中六项）。设计稿状态改为“已确认并实现”。
- 状态与接口：`AppState.providers`（`ModelProviderList`，只含 key 来源，不含 key）与 `providers/loaded`；`useAgentCatalog.loadAgents` 顺带拉 `provider_list`（失败不影响 Agent 加载），新增 `loadProviders`/`saveProvider`/`deleteProvider`/`testProvider`，provider 调用直接抛错交给页面显示。
- Settings → 模型来源（`ProvidersPanel.tsx`、`providerDraft.ts`）：列表显示名称、协议 · 地址、默认模型、key 来源（已保存 / `$变量` / 本机无需 / 缺少 key 或“未设置 $变量”）与被引用的 Agent；“测试”结果就地显示，只映射 `outcome` 与 HTTP 状态；被引用时删除禁用并说明。添加表单含六个预设、协议单选、Base URL、默认模型、`type=password` 的 key（不回填，编辑时默认保留，“替换”才出现输入框，“清除”需确认）、环境变量；保存/删除后关闭表单丢弃 key，并刷新 Agents 与诊断；revision 冲突时重新加载并显示横幅，表单输入保留。新 provider 的 id 由名称生成（ASCII slug，重名加序号，非 ASCII 名称为 `provider`）。
- Settings → Agents：新增“模型来源”列；直连 API 的路径列显示 provider 主机（悬停为完整 URL）、能力列“仅文本回答”、版本位“直连 API”；页头加“+ 直连 API”，表单只有名称和必选 provider（默认名 `API · <provider>`，改 provider 时若名称未改则跟随）；CLI Agent 表单加模型来源下拉，兼容性取诊断里的 `providerKinds`（未诊断时列出全部，由后端校验），不兼容项放在“不兼容”分组并禁用，自定义 CLI 与不支持注入的执行器整个下拉禁用，旁边“管理模型来源”跳到页签。
- `AgentPicker`：新增 `providers`、`needsTools`；按钮为“执行器 · 来源”（自带登录与直连 API 只显示名称）；菜单第二行写来源与能力，provider 缺 key 时为 `down` 并写“模型来源 X 缺少 key”，`needsTools` 时 `api` 项禁用并说明原因；provider 有默认模型时占位符为“默认：<model>”；`defaultAgentId` 不再选 `api`；导出 `pickerAgents`、`agentProvider`、`agentButtonLabel`。Chat、Learn、Research 传 `needsTools`（Learn/Research 通过 `pickerAgents` 把 `api` Agent 列出但置灰），Discuss 的 `DISCUSS_ADAPTERS` 加入 `api`、`needsTools` 取“是否选了项目”，选了项目又选了 `api` 参与者时不能提交并提示；各功能的提交校验同样排除缺 key 的 provider。
- 窄窗口：700px 时 Providers 表格变卡片（名称、协议与地址、key 来源与操作），表单单列，预设换行；只用现有 `--chat-*` 令牌。
- 证据：新增 `tests/unit/agentProviders.test.cjs`（jsdom：菜单在 `needsTools` 下禁用 `api` 并给原因、缺 key 的 CLI 禁用；Providers 列表只显示 key 来源、被引用时删除禁用及提示、编辑默认不出现 key 框、“替换”后 key 框为空；draft → input：空 key 不发送、去空白、id 生成、预设表）与 `agentPicker.test.cjs` 三项（按钮标签、默认不选 `api` 与默认模型占位符、缺 key 为 `down`）。`pnpm check` 通过（前端 156/156、Rust 429 passed / 35 ignored、clippy `-D warnings`、层检查）。`pnpm smoke:visual` 通过：活动栏矩阵新增 Providers 列表与添加表单截图（en / zh-CN × light / dark × 700px，`.designs/exports/agent-providers/`），断言卡片布局、无横向滚动、列表页没有 password 输入、添加表单的 key 框为空；首次运行时预览服务被外部 SIGTERM（Vite 退出 143），重跑通过。
- 未验证：没有在桌面 Tauri 窗口里真实保存/测试/删除 provider（预览模式无后端，只用夹具渲染）；没有用真实 provider 发请求（A7）。

### A6 门禁与文档

- `scripts/check-layers.mjs`：`agent_cli` 只用 `agent/mod.rs` 再导出及 `storage` / `projects` 公共函数；功能模块不引用 `agent_cli`、不直接用 `reqwest` 调模型；`tests/unit/layerCheck.test.cjs` 加正反例。
- 文档：新增 `docs/agent-cli.md`（用法、NDJSON、退出码、三种模式）；更新 `docs/architecture/agent-layer.md`（执行器 / 模型来源、两种 runner）、`docs/agent-adapter.md`（各 CLI 注入方式与核对版本）、`docs/security-policy.md`（provider key 存储、环境清理、URL 规则）、`docs/architecture.md`、`docs/testing.md` 验证矩阵、`docs/requirements.md`（用户确认后加入 API Key 模式范围）。


完成记录（2026-11-03 18:20，未提交）：

- 门禁：`scripts/check-layers.mjs` 新增三条——`agent_cli.rs` 只引用 `agent` / `models` / `storage` / `projects`（模块名从 `lib.rs` 读取，新模块默认禁止），且不用 Agent 私有子模块；`src/bin/loom-agent.rs` 只调用 `agent_cli`；其它模块都不引用 `agent_cli`。功能模块（chat、chat_context、learn、research、discuss、experiments、skills、workbench）生产代码不得出现 `reqwest`，具名例外 `HTTP_ALLOWLIST`：`learn/github.rs`（另一会话实施中的 Learn 发现）、`skills/github_api.rs`、`skills/translation/remote.rs`。私有子模块判定抽成 `agentPrivatePaths`（加入 `providers`、`api`），原有功能规则改用它。`scripts/lib/rust-layer-rules.mjs` 新增 `usesHttpClient`、`agentPrivatePaths`；`tests/unit/layerCheck.test.cjs` 加正反例（别名、`::reqwest`、字符串与注释、标识符包含 reqwest、`agent_cli` 分组引用）。
- 文档：新增 `docs/agent-cli.md`（入口、命令、`run` 参数、三种模式、NDJSON、退出码、执行记录、分层）；`docs/architecture/agent-layer.md` 加“执行器 × 模型来源”、`loom-agent` 调用方行与新门禁；`docs/agent-adapter.md` 加“模型来源注入（模式 B）”表并修正“Loom 不管理凭据”的旧表述；`docs/security-policy.md` 加“模型来源与 API Key”；`docs/architecture.md` 模块表加 providers/injection、`agent/api/`、`agent_cli`、Settings 模型来源；`docs/testing.md` 验证矩阵加 Agent 调用层一行。
- 与本节原文的偏差：`docs/requirements.md` 未改——按原文需用户确认后再加入 API Key 模式范围；HTTP 规则不能区分“调模型”与其它 HTTP，所以按“功能模块不直接用 reqwest + 具名白名单”实现；`skills/translation/remote.rs` 确实直接调模型，早于 Agent 层，本次只登记为例外，没有迁移。
- 证据：层检查通过；临时探针文件（learn 内用 `reqwest::Client`、chat 内引用 `crate::agent_cli` 与 `agent::providers::`）三条违规均被报告，探针已删除；`layerCheck` 4/4 通过。全量 `pnpm check` 结果见本轮回复（工作区同时含另一会话未提交的 Learn 发现改动）。
- 未验证：A7 全部内容；文档中模式 B 注入表按代码与 CLI 参数核对，未用真实 provider 对话验证。

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
