# 简化 CLI Chat 验收记录（2026-10-20）

- 对应计划：[重构计划](../plans/2026-10-20/00:44-simple-cli-chat-refactor.md) §1 成功标准、[实施计划](../plans/2026-10-20/01:01-simple-chat-implementation.md) P9 场景矩阵。
- 代码：分支 `codex/simple-cli-chat`，`7e1dfcb`（CLI 样本）+ `3dd9a9f`（实现），未合入 `main`；文档改动在工作区未提交。
- 环境：macOS（Darwin 25.5.0），Claude Code 2.1.304、Codex 0.163.0、Grok 1.0.62、OpenCode 1.19.3（官方临时副本，见下文）。
- 结论：**partial**。工程门禁通过；Claude Code / Codex / OpenCode 的真实 CLI 进程验收通过；**用户桌面验收（`scripts/debug.sh desktop`）未执行**，所以没有任何一项按计划要求标为桌面 `passed`。

## 状态口径

| 状态 | 含义 |
|---|---|
| `passed` | 已按标准本身要求的证据层级验证 |
| `partial` | 已有较低层级的证据，例如真实 CLI 进程（无 UI）或合成网页，但缺少标准要求的桌面验证 |
| `blocked` | 已尝试，因环境原因（额度、签名、登录）无法完成 |
| `not-tested` | 本轮没有运行 |

证据分三层，不能互相替代：

1. **工程与合成**：单测、构建、样本回放、`pnpm smoke:visual` 合成网页（`kind: synthetic-web-only`）。
2. **真实 CLI 进程**：`src-tauri/src/chat/real_agent_tests.rs` 里需要显式开启的测试（`LOOM_REAL_CHAT=1 … -- --ignored`）。它们经过生产仓储、runtime 和适配器调用真实模型，但不经过 React UI。所谓“重启”是每轮重新打开磁盘仓储，并不是重启应用。
3. **用户桌面**：本轮未执行。

## 工程门禁

| 检查 | 结果 | 证据 |
|---|---|---|
| `pnpm test` | passed | 82/82 |
| `pnpm build` | passed | tsc + vite build |
| `cargo test`（src-tauri） | passed | 148 通过，16 个真实 CLI 测试按设计忽略 |
| `cargo clippy --all-targets --all-features -D warnings` | passed | 无警告 |
| `pnpm check`（含 release 文档检查与 fmt） | passed | 2026-10-20 review 时在 HEAD + 当前工作区复跑；`check.sh` 使用 `set -e`，最后一步 cargo test 已执行完毕 |
| `pnpm tauri build --no-bundle` | passed | 同日复跑，release 构建产物 `src-tauri/target/release/loom` |
| 样本回放 | passed | `chat::tools` 测试读取 `src-tauri/tests/fixtures/cli-streams/` 下 Claude / Codex / OpenCode 样本，验证配对且没有重复卡片 |
| 旧数据兼容 | passed | `chat::models` 读取旧 snake_case 工具片段；Rust 非测试代码中没有删除 `.loom/` Task 数据的调用 |
| 硬编码文案 | passed | `src/features`、`src/components`、`src/hooks`、`src/state` 中没有 CJK 字符（字典与测试除外） |
| 旧工作流残留 | passed | `chat_promote_to_task`、`WorkflowShell`、`useTaskBridge`、`VITE_LOOM_WORKBENCH` 在源码、契约、脚本中零命中（`preview.ts` 中的 diff 示例文本除外） |

## 成功标准（重构计划 §1）

| # | 标准 | 状态 | 依据 / 缺口 |
|---|---|---|---|
| 1 | 默认构建即 Chat，没有 Task/Board/Planning 入口 | passed | `AppView = "chat" \| "settings"`；旧组件和命令已删除；构建通过 |
| 2 | 侧栏按项目分组、可折叠，会话显示模式和 Agent | partial | 合成网页通过（`project groups`）；桌面 not-tested |
| 3 | 启动时探测本机 CLI，不写死用户目录 | partial | 候选路径与超时有单测；设置页的真实显示没有在桌面核对；本机 OpenCode 原安装因签名无效无法启动 |
| 4 | Claude Code / Codex 真实桌面验收 | partial | 真实 CLI 进程通过（见下表）；桌面 not-tested |
| 5 | OpenCode 流式 + 多轮 + 停止，续聊按实测 | partial | 官方 1.19.3 临时副本通过流式、三轮原生续聊和停止后再发；用户本机安装 blocked；桌面 not-tested |
| 6 | Grok 和自定义 CLI 不回退 | blocked / partial | Grok 返回 402（额度耗尽）；自定义 CLI 只有本地 fixture 进程回归 |
| 7 | test / cargo test / build / tauri build 通过，无死引用 | passed | 见“工程门禁” |
| 8 | 能读取旧 `.loom/chat/v2`，Task 数据不被修改或删除 | passed | 兼容读取测试；没有删除 Task 数据的代码 |
| 9 | 默认英文，可切简体中文，切换后立即生效并持久化 | partial | i18n 单测（key 一致、插值、重新挂载后持久化）+ 合成网页 `language persists after reload`；桌面重启 not-tested |
| 10 | 新会话页只选模式，Agent 在 Composer 左下选择 | partial | 合成网页 `new chat mode-only cards`、`new chat sends with selected Agent and mode`；桌面 not-tested |

## P9 场景矩阵

每格先写状态，再写证据所在的 sample。`partial` 表示真实 CLI 进程或合成网页已通过，桌面 not-tested。

