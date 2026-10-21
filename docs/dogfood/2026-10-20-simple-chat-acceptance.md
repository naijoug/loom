# 简化 CLI Chat 验收记录（2026-10-20）

- 对应计划：[重构计划](../plans/2026-10-20/00:44-simple-cli-chat-refactor.md) §1 成功标准、[实施计划](../plans/2026-10-20/01:01-simple-chat-implementation.md) P9 场景矩阵。
- 交付基线：分支 `codex/simple-cli-chat`；`7e1dfcb` 为 CLI 样本，`3dd9a9f` 为重构实现，`0acbb10` 已提交首版验收资料。未推送、未合入 `main`。本记录的完整工程门禁针对重构基线，不自动覆盖后续 UI 打磨改动。
- 环境：macOS（Darwin 25.5.0），Claude Code 2.1.304、Codex 0.163.0、Grok 1.0.62、OpenCode 1.19.3（官方临时副本，见下文）。
- 交付结论：**重构工程与资料收尾完成，产品验收为 partial，等待用户桌面验收**。用户已明确选择“原生桌面验收留给我，完成其余工作后通知我”。本轮不再以自动操作原生窗口作为交付前提；没有将任何未执行的桌面场景标为 passed。

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
3. **用户桌面**：自动化曾因 Mac 锁屏无法操作；随后用户明确接手。本轮仍记 not-tested。

## 工程门禁

| 检查 | 结果 | 证据 |
|---|---|---|
| `pnpm test` | passed | 82/82 |
| `pnpm build` | passed | tsc + vite build |
| `cargo test`（src-tauri） | passed | 148 通过，16 个显式 opt-in 测试按设计忽略（包括真实 CLI 和证据生成用例） |
| `cargo clippy --all-targets --all-features -D warnings` | passed | 无警告 |
| `pnpm check`（含 release 文档检查与 fmt） | passed | 重构收口工作树通过，随后提交为 `3dd9a9f`；`check.sh` 使用 `set -e`，全部步骤执行完毕 |
| `pnpm tauri build --no-bundle` | passed | 重构收口时通过；产物 `src-tauri/target/release/loom`。后续前端打磨未重新打包，测试最新工作区请使用下方 debug 启动方式 |
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

sample 保留开发过程中的失败运行。测试总数仅是原始输出中的数量，不能证明运行来自相同提交。重构收口门禁的总数为 164（148 passed + 16 ignored）；各真实 CLI 用例在开发过程中按对应改动定向复验。

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

- 原 sample 仅保留摘要。收尾补充了 [失败原因与复验说明](samples/simple-chat/failure-retests.md) 和 [持久化失败字段](samples/simple-chat/failure-details.json)：区分测试提示词修正、CLI 工具发现超时与停止收口修复，不把后续通过解释为所有自然语言请求都稳定。
- **没有在最终提交上将全部真实 CLI 场景再跑一遍**；记录对应各功能的定向运行，完整离线门禁在收口时执行。后续因实现、CLI 版本、账户状态变化或新失败而复验；已有本任务内授权不需重复确认，但真实调用仍会消耗模型额度。
- OpenCode 结果来自官方 1.19.3 临时副本，来源、SHA-256 和签名见 [opencode-provenance.json](samples/simple-chat/opencode-provenance.json)。用户本机原有安装签名无效（exit 137），保持原样未改动。
- Grok 失败是账户额度问题（402），不能视为适配器回退，也不能视为通过。

## 已知限制（未在本轮解决）

- **桌面专属行为未验证**：`3dd9a9f` 的切换 Agent 与 Auto-run 确认使用 `window.confirm`。后续独立 UI 打磨提交（05:55 计划）已替换为应用内提示条；本记录没有将它计入重构基线的桌面通过证据。2026-10-20 用户在 `scripts/debug.sh desktop` 中手动确认这两处应用内提示条正常（见 [UI 打磨计划](../plans/2026-10-20/05:55-chat-ui-polish.md)）；该结论只属于 UI 打磨提交，P9 整体桌面验收仍未执行。
- **Codex 的 Auto-run 与 Code 参数相同**：都是 `workspace-write; approval_policy=never`。打开 Auto-run 只是跳过 Loom 的回合确认，不改变 CLI 行为。
- **工具卡片元信息不全**：重构基线没有行数、匹配数和耗时，日志入口可能重复。后续 UI 打磨提交已改成每回合一个日志入口；其余元信息仍不在已验收范围内。
- **样本环境不干净**：Codex 样本中出现了本机全局指令（`rtk`、`RTK.md`）的痕迹，录制时没有隔离用户级 CLI 配置。

## 启动用户测试

当前工作区已安装依赖。在项目终端运行：

```bash
cd /Users/guojian/Workspace/naijoug/loom
scripts/debug.sh stop
scripts/debug.sh desktop
```

等待 Tauri 编译结束，打开的窗口标题为 **Loom**，开发服务固定端口 **1420**。此方式加载工作区最新前端，也包含 UI 打磨；不会启动旧 Task 或 Workbench。

建议先用 Claude Code / Codex 和可安全修改的临时项目，按 [用户验收清单](../testing.md#用户验收) 检查：新建与切换会话、三轮追问、停止再发、Research → Code、重启恢复、语言与主题，以及两处应用内确认。

测试结束后，在另一终端执行：

```bash
scripts/debug.sh stop
```

仅查看合成页面可用 `scripts/debug.sh web`，访问 `http://127.0.0.1:1420/?fixture=simple-chat`。它不调用真实 Agent。截图目录会随 UI 打磨更新，不作为重构基线的不可变截图。

## 用户接手项

1. 原生桌面验收由用户执行，再将矩阵中相应 partial 更新为 passed 或记录失败；本轮不自动合入 main。
2. OpenCode 原安装需修复签名后再验收默认路径；官方临时副本的通过不表示原安装已修好。
3. Grok 额度恢复后补测。遇到问题请附会话导出或脱敏执行日志，注明 CLI 版本和模式。

本轮收尾只提交验收文档、两份计划状态和重构依赖/删除清单；独立 UI 打磨及原有架构评审文件保留原工作区状态。
