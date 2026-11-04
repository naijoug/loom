# Agent 三种模式真实验收（A7，2026-11-03）

计划：[Agent 调用层计划 A7](../plans/2026-11-02/21:54-loom-agent-cli.md#a7-真实验收)。本记录只写本轮实际运行过的结果。

## 环境

- macOS（Darwin 25.5.0），`main` 工作区（含另一会话未提交的 Learn 发现改动，未参与本验收）。
- `loom-agent`：`cargo build --bin loom-agent`（debug）。
- CLI：codex-cli 0.166.0、Claude Code 2.1.317、opencode 2.1.2、grok 1.0.70；magpie 0.1.2687，网关监听 `127.0.0.1:3425`（`/v1/models`、`/v1/messages`、`/v1/responses` 均可用）。
- 隔离：临时 `LOOM_PROFILE_DIR`（新建，`providers.json` 0600、`cli/runs` 0700）+ 临时 git 项目（只有 `NOTES.md`，内含魔法词 `TANGERINE`）。未写 `~/.claude`、`~/.codex`，未读 CLI 订阅凭据。
- key：全部用 `--key-env 变量名` 引用本机已有环境变量，没有 `set-key`，命令行与本文都不含 key 值。

## Provider

| id | 类型 | Base URL | 默认模型 | key | `provider test` |
|---|---|---|---|---|---|
| ds-chat | openai-chat | `https://api.deepseek.com` | deepseek-chat | `$DEEPSEEK_API_KEY` | ok 200 |
| ds-anthropic | anthropic | `https://api.deepseek.com/anthropic` | deepseek-chat | `$DEEPSEEK_API_KEY` | not_found 404（该端点没有模型列表，实际对话可用，见发现 1） |
| openai | openai-responses | `https://api.openai.com/v1` | gpt-5-mini | `$OPENAI_API_KEY` | ok 200 |
| zai | anthropic | `https://api.z.ai/api/anthropic` | glm-4.6 | `$Z_AI_API_KEY` | ok 200（列模型可用，但对话 429） |
| openrouter-anth | anthropic | `https://openrouter.ai/api` | anthropic/claude-haiku-4.5 | `$OPENROUTER_API_KEY` | ok 200 |
| magpie | openai-chat | `http://127.0.0.1:3425/v1` | auto/free | 无（本机网关） | ok 200 |
| magpie-anth | anthropic | `http://127.0.0.1:3425` | auto/claude-haiku-4-5 | 无（本机网关） | ok 200 |

## 运行结果

全部是真实模型调用（没有合成测试）。模式 A/B 的提示词为“Read NOTES.md and reply with only the magic word.”，模式 C 为“Reply with only the word PONG.”。“key 检查”指：运行期间每 0.2 s 抓一次 `ps -axww -o args=`，结束后扫描 NDJSON 输出（含 `--verbose` 日志行）与整个 profile 目录（`providers.json`、执行记录），用 5 个本机 key 的全文及首尾各 12 字符匹配；只统计命中数，不输出值。

| 模式 | 执行器 / provider | 模型 | 结果 | 退出码 | 用时 | key 命中（ps / 文件） | 残留进程 |
|---|---|---|---|---|---|---|---|
| A | Codex 自带登录 | 默认 | 通过：读文件后回答 `TANGERINE` | 0 | 19.7 s | 0 / 0 | 0 |
| B | Codex + openai | gpt-5-mini | 失败：Codex 报 “Incomplete response returned”，并提示没有 `gpt-5-mini` 元数据 | 1 | 29.8 s | 0 / 0 | 0 |
| B | Codex + openai | gpt-5.4-mini | 通过：`TANGERINE` | 0 | 24.1 s | 0 / 0 | 0 |
| B | Claude Code + zai | glm-4.6 / glm-5.3-flash | **blocked**：上游持续 429（Claude 重试 10 次后被 `--timeout-secs` 停止，退出 124）；模式 C 直连同样 429，判定为账户额度 / 限流 | 124 | 182 s / 122 s | 0 / 0 | 0 |
| B | Claude Code + ds-anthropic | deepseek-chat | 失败：DeepSeek 拒绝 Claude Code 内置工具的 JSON Schema（`pattern` 不被识别为正则，HTTP 400），属 provider 兼容性 | 1 | 8.5 s | 0 / 0 | 0 |
| B | Claude Code + openrouter-anth | anthropic/claude-haiku-4.5 | 通过：`TANGERINE` | 0 | 12.9 s | 0 / 0 | 0 |
| B（magpie） | Claude Code + magpie-anth | auto/claude-haiku-4-5 | 通过：`TANGERINE`（网关内部路由到哪个上游未核实） | 0 | 12.9 s | 0 / 0 | 0 |
| C | ds-chat（OpenAI 兼容） | deepseek-chat | 通过：`PONG` | 0 | 2.9 s | 0 / 0 | — |
| C | ds-anthropic（Anthropic） | deepseek-chat | 通过：`PONG` | 0 | 2.9 s | 0 / 0 | — |
| C | openai（Responses） | gpt-5.4-mini | 通过：`PONG` | 0 | 3.7 s | 0 / 0 | — |
| C（magpie） | magpie（OpenAI 兼容） | auto/free | 通过：`PONG` | 0 | 3.7 s | 0 / 0 | — |
| C | zai（Anthropic） | glm-5.3-flash | **blocked**：`api_rate_limited`（HTTP 429），错误摘要未回显上游正文 | 1 | 2.9 s | 0 / 0 | — |

计划要求的组合：模式 A（Codex）通过；模式 B 的 Claude Code + Anthropic 兼容（OpenRouter、magpie）与 Codex + OpenAI Responses 均通过；模式 C 的 Anthropic（DeepSeek `/anthropic`）与 OpenAI 兼容（DeepSeek、OpenAI Responses）均通过；magpie 网关模式 B、C 各一次通过。Z.ai 记 blocked。OpenCode 与 Grok 本轮未跑。

## 中断（Ctrl-C）

用长提示词（“写 600 字散文”）在运行中发 SIGINT，结束后 1.5 s 检查运行期间见过的全部子孙进程是否还活着。

| 场景 | 发送方式 | 退出码 | 结束状态 | 子孙进程 | 残留 |
|---|---|---|---|---|---|
| Codex 自带登录 | 6 s 后向 `loom-agent` 单发 | 130 | aborted / chat_abort | 10 | 首次 1 个（未记下身份）；同条件复跑 2 次均为 0，5 s 后 `ps` 无本次运行的 codex 进程 |
| Claude + openrouter-anth | 6 s 后向整个进程组发（等同终端 Ctrl-C） | 130 | aborted / chat_abort | 3 | 0 |
| Codex + openai | 4 s 后向进程组连发两次（强制停止） | 130 | aborted / chat_abort | 11 | 0 |
| 模式 C ds-chat | 1.5 s 后单发 | 130 | aborted / chat_abort | 0 | 0 |

首次那 1 个残留没有复现，也没有拿到进程身份，不能排除是 1.5 s 内尚未退出的子进程；记为未解决的偶发观察。

## Discuss：无项目、模式 C

`src-tauri/src/discuss/real_tests.rs` 新增可选真实测试 `api_participants_complete_without_project`（`#[ignore]`，需 `LOOM_REAL_DISCUSS=1` 与 `LOOM_REAL_DISCUSS_API_A/B`）：两位 `api` 参与者，无 `project_path`，走桌面同一 `Pipeline`。

```bash
LOOM_PROFILE_DIR=<临时 profile> LOOM_REAL_DISCUSS=1 LOOM_REAL_DISCUSS_ROOT=<临时目录> \
  LOOM_REAL_DISCUSS_API_A=ds-chat LOOM_REAL_DISCUSS_API_B=openai \
  cargo test --manifest-path src-tauri/Cargo.toml --lib discuss::real_tests::api_participants -- --ignored --nocapture
```

结果：通过（16.5 s）。r1-p1、r1-p2、r2-p1、r2-p2、summary 五个任务均 Done、各 1 次尝试，讨论状态 `ready`；汇总给出共识与分歧，并注明没有附项目文件。讨论目录（prompt、结果、作业记录）纳入上面的文件扫描，key 命中 0。

## 发现

1. **`provider test` 对部分 Anthropic 兼容端点误报**：测试用 `GET /v1/models`，DeepSeek `/anthropic` 没有这个接口，返回 404 → `not_found`，但对话正常。暂未修改，文档补了说明。
2. **日志脱敏过度**：`--verbose` 日志行、执行记录 `stderrTail` 和 Codex 错误片段用按行脱敏（`agent::redact_sensitive_text`），整行只要含 `token` 就截断到第一个 `:`。于是带 `usage.input_tokens` 的 Claude 结果帧变成 `{"api_error_status": [REDACTED]`，Codex 的 “Incomplete response returned, reason: …” 原因（推测为 `max_output_tokens`）也被遮住。这不是泄露，但降低了诊断价值；JSON 感知的 `redact_stream_line` 已存在，只是 CLI 日志没用它。
3. **错误摘要取到无关行**：Codex 模式 B 失败时 `errorSummary` 取的是 stderr 第一行（用户 `~/.codex` 中 MCP 服务的 `rmcp` 握手错误），真正原因在 stdout 的 `turn.failed`。模式 B 不改 CLI 全局配置，所以用户自己的 MCP 服务照常加载，这符合设计，但摘要会误导。
4. **Claude Code 对非 Claude 模型名打印 `unrecognized_model`**：只是 stderr 警告，不影响结果。
5. **Codex 的模型元数据**：`gpt-5-mini` 在 Codex 0.166.0 里没有元数据，走回退后响应不完整；`gpt-5.4-mini` 正常。provider 默认模型应选 Codex 认识的名称。

## 未验证

- `.app` 是否内含 `loom-agent`：本轮未执行 `tauri build`。
- OpenCode 与 Grok 的真实模式 A/B、自定义 CLI。
- 桌面窗口里通过 Settings 保存 / 测试 provider 后走真实对话（本轮只用 `loom-agent` 与 Discuss 测试管线）。
- magpie 网关内部的上游路由。
