# 本机 CLI 适配器

Loom 通过 Rust Agent 调用层 `src-tauri/src/agent/`（参数在 `invocation.rs`，能力在 `capabilities.rs`）生成可执行文件、参数、cwd、stdin 方式、输出协议和必要的单次环境配置。React 只传会话、消息、Agent ID 和模式，不拼接命令。

通用运行与生命周期契约见 [Agent 调用层](architecture/agent-layer.md)；本页说明 CLI 参数与协议。

## 探测与配置

`agent/registry.rs` 先查 `$PATH`，再查当前 `$HOME` 下的 `.local/bin`、`.npm-global/bin`、`.opencode/bin`、`.grok/bin`，以及 `/opt/homebrew/bin`、`/usr/local/bin`。检查可执行位，不使用 shell，不包含写死的用户名目录。

内建 Agent 为 Claude Code、Codex、OpenCode、Grok；Dummy 仅在 debug/test 构建出现。已保存的命令路径覆盖和启用状态不被重新探测覆盖。诊断直接执行 `--version`，两秒超时，取不到版本显示 `unknown`；能启动但 `--version` 非零退出（常见于安装不完整）标为 `broken`，Settings 与 Agent 选择器显示“安装已损坏”且不可选；“已安装”只代表找到可执行文件，登录/模型可用性需真实调用验证。

诊断返回 `capabilities { structuredStream, toolEvents, nativeResume, inlineCards, modelSelection, evidenceOnly?, modes }`。每个 mode 含适配器生成的 policy 文本；设置表格不自行拼写参数。单项测试同样仅执行版本探测。`evidenceOnly` 为新 Research 的独立无工具执行配置，当前只对受测 Claude 2.1.311／无额外 argv 声明；不是模型登录可用证明，也不改变原有 explore/ask/auto。详见 [Agent 层](architecture/agent-layer.md)。

## 启动前检查与错误格式

- `agent::check_startable` 在运行前确认 CLI 能启动：内建适配器执行 `<command> --version`（15 秒上限），自定义 CLI 只要求命令可解析（`--version` 不属于它们的约定）。Learn / Research 开始前、Chat 每次发送前、Discuss 每项调用前都会调用；Chat 在接受回合前拒绝，不留下失败回合。
- 通过的结果按 Agent id 缓存，键为解析后的可执行文件路径与修改时间，应用会话内有效；失败不缓存。当前 `load_agents` 会保存合并后的配置并清空缓存，不能保证跨每次功能请求复用缓存。保存 Agent 配置、Settings 重新检测，以及 Chat / Learn / Research / Discuss 运行失败时清除缓存。
- 面向用户的错误统一为“一行摘要 + 空行 + 原始详情”（`agent::AgentError`），IPC 仍传字符串。Agent 非零退出写作 `agent exited with code <n>`，详情为 stderr 最后 2,000 字符。前端共享的 `ErrorText`（`src/components/common/`）显示摘要并把详情收进折叠区，兼容旧会话中的 `agent exited with status …; stderr: …` 格式；Chat 的错误片段、回合详情和发送错误与 Learn 共用它。
- 例外：Skills 本地翻译（`skills/translation/local.rs`）仍自行启动 Codex / Claude。它需要原始 stdout（Claude 的单个 JSON 结果），而 `agent::run` 在解析前逐行脱敏，可能改写译文里形似密钥的示例；翻译也不走 Agent 配置，按命令名解析。它同样经 `process_supervisor` 登记进程组、限时和取消。

`inlineCards` 只对 Claude Code 为 true（实验）。设置 `experimentalInlineCards` 打开时，适配器每轮（含续聊）追加 `--append-system-prompt <提示词>`；提示词由 `pnpm openui:prompt` 生成并编译进二进制，执行记录中以 `[OPENUI_PROMPT sha256:…]` 代替原文。其他适配器忽略该字段，参数不变。

## 权限与续聊

| CLI | Read-only (`explore`) | Ask (`ask`) | Auto (`auto`) | 输出 |
|---|---|---|---|---|
| Claude Code | `--permission-mode plan` | `acceptEdits` | `auto` | `stream-json` + partial messages |
| Codex | `--sandbox read-only` | `workspace-write` | `workspace-write` | `exec --json` |
| Grok | `--permission-mode plan` | `acceptEdits` | `auto` | `streaming-messages-json` |
| OpenCode | `plan`，单次权限配置默认拒绝，仅允许读取/搜索/网页读取 | `build`，编辑可用，其余需确认（无窗口运行时被拒绝） | `build`，自动权限，禁止 external_directory | `run --standalone --format json` |
| 自定义 CLI | 执行自身策略，Loom 不提供额外沙箱 | 同左 | 同左 | 纯文本 |