| 场景 | Claude Code | Codex | OpenCode 1.19.3 | Grok 1.0.62 |
|---|---|---|---|---|
| 流式回复 | partial（cli-resume） | partial（cli-resume） | partial（opencode） | blocked（402，cli-resume） |
| 连续三轮追问 | partial（cli-resume，每轮重开仓储） | partial（cli-resume） | partial（opencode） | blocked |
| 工具卡片：读、命令、编辑 diff | partial（Read/Edit/Write 片段：claude-permissions；样本回放） | partial（command/file_change：codex-diff；基线 diff） | partial（read/edit：opencode） | not-tested |
| 停止后再发 | partial（claude-stop；通过 API 中止，不是 Esc） | partial（codex-stop，之前有失败） | partial（opencode-stop，之前有失败） | not-tested |
| 重启后续聊 | partial（仓储重开 + 原生续聊；应用重启 not-tested） | partial（同左） | partial（同左） | blocked |
| 调研模式无文件写入 | partial（claude-permissions：新建与续聊的 explore 均 `file_changed=false`，CLI 尝试 Write 被拦截） | partial（codex-permissions） | partial（opencode） | not-tested（`grok_file_permissions…` 未运行） |
| 中途调研切 Code 后能改文件 | partial（claude-mode-switch，之前有失败） | partial（codex-mode-switch） | not-tested（没有对应测试） | — |
| 切换 Agent 后历史重放 | partial（agent-switch：Claude → Codex） | 同左 | — | — |
| 两项目分组，重启后分组与折叠保留 | partial（合成网页；折叠状态重启保留未单独验证） | | | |
| 新会话只有模式卡片，按所选 Agent 创建 | partial（合成网页） | | | |
| 默认英文，切中文后重启保留 | partial（单测 + 合成网页 reload） | | | |

sample 均位于 [`samples/simple-chat/`](samples/simple-chat/)，文件名为 `<名称>.txt`。合成网页结果见 [browser-result.json](samples/simple-chat/browser-result.json)，截图见 `designs/exports/simple-chat/`。

## 真实 CLI 运行明细与失败记录

sample 保留了开发过程中的失败运行。下表按 cargo 测试总数（passed + failed + filtered）排序，这个数字可以近似反映运行时的代码状态。HEAD 的总数是 164（148 + 16 ignored）。

| sample | 测试总数 | 结果 |
|---|---|---|
| [cli-resume](samples/simple-chat/cli-resume.txt) | 146 | Claude ✔、Codex ✔ 三轮续聊；Grok ✘：turn 1 `status=error stream_bytes=0` |
| [claude-permissions](samples/simple-chat/claude-permissions.txt) | 148 | Claude ✔；Codex ✘：explore round 1 没有工具事件（`parts=[]`） |
| [codex-permissions](samples/simple-chat/codex-permissions.txt) | 148 | Codex ✔（与上一行失败时的总数相同） |
| [codex-diff](samples/simple-chat/codex-diff.txt) | 149 | Codex ✔，ask 两轮都有 `file_change` |
| [claude-stop](samples/simple-chat/claude-stop.txt) | 151 | Claude ✔；Codex ✘（没有输出摘要） |
| [codex-stop](samples/simple-chat/codex-stop.txt) | 154 | Codex ✔ |
| [codex-mode-switch](samples/simple-chat/codex-mode-switch.txt) | 157 | Codex ✔；Claude ✘：explore round 1 `aborted native=false`，文件未改 |
| [agent-switch](samples/simple-chat/agent-switch.txt) | 157 | Claude → Codex 重放 ✔ |
| [claude-mode-switch](samples/simple-chat/claude-mode-switch.txt) | 159 | Claude ✔ |
| [opencode](samples/simple-chat/opencode.txt) | 162 | 权限 ✔、三轮续聊 ✔；停止 ✘ |
| [opencode-stop](samples/simple-chat/opencode-stop.txt) | 164 | OpenCode 停止 ✔ |

限制：

- sample 没有保存失败的 panic 信息，所以无法判断后来的通过是因为代码修复，还是因为模型行为不同。Codex 权限测试在同一测试总数下先失败后通过，可能存在不稳定。
- 只有最后一条（opencode-stop）和 HEAD 的测试总数一致。**真实 CLI 测试没有在 `3dd9a9f` 上完整重跑一遍**。重跑会消耗账户额度，需要用户决定。
- OpenCode 结果来自官方 1.19.3 临时副本，来源、SHA-256 和签名见 [opencode-provenance.json](samples/simple-chat/opencode-provenance.json)。用户本机原有安装签名无效（exit 137），保持原样未改动。
- Grok 失败是账户额度问题（402），不能视为适配器回退，也不能视为通过。

## 已知限制（未在本轮解决）

- **桌面专属行为未验证**：切换 Agent 的确认和 Auto-run 首次警告使用 `window.confirm`，它在 Tauri WKWebView 中能否弹出未验证；计划设计的是应用内弹层。
- **Codex 的 Auto-run 与 Code 参数相同**：都是 `workspace-write; approval_policy=never`。打开 Auto-run 只是跳过 Loom 的回合确认，不改变 CLI 行为。
- **工具卡片元信息不全**：没有行数、匹配数和耗时；同一条消息的多张工具卡片会重复渲染执行日志入口。
- **样本环境不干净**：Codex 样本中出现了本机全局指令（`rtk`、`RTK.md`）的痕迹，录制时没有隔离用户级 CLI 配置。

## 下一步

1. 按 [测试指南 · 用户验收](../testing.md#用户验收) 跑 `scripts/debug.sh desktop`（前后执行 `scripts/debug.sh stop`），把上表中的 `partial` 升级为 `passed`，或改记为失败。
2. 用户同意消耗额度后，在 HEAD 上重跑 Claude / Codex / OpenCode 的真实 CLI 测试，并保存失败输出。
3. Grok 额度恢复后补测续聊和权限。
