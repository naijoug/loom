# 本机 CLI 适配器

Loom 通过 Rust `agent_adapter.rs` 生成可执行文件、参数、cwd、stdin 方式、输出协议和必要的单次环境配置。React 只传会话、消息、Agent ID 和模式，不拼接命令。

## 探测与配置

`agents/config.rs` 先查 `$PATH`，再查当前 `$HOME` 下的 `.local/bin`、`.npm-global/bin`、`.opencode/bin`、`.grok/bin`，以及 `/opt/homebrew/bin`、`/usr/local/bin`。检查可执行位，不使用 shell，不包含写死的用户名目录。

内建 Agent 为 Claude Code、Codex、OpenCode、Grok；Dummy 仅在 debug/test 构建出现。已保存的命令路径覆盖和启用状态不被重新探测覆盖。诊断直接执行 `--version`，两秒超时，失败显示 `unknown`；“已安装”只代表找到可执行文件，登录/模型可用性需真实调用验证。

诊断返回 `capabilities { structuredStream, toolEvents, nativeResume, inlineCards, modes }`。每个 mode 含适配器生成的 policy 文本；设置表格不自行拼写参数。单项测试同样仅执行版本探测。

`inlineCards` 只对 Claude Code 为 true（实验）。设置 `experimentalInlineCards` 打开时，适配器每轮（含续聊）追加 `--append-system-prompt <提示词>`；提示词由 `pnpm openui:prompt` 生成并编译进二进制，执行记录中以 `[OPENUI_PROMPT sha256:…]` 代替原文。其他适配器忽略该字段，参数不变。

## 权限与续聊

| CLI | Research (`explore`) | Code (`ask`) | Auto-run (`auto`) | 输出 |
|---|---|---|---|---|
| Claude Code | `--permission-mode plan` | `acceptEdits` | `auto` | `stream-json` + partial messages |
| Codex | `--sandbox read-only` | `workspace-write` | `workspace-write` | `exec --json` |
| Grok | `--permission-mode plan` | `acceptEdits` | `auto` | `streaming-messages-json` |
| OpenCode | `plan`，单次权限配置默认拒绝，仅允许读取/搜索/网页读取 | `build`，编辑可用，其余遵守 CLI 确认 | `build`，自动权限，禁止 external_directory | `run --format json` |
| 自定义 CLI | 执行自身策略，Loom 不提供额外沙箱 | 同左 | 同左 | 纯文本 |

Codex 每轮显式设置 `approval_policy="never"`、cwd 和 sandbox，包括 resume；支持无 Git 的项目目录。Claude/Grok 每轮重申权限。Code 的发送确认是 Loom 的回合授权，不是逐工具审批；CLI 非交互模式可能拒绝需要额外审批的工具。

Claude 使用 `--resume <id>`，Codex 使用 `exec … resume --json -- <id> -`，Grok 使用 `--resume <id>`。成功后保存绑定 Agent/命令/项目/权限/参数的原生句柄；换 Agent 或模式清除句柄，保留转录并在下一轮有界重放。OpenCode 使用 `run --session <id>`，从事件的 `sessionID` 捕获原生句柄；官方 1.19.3 临时副本已通过三轮原生续聊、仓储重开和停止后再发，因此能力中启用 nativeResume。

OpenCode 的参数与权限配置依据 [官方 CLI 文档](https://opencode.ai/docs/cli/) 和 [权限文档](https://opencode.ai/docs/permissions/)；本机旧安装无法启动；另用官方验签的 1.19.3 临时副本完成 runtime 验收。模型调用仍使用 CLI 自身的账户配置，不在 Loom 管理凭据。

自定义 CLI 使用专用可执行文件，拒绝 `sh/bash/zsh/fish/cmd/powershell/pwsh` 包装；参数按数组传递，支持 `{prompt}`、`{projectPath}` 等已有模板。不承诺自定义工具卡片或原生续聊。

## CLI 实测矩阵（2026-10-20）

| CLI / 版本 | 真实状态 | 已观察的协议 | 续聊 |
|---|---|---|---|
| Claude Code 2.1.304 | 问答、读文件、pwd、Research 禁写、Code 编辑已测 | `tool_use.id` 与 `tool_result.tool_use_id`；Read/Bash/Edit/Write 输入输出；部分正文 + assistant 最终消息 | 三轮与每轮磁盘仓储重开通过 |
| Codex 0.163.0 | 问答、读取命令、pwd、Research 禁写、Code 编辑已测 | `thread.started.thread_id`；`item.started/completed`，命令 item id / aggregated_output / exit_code；file_change | 三轮与每轮磁盘仓储重开通过 |
| Grok 1.0.62 | blocked：返回 402，使用额度耗尽；同时报告刷新登录异常 | 真实 system/result 错误样本已保存 | 本轮未通过，保留既有适配能力，不将历史结果计为本轮通过 |
| OpenCode 1.19.3（官方临时副本） | 读取、Research 禁写、Code 编辑、停止后再发通过；本机原安装仍因无效代码签名 blocked | `step_start/tool_use/text/step_finish`；`part.callID`、`state.input.filePath`、`state.output`、`sessionID` | 三轮原生续聊与仓储重开通过 |
| 自定义 CLI | 本地 fixture 进程回归 | 纯文本 / 有界历史 | 不提供原生续聊 |

OpenCode 临时副本来自 [官方 1.19.3 发布](https://github.com/anomalyco/opencode/releases/tag/v1.19.3)，下载 SHA-256 为 `c2d0fecd27acf6e08be02c1f5d384614d07495587766bcc4a6bff3971749bb64`，`codesign --verify` 通过。没有替换用户已有安装或改写全局模型配置。

真实样本位于 `src-tauri/tests/fixtures/cli-streams/`。样本保留协议结构，去掉无关系统/hook 通知，替换用户路径和原生 session ID。原始临时日志不提交；Grok 错误样本不能计为流式成功。具体命令、工程门禁与桌面测试范围见 [验收记录](dogfood/2026-10-20-simple-chat-acceptance.md)。

## 工具与流

工具片段按 `callId` 合并，运行态依次变成 done/error；runtime、持久化 journal 和前端投影遵循相同规则。Claude 的初始空输入不覆盖后续完整输入，最终 assistant 文本不重复追加已流式显示的正文。

可选字段包含 `kind`、`target`、`exitCode`、`diffStats`、`diffPreview`。Edit/Write 的 old/new 输入可生成差异；有 patch 内容的 Codex 事件按增删行计算。本机 0.163.0 实测的 file_change 只有路径与状态，因此 Code 回合在 Rust 侧先保存有界文本基线，再按已完成事件的路径计算前后替换差异；不使用 HEAD，已有未提交修改不会被误算为本轮新增。基线限制为 4096 个目录项、单文件 256 KiB、总计 8 MiB；跳过 symlink、二进制、依赖/构建目录和 .env。无法可靠比较时只显示文件变更信息，不伪造增删行。预览最多 40 行 / 8 KiB；旧片段缺字段时按 other 降级。

日志脱敏先保持 JSON 信封，再处理敏感字段；usage 的 token 计数不会破坏整条协议帧。正文、参数、工具输出和导出均作为文本处理，不执行其中的 HTML。

CLI 明确报告失败（result.is_error、turn.failed、OpenCode error）时，即使进程退出码为 0，也按失败保存，不更新续聊句柄。
