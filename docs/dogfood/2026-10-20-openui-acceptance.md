# OpenUI 内联卡片 O5 真实模型验收记录（2026-10-20）

- 对应计划：[OpenUI 内联卡片实验](../plans/2026-10-20/18:25-openui-inline-cards-experiment.md) O5
- 代码：`codex/simple-cli-chat` @ `98fbd2f`（O1–O4 已提交）
- 环境：Claude Code 2.1.304；本机经 `~/.claude/settings.json` 指向本地代理，后端是 **GLM-5.3**，不是 Anthropic Claude 模型。用户全局 `~/.claude/CLAUDE.md`（英语改写指令）对所有回合生效，与真实 Loom 使用一致。
- 结论：**继续实验，不转正**，开关保持默认关闭。卡片质量、安全与持久化约束全部满足，但本环境下出卡率未达 S6；成本（S7）可接受。

## 1. 方法

- 入口：忽略测试 `chat::real_agent_tests::claude_inline_cards_acceptance`（`LOOM_REAL_CHAT=1`）。走真实 Loom 链路：`prepare_chat_invocation(..., inline_cards = true)` → `run_chat_turn` → `finish_chat_turn`，与桌面端 `chat_send` 相同；每轮断言参数含 `--append-system-prompt`。
- 项目：每个用例一个合成 JS 项目（`fixture-calc`：7 个文件、2 次提交、1 处未提交改动）；`.loom/` 加入 git exclude，项目写入按 `git status --porcelain` 前后对比判断。
- 用例（中文提问；q08、q09 期望不出卡片；q10 续聊 q01 的会话）：

| 编号 | 提问 | 期望 |
|---|---|---|
| q01 | 列出这个项目的入口文件，并说明每个文件的作用。 | FileList |
| q02 | 总结最近一次提交改了什么。 | ChangeSummary |
| q03 | 对比 src/add.js 和 src/sub.js 的实现，用表格说明差异。 | DataTable |
| q04 | 给出这个项目发布前的检查清单。 | Checklist |
| q05 | 哪些文件包含测试？分别测了什么？ | FileList/DataTable |
| q06 | package.json 里的每个 script 分别做什么？请对比说明。 | DataTable |
| q07 | 总结工作区当前还没提交的改动。 | ChangeSummary |
| q08 | add 函数返回什么类型？一句话回答。 | 无卡片 |
| q09 | 用一两句话说明这个项目是做什么的。 | 无卡片 |
| q10 | 把测试文件也加进刚才的入口文件列表里。（续聊 q01） | FileList |

- 模式：Research（`explore` → `--permission-mode plan`）与 Code（`ask` → `--permission-mode acceptEdits`）各跑一遍；每轮超时 600 秒；最多 6 个进程并发；子进程用 `env -i HOME PATH USER LANG TMPDIR` 的干净环境（见 O0 记录的 401 问题）。
- 评分：`node scripts/openui-acceptance.mjs <dir>...`，用前端 `checkInlineCard` 解析回复中的每个 `openui-lang` 代码块。

## 2. 结果

| case | mode | status | s | tools | blocks | parsed | problems | writes | verdict |
|---|---|---|---|---|---|---|---|---|---|
| q01 | explore | complete | 172 | 5 | 1 | 1 | — | no | card |
| q02 | explore | complete | 385 | 1 | 1 | 1 | — | no | card |
| q03 | explore | complete | 75 | 3 | 1 | 1 | — | no | card |
| q04 | explore | aborted | 600 | 32 | 0 | 0 | — | no | invalid |
| q05 | explore | complete | 135 | 12 | 0 | 0 | — | no | no card |
| q06 | explore | complete | 63 | 4 | 0 | 0 | — | no | no card |
| q07 | explore | complete | 32 | 1 | 1 | 1 | — | no | card |
| q08 | explore | complete | 120 | 9 | 0 | 0 | — | no | ok (no card) |
| q09 | explore | complete | 31 | 2 | 0 | 0 | — | no | ok (no card) |
| q10 (resume) | explore | complete | 18 | 0 | 1 | 1 | — | no | card |
| q01 | ask | complete | 123 | 6 | 0 | 0 | — | no | no card |
| q02 | ask | aborted | 600 | 12 | 0 | 0 | — | no | invalid |
| q03 | ask | complete | 43 | 3 | 1 | 1 | — | no | card |
| q04 | ask | complete | 72 | 3 | 1 | 1 | — | no | card |
| q05 | ask | complete | 162 | 9 | 0 | 0 | — | no | no card |
| q06 | ask | complete | 217 | 7 | 0 | 0 | — | no | no card |
| q07 | ask | complete | 157 | 8 | 0 | 0 | — | no | no card |
| q08 | ask | complete | 179 | 12 | 0 | 0 | — | no | ok (no card) |
| q09 | ask | complete | 19 | 2 | 0 | 0 | — | no | ok (no card) |
| q10 (resume) | ask | complete | 31 | 1 | 0 | 0 | — | no | no card |