Codex 每轮显式设置 `approval_policy="never"`、cwd 和 sandbox，包括 resume；支持无 Git 的项目目录。Claude/Grok 每轮重申权限。Code 的发送确认是 Loom 的回合授权，不是逐工具审批；CLI 非交互模式可能拒绝需要额外审批的工具。

Claude 使用 `--resume <id>`，Codex 使用 `exec … resume --json -- <id> -`，Grok 使用 `--resume <id>`。成功后保存绑定 Agent/命令/项目/权限/参数的原生句柄；换 Agent 或模式清除句柄，保留转录并在下一轮有界重放。OpenCode 使用 `run --session <id>`，从事件的 `sessionID` 捕获原生句柄；官方 1.19.3 临时副本已通过三轮原生续聊、仓储重开和停止后再发，因此能力中启用 nativeResume。

OpenCode 的参数与权限配置依据 [官方 CLI 文档](https://opencode.ai/docs/cli/) 和 [权限文档](https://opencode.ai/docs/permissions/)；本机旧安装无法启动；另用官方验签的 1.19.3 临时副本完成 runtime 验收。未配置模型来源时，模型调用使用 CLI 自身的账户配置；配置后按下文“模型来源注入”单次注入，Loom 只管理 `providers.json` 里的 key，不读取 CLI 的订阅凭据。

OpenCode 2.x（本机 2.1.2，2026-11-03 核对）与 1.19.3 不同：`run` 没有 `--dir`，项目目录取自 `PWD` 环境变量（只设进程 cwd 时会落到 Loom 继承的目录）；`OPENCODE_PERMISSION` 被忽略；默认连接共享后台服务，本进程的环境变量不生效。因此 adapter 固定传 `--standalone`，设置 `PWD` 为项目目录，并把单次权限写进 `OPENCODE_CONFIG_CONTENT` 的 `permission`（API Key 模式再在同一份配置里加 `provider.loom`）。2.x 工具名为 `read`/`write`/`edit`/`shell` 等，参数用 `path`；解析器已兼容。

自定义 CLI 使用专用可执行文件，拒绝 `sh/bash/zsh/fish/cmd/powershell/pwsh` 包装；参数按数组传递，支持 `{prompt}`、`{projectPath}` 等已有模板。不承诺自定义工具卡片或原生续聊。

## 模型来源注入（模式 B）

Agent 配置了 `providerId` 时，`agent/injection.rs` 在参数准备之后加入 provider，只作用于这一次调用：不写 `~/.claude`、`~/.codex`、OpenCode 全局配置，key 只进入子进程环境（`secret_environment`），不出现在 argv、执行记录或日志中。核对版本：Claude Code 2.1.317、Codex 0.166.0、OpenCode 2.1.2（2026-11-03）。

| CLI | 可接协议 | 注入方式 | 清理 |
|---|---|---|---|
| Claude Code | Anthropic | `ANTHROPIC_BASE_URL`、`ANTHROPIC_API_KEY`；`--model` 加 `ANTHROPIC_DEFAULT_{OPUS,SONNET,HAIKU}_MODEL`、`ANTHROPIC_SMALL_FAST_MODEL` 同为所选模型（后台调用用别名） | 移除继承的 `ANTHROPIC_*`、`CLAUDE_CODE_USE_*`；`ANTHROPIC_AUTH_TOKEN`、`ANTHROPIC_CUSTOM_HEADERS`、`CLAUDE_CODE_USE_{BEDROCK,VERTEX,FOUNDRY}` 设为空，使用户 settings 的 `env` 块不能补回；无 key 的本机网关传占位 key，避免回落到自身登录 |
| Codex | OpenAI Responses | `exec` 后插入 `--config model_provider="loom"` 与 `model_providers.loom.{name,base_url,wire_api="responses",env_key}`；key 在 `LOOM_PROVIDER_API_KEY` | 移除继承的 `OPENAI_*`。Codex 已删除 `wire_api="chat"`，不能接 Chat Completions |
| OpenCode | 三种 | 在单次 `OPENCODE_CONFIG_CONTENT` 中加 `provider.loom`（`@ai-sdk/anthropic` / `@ai-sdk/openai` / `@ai-sdk/openai-compatible`，`apiKey` 为 `{env:LOOM_PROVIDER_API_KEY}`），`--model loom/<model>`；与权限配置合并在同一份 JSON | 依赖 `--standalone`，不连接共享后台服务 |
| Grok / 自定义 CLI | 无 | 不支持；保存或准备调用时拒绝 | — |

模型取 Agent 的模型覆盖，否则取 provider 的默认模型；两者都没有时拒绝启动。直连 API（`adapterType = "api"`）不经过适配器参数，见 [Agent 层契约](architecture/agent-layer.md#执行器--模型来源)。上表为按 CLI 文档与本机参数核对的结果，真实 provider 对话尚未验证（计划 A7）。

## CLI 实测矩阵（2026-10-20）

| CLI / 版本 | 真实状态 | 已观察的协议 | 续聊 |
|---|---|---|---|
| Claude Code 2.1.304 | 问答、读文件、pwd、Research 禁写、Code 编辑已测 | `tool_use.id` 与 `tool_result.tool_use_id`；Read/Bash/Edit/Write 输入输出；部分正文 + assistant 最终消息 | 三轮与每轮磁盘仓储重开通过 |
| Codex 0.163.0 | 问答、读取命令、pwd、Research 禁写、Code 编辑已测 | `thread.started.thread_id`；`item.started/completed`，命令 item id / aggregated_output / exit_code；file_change | 三轮与每轮磁盘仓储重开通过 |
| Grok 1.0.62 | blocked：返回 402，使用额度耗尽；同时报告刷新登录异常 | 真实 system/result 错误样本已保存 | 本轮未通过，保留既有适配能力，不将历史结果计为本轮通过 |
| OpenCode 2.1.2（本机安装，脚本化本地模型） | 用 Loom 生成的三种模式调用核对：explore 只能读，write/shell 工具不可用；ask 可写、shell 在无窗口运行时被拒、写项目外被拒；auto 可写可执行、写项目外被拒；真实模型未测（本机默认模型的网关/MCP 未响应） | `step_start/tool_use/text/step_finish`；`part.id` 作为调用 id，`state.input.path`、`state.output`、`sessionID` | `--session` 在 `--standalone` 下续上同一会话并回放上一轮工具结果 |
| OpenCode 1.19.3（官方临时副本） | 读取、Research 禁写、Code 编辑、停止后再发通过；本机原安装仍因无效代码签名 blocked | `step_start/tool_use/text/step_finish`；`part.callID`、`state.input.filePath`、`state.output`、`sessionID` | 三轮原生续聊与仓储重开通过 |
| 自定义 CLI | 本地 fixture 进程回归 | 纯文本 / 有界历史 | 不提供原生续聊 |

OpenCode 临时副本来自 [官方 1.19.3 发布](https://github.com/anomalyco/opencode/releases/tag/v1.19.3)，下载 SHA-256 为 `c2d0fecd27acf6e08be02c1f5d384614d07495587766bcc4a6bff3971749bb64`，`codesign --verify` 通过。没有替换用户已有安装或改写全局模型配置。

真实样本位于 `src-tauri/tests/fixtures/cli-streams/`。样本保留协议结构，去掉无关系统/hook 通知，替换用户路径和原生 session ID。原始临时日志不提交；Grok 错误样本不能计为流式成功。具体命令、工程门禁与桌面测试范围见 [验收记录](dogfood/2026-10-20-simple-chat-acceptance.md)。

## 工具与流

工具片段按 `callId` 合并，运行态依次变成 done/error；runtime、持久化 journal 和前端投影遵循相同规则。Claude 的初始空输入不覆盖后续完整输入，最终 assistant 文本不重复追加已流式显示的正文。

可选字段包含 `kind`、`target`、`exitCode`、`diffStats`、`diffPreview`。Edit/Write 的 old/new 输入可生成差异；有 patch 内容的 Codex 事件按增删行计算。本机 0.163.0 实测的 file_change 只有路径与状态，因此 Code 回合在 Rust 侧先保存有界文本基线，再按已完成事件的路径计算前后替换差异；不使用 HEAD，已有未提交修改不会被误算为本轮新增。基线限制为 4096 个目录项、单文件 256 KiB、总计 8 MiB；跳过 symlink、二进制、依赖/构建目录和 .env。无法可靠比较时只显示文件变更信息，不伪造增删行。预览最多 40 行 / 8 KiB；旧片段缺字段时按 other 降级。

日志脱敏先保持 JSON 信封，再处理敏感字段；usage 的 token 计数不会破坏整条协议帧。正文、参数、工具输出和导出均作为文本处理，不执行其中的 HTML。

CLI 明确报告失败（result.is_error、turn.failed、OpenCode error）时，即使进程退出码为 0，也按失败保存，不更新续聊句柄。
