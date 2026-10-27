# Learn L0：CLI 与成本探针（2026-10-26）

对应 [Learn 实施计划](../plans/2026-10-26/19:26-learn-repo-study-module.md) L0。探针脚本放在会话 scratchpad，不提交；本文记录命令、版本和结论。所有运行均在样例仓库的独立副本中进行，运行前后计算文件指纹并检查 `git status`。

## 环境与版本

| 项 | 值 |
|---|---|
| 主机 | macOS（Darwin 25.5.0，arm64） |
| Claude Code | 2.1.309；经 `~/.claude/settings.json` 指向本机代理（默认模型 `auto/glm-5.3[1m]`，非 Anthropic 官方模型） |
| Codex | 0.153.0（VS Code agent-host 缓存二进制；全局 0.164.0 安装缺少 `@openai/codex-darwin-arm64`，未修改用户全局安装） |
| OpenCode | v2.1.2（默认模型） |
| Grok | 1.0.70（默认 grok-4.7-build） |
| git | 2.53.0 |
| 样例仓库 | `sharkdp/hexyl` @ `6ecc29b9`（22 个跟踪文件，约 3.4k 行代码）；`junegunn/fzf` @ `26ff936d`（163 个跟踪文件，约 46.7k 行代码） |

## 模型参数与报错行为

| CLI | 参数 | 列表命令 | 非法模型 |
|---|---|---|---|
| Claude Code | `--model <name>` | 无 | 经代理时非法名称仍 **成功**，`modelUsage` 只回显请求名，无法核实实际模型 |
| Codex | `-m/--model <name>` | 无 | `model_not_found`，rc=1；合法名称也可能打印 “Model metadata not found” 警告 |
| OpenCode | `--model provider/model[#variant]` | `opencode models` | `provider.no-route`，rc=1 |
| Grok | `-m/--model <name>` | `grok models` | `unknown model id`，rc=1 |

实测模型名包含 `[ ] @ #`（如 `auto/glm-5.3[1m]`、`grok-4.7@500k`、`provider/model#variant`），计划原定的 `.-_/:` 字符集过窄。`grok -m magpie/auto/glm-5.3-flash` 挂起 272 s 后报序列化错误，属于 CLI/代理问题，Learn 需依赖超时与取消兜底。

## 结构化输出

| CLI | 原生能力 | 结果 |
|---|---|---|
| Claude Code | `--json-schema '<schema>'`，结果在 `result.structured_output` | 可用 |
| Codex | `--output-schema <file>` + `-o last.txt`，最终消息即 JSON | 可用 |
| Grok | `--json-schema '<schema>'` | 可用 |
| OpenCode | 无 | 只能从最终文本提取 ` ```json ` 围栏块；hexyl 上出现 2 处 schema 漂移（`entryPoints` 为字符串） |

结论：统一在 Rust 侧做“结构化字段优先 → 最后一个 json 围栏块 → 整段 JSON”的提取，并做 schema 与路径校验；校验失败给一次修复回合。原生 schema 参数作为增强，不作为唯一保障。

## 架构阶段（只读，单次调用）

调用方式：Claude `-p --output-format stream-json --verbose --permission-mode plan --json-schema`；Codex `exec --json -s read-only -c approval_policy="never" --output-schema`；OpenCode `run --format json --agent plan`；Grok `-p --output-format streaming-messages-json --permission-mode plan --json-schema`。全部 Claude 运行清除了宿主会话继承的 `ANTHROPIC_*` / `CLAUDE_*` 环境变量（否则 401）。

### hexyl（约 3.4k 行）

| CLI | 耗时 | 工具调用 | tokens 入 / 出 | 模块 | 引用 | 越界引用 | 标识符命中 | 源码覆盖 | 只读 |
|---|---|---|---|---|---|---|---|---|---|
| Claude | 706 s | 13 | 267k / 22.7k | 8 | 31 | 0 | 29/29 | 1.0 | 是 |
| Codex | 259 s | 12 | 376k（缓存 300k）/ 4.4k | 7 | 33 | 0 | 10/10 | 1.0 | 是 |
| OpenCode | 399 s | 13 | 173k / 8k | 6 | 20 | 0 | 20/20 | 1.0 | 是 |
| Grok | 132 s | **0** | 26.7k / 2.7k | 6 | 17 | 0 | 4/5 | 1.0 | 是 |

### fzf（约 46.7k 行）

| CLI | 耗时 | 工具调用 | tokens 入 / 出 | 模块 | 引用 | 越界引用 | 标识符命中 | 源码覆盖 | 只读 |
|---|---|---|---|---|---|---|---|---|---|
| Codex | 454 s | 16 | 532k（缓存 444k）/ 9k | 10 | 50 | 0 | 12/14 | 1.0 | 是 |
| Grok | 139 s | **0** | 28k / 5.5k | 12 | 45 | 0 | 17/23 | 0.991 | 是 |
| Claude（代理模型） | > 47 min 未完成，手动终止 | 35 | — | — | — | — | — | — | 是 |
| OpenCode | > 47 min 未完成，手动终止 | 58 | — | — | — | — | — | — | 是 |

“标识符命中”：引用 `why` 中出现、且在该文件中存在的标识符，是否落在引用行范围内（启发式，仅统计可判定的引用）。行号“在文件范围内”不等于“准确”，Grok 未读任何文件、范围偏宽（如 `src/main.rs` 250–520），命中率明显低。

## 模块页：全新调用 vs 续聊复用（Codex，fzf `input-data` 模块）

| 方式 | 耗时 | 工具调用 | tokens 入 / 出 | 链接 | 越界 | 中位跨度 |
|---|---|---|---|---|---|---|
| 全新调用（附架构 JSON 中的模块条目） | 93 s | 14 | 144k（缓存 102k）/ 1.7k | 23 | 0 | 21 行 |
| 续聊架构会话 | 69 s | 6 | 755k（缓存 554k）/ 10.3k | 17 | 0 | 19 行 |

续聊会把整个架构会话历史重新计入输入，token 约为全新调用的 5 倍，质量无提升。结论：模块页用全新调用，提示词附上该模块的架构条目与相关模块摘要；不做续聊复用。

## 出口决策

1. **结构化输出**：Rust 统一提取 + 校验 + 一次修复回合；Claude / Codex / Grok 可附加原生 schema 参数（L4 首版先只做提取，原生参数按适配器能力可选）。
2. **模型参数**：Claude `--model`、Codex `--model`、OpenCode `--model`、Grok `--model`。模型名校验改为 1–128 字符、不含空白和控制字符、不以 `-` 开头；不支持的适配器传入模型时报错。
3. **默认超时**：架构阶段 30 分钟、模块页 15 分钟、术语表 10 分钟（均高于 Chat 的 10 分钟）；超时后作业失败并保留日志，可重试或换 Agent。代理模型在约 5 万行仓库上可能超过 47 分钟，新建表单需提示“大仓库建议 Codex 或缩小深度”。
4. **仓库上限**：跟踪文件数 ≤ 20,000、checkout ≤ 500 MiB；清单基于 `git ls-files`。
5. **环境**：Loom 桌面从 Finder/Dock 启动时不继承宿主 Agent 的 `ANTHROPIC_*` 变量；从终端开发时可能继承，记录为开发注意事项，不在适配器中静默删除用户环境。
6. **验收指标**：除越界引用率外，记录工具调用次数（0 次视为未读源码，作业给出警告）。

## 未覆盖

- Claude 官方模型（本机走代理）、OpenCode 在大仓库上的完成耗时。
- Windows / Linux。
