# Agent 命令行（`loom-agent`）

状态：A4 已实现并通过合成测试（参数表、桩 CLI、假 SSE 服务）；尚未用真实 CLI 或真实 provider 跑提示词，也未复核 `.app` 是否随包带有 `loom-agent`（A7）。计划见 [Agent 调用层计划](plans/2026-11-02/21:54-loom-agent-cli.md)。

`loom-agent` 是 `crate::agent` 的无窗口入口：读取与桌面相同的 `agents.json` / `providers.json`，按与 Chat 一回合相同的方式准备调用，再经 `agent::run` 执行。它不启动 Tauri，也不经 IPC。

## 入口

```bash
cargo build --manifest-path src-tauri/Cargo.toml --bin loom-agent
src-tauri/target/debug/loom-agent --help
```

配置目录与桌面相同（`storage::system_config_dir()`）；设 `LOOM_PROFILE_DIR=/absolute/dir` 可隔离到临时 profile，验收时应这样做。`loom-agent` 是 `src-tauri/Cargo.toml` 的第二个 `[[bin]]`，入口 `src/bin/loom-agent.rs` 只调用 `agent_cli::main`。

## 命令

| 命令 | 作用 |
|---|---|
| `list [--json]` | 已配置的 Agent：id、名称、执行器、模型来源（`own login` 或 provider id）、启用/安装状态；`available` 按本机 `PATH` 重算 |
| `doctor [AGENT] [--json]` | 与 Settings 相同的诊断；`api` Agent 只检查 provider 存在且有 key（本机/内网网关可无 key），不发请求 |
| `provider list [--json]` | provider 列表，只显示 key 是否存在及来源（stored / environment / local），从不显示 key |
| `provider add ID --kind KIND --base-url URL [--name NAME] [--model MODEL] [--key-env VAR]` | 新建 provider；KIND 为 `anthropic`、`openai-responses`、`openai-chat`。只新建，修改已有项用 Settings 或删除后重建 |
| `provider set-key ID [--clear]` | 从管道或文件形式的 stdin 读 key（≤16 KiB）并保存；`--clear` 删除已保存的 key。stdin 是终端时拒绝 |
| `provider remove ID` | 删除未被任何 Agent 引用的 provider |
| `provider test ID` | 一次最小请求（列模型，不消耗 token）；只报告分类结果与 HTTP 状态 |
| `run [options]` | 跑一个提示词 |

没有 `--key` 参数；key 只能经 stdin 进入，避免出现在 argv、shell 历史和 `ps` 中：

```bash
pbpaste | loom-agent provider set-key deepseek
```

## `run`

| 参数 | 说明 |
|---|---|
| `--agent ID` | Settings 中的 Agent；用它自己的登录，或它配置的 provider |
| `--provider ID` | 与 `--agent` 同给时替换该 Agent 的 provider（模式 B）；只给 `--provider` 时合成一个 `api` Agent 直连模型（模式 C） |
| `--project DIR` | 工作目录，经 `projects::canonical_project_path` 规范化；默认当前目录 |
| `--mode explore\|ask\|auto` | 默认 `explore`（只读）；只读 Agent 拒绝 `auto` |
| `--model NAME` | 覆盖模型 |
| `--resume SESSION` | 续接 CLI 原生会话（`api` Agent 不支持） |
| `--prompt TEXT` / `--prompt-file FILE` / stdin | 三选一，≤1 MiB；stdin 为终端时不读 |
| `--timeout-secs N` | 1–1200，默认 600 |
| `--json` | NDJSON 事件流 |
| `--verbose` | 同时输出脱敏后的进程日志行 |

三种模式：

| 模式 | 写法 | 执行 |
|---|---|---|
| A 自带登录 | `--agent codex` | 本机 CLI 使用自身账户 |
| B CLI + provider | `--agent claude --provider my-anthropic`，或 Agent 已配置 provider | 本机 CLI，按单次调用注入 Base URL / key / 模型，见 [适配器](agent-adapter.md#模型来源注入模式-b) |
| C 直连 API | `--provider deepseek` | 无工具、无文件访问、无原生续聊的流式请求 |

`run` 与 Chat 一回合生成相同的调用（`chat_permission_mode = mode`、stage 由 `permission_to_stage` 给出、提示词走 stdin 还是 argv 由 `embeds_prompt` 决定），区别只是 `needs_tools = false`、不包 Chat 的提示词外壳、不加内联卡片。非 `api` Agent 先过 `check_startable`。

## 输出

默认人读：正文到 stdout；工具与错误片段、状态和续聊 id 到 stderr。脚本可以只读 stdout。

`--json` 每行一个对象：

| `type` | 字段 |
|---|---|
| `started` | `runId`、`agentId`、`adapterType`、`providerId`、`pid`（模式 C 在请求前发出，`pid: null`） |
| `text` | `text`：正文增量 |
| `part` | `part`：工具调用 / 结果等 `MessagePart`，与 Chat 同一序列化 |
| `log` | 仅 `--verbose`：`stream`（`stdout` / `stderr`）、`line`，已脱敏 |
| `done` | `runId`、`status`、`exitCode`、`errorSummary`、`terminationReason`、`resumeSessionId` |

退出码：`0` 完成，`1` 失败，`2` 用法错误，`124` 超时，`130` 中断。第一次 Ctrl-C 请求正常停止，第二次强制结束本进程启动的运行。

## 执行记录

每次 `run` 写 `<config>/cli/runs/<runId>.json`（目录 0700、文件 0600、原子写；runId 为 `cli-<ms>-<pid>-<序号>`）：Agent / provider id、适配器、模式、model、program、cwd、是否续聊、prompt 字数、起止时间、结束状态、退出码、终止原因、错误摘要、最后 40 行脱敏 stderr。args 中含提示词的项记为 `<prompt>`，超过 256 字符的记为 `<N chars>`。不存提示词正文、进程环境和 key；首版不自动清理。

## 分层

`agent_cli.rs` 只用 `crate::agent` 的再导出和 `models` / `storage` / `projects`，不引用功能模块或 Agent 私有子模块；除 `loom-agent` 二进制外没有模块引用 `agent_cli`。由 `scripts/check-layers.mjs` 检查，见 [Agent 层契约](architecture/agent-layer.md#分层验证)。