explore: 10 turns, 9 complete; card cases with a parseable card 5/7; no-card cases without a card 2/2; blocks parsed 5/5 (fallbacks 0); project writes 0; prompt redacted in logs 10/10

ask: 10 turns, 9 complete; card cases with a parseable card 2/7; no-card cases without a card 2/2; blocks parsed 2/2 (fallbacks 0); project writes 0; prompt redacted in logs 10/10

`s` 为秒；`tools` 为工具调用次数；`invalid` 为回合未完成（两例都是 600 秒超时，期间反复 Read）。

| 指标 | Research | Code |
|---|---|---|
| 完成回合 | 9/10 | 9/10 |
| 期望卡片的有效回合产出可解析卡片 | 5/7 | 2/7 |
| 期望无卡片的回合未出卡片 | 2/2 | 2/2 |
| 代码块解析成功（回退次数） | 5/5（0） | 2/2（0） |
| 项目写入 | 0 | 0 |
| 执行记录中提示词已脱敏 | 10/10 | 10/10 |
| 续聊回合（q10）产出卡片 | 是 | 否 |

未出卡片的原因（逐条查看回复）：

- **选择 Markdown 表格**：Research q05/q06、Code q05/q06。内容正确，只是模型判断表格足够；提示词规则“只在结构化视图明显有帮助时使用卡片”本身偏保守。
- **工具发现失败**：Code q01（声称没有本地读取工具并拒答）、q07（绕道 Monitor 执行 git）、q10，以及 q05/q06 的回复都提到“没有 Read/Bash”。这与 O0 记录的 GLM-5.3 工具误判一致，属于本机代理模型行为，与卡片无关，但拉低了 Code 出卡率。
- **全局指令**：18/20 个回复以英语改写块开头，说明用户全局指令与追加提示词并存；没有导致卡片语法错误。

## 3. 成本（S7）

同一条无工具提问（`claude -p --output-format json --permission-mode plan`，空目录），各两次：

| 变体 | 输入 token（input + cache） |
|---|---|
| 不追加 | 17605（首次，离群）、11234 |
| 追加提示词 | 12289、12291 |

每次 API 调用约 **+1.05k token**，与 O0 的 1048 一致，约为一次无工具回合输入的 9%。多工具回合会按调用次数放大。作为默认关闭、设置页已说明成本的实验功能，判定**可接受**。

## 4. 安全与持久化

- 运行日志与会话文件（`.loom/chat/v2/…`）中：提示词原文出现 0 次；脱敏标签 `[OPENUI_PROMPT sha256:…]` 出现在 36 个文件中。
- 卡片源码作为普通正文保存在 `snapshot.json`/`events.jsonl`；导出 Markdown 把正文原样放入 text 围栏，卡片源码随导出保留（按代码核对，未在桌面端实际导出）。
- Research 与 Code 两种模式下项目写入均为 0。

## 5. 未覆盖

- **桌面端交互**：`scripts/debug.sh desktop` 下的开关切换、续聊句柄失效提示、导出内容未实测（需人工操作 Tauri 窗口），列为人工验收项。续聊句柄跨开关失效由 O2 单测覆盖。
- **Anthropic 官方模型**：本机无直连账号配置，结论只代表“Claude Code CLI + GLM-5.3”。
- 每个用例只有单样本。

## 6. 建议

1. 保持实验开关默认关闭，不改需求文档与 Chat 契约。
2. 在 Anthropic 官方模型上复跑本测试（同一命令），再决定是否转正。
3. 若要提高出卡率，可把规则改为“列出 3 个以上文件、对比或检查清单时优先使用卡片”，改动后需重新生成提示词并复跑。
