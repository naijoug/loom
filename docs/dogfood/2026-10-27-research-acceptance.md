# Research 验收记录（2026-10-27）

- 对应计划：[Agent 调用层与左侧功能栏重构计划](../plans/2026-10-26/22:42-agent-layer-and-activity-rail.md) P6；契约见 [Research 契约](../architecture/research-contracts.md)，范围见 [需求文档](../requirements.md#research-范围2026-10-27按计划-8-2-默认决策新增)。
- 基线：`main` @ `005f843` 之上的 P6 工作区改动（提交前验收，未推送）。
- 环境：macOS（Darwin 25.5.0，arm64）；Codex CLI 0.164.0（用户全局安装，未修改）。
- 结论：**工程门禁、合成网页矩阵与 Codex 真实调研（首问 + 原生续接追问）通过；Claude Code 真实调研未运行；桌面人工验收待进行**。

状态口径同 [简化 Chat 验收](2026-10-20-simple-chat-acceptance.md#状态口径)：`passed` / `partial` / `blocked` / `not-tested`。

## 工程门禁

| 检查 | 结果 | 证据 |
|---|---|---|
| `pnpm check`（前端测试/build、分层检查、Rust fmt/clippy/test、release 文档检查） | passed | 前端 140 项（含 `tests/unit/research.test.cjs`），`cargo test --lib` 311 项通过、29 项需显式开启（提交前复跑） |
| `pnpm smoke:visual`（含 Research 矩阵与功能栏 ⌘/Ctrl+3） | passed（synthetic-web-only） | `.designs/exports/research/result.json` 与 15 张截图（不入库） |
| `pnpm tauri build --no-bundle` | not-tested | 本阶段未改 Tauri 配置与打包；P8 完整门禁时运行 |

Research 后端单测（13 项）覆盖：ID 格式、过期“运行中”问题标为已中断而存活作业保留、报告与日志路径不逃逸研究目录、提示词占位与引用规则、回放预算、报告标记提取、首问写报告并校验引用、无项目时在研究工作目录运行、追问原生续接与续接失败后回放、Agent 命令变化时回放、项目被写入只警告、失败与取消记录在问题上、长参数省略。前端 4 项覆盖 Agent 过滤、报告与引用渲染、追问与停止、失败重试、新建参数。

## 合成网页矩阵

DEV 专用 `researchFixture`，不调用 IPC、git 或模型，不能作为桌面或真实模型证据。

| 场景 | 截图 |
|---|---|
| 两问报告、追问上下文标签、失效引用警告 | `ready-*-*.png`（zh-CN / en × 深浅） |
| 源码引用打开查看器并高亮整段范围 | `source-range.png` |
| 作业日志 | `logs.png` |
| 追问运行中 → 第三份报告 | `followup-running.png` |
| 无项目研究不显示源码浏览 | `no-project.png` |
| 运行中、停止、重试 | `running.png`、`cancelled.png` |
| 失败摘要与回放警告；存储错误 | `failed.png`、`error.png` |
| 新建表单 → 创建并回答 | `new-research.png`、`created-ready.png` |
| 700px 侧栏堆叠、无横向溢出 | `ready-zh-narrow.png` |

## 真实 Codex 调研

命令（会消耗额度；项目为 `005f843` 的本地干净克隆，避免工作区未提交改动触发“项目状态变化”警告）：

```bash
LOOM_REAL_RESEARCH=1 LOOM_REAL_RESEARCH_ROOT=<dir> LOOM_REAL_RESEARCH_PROJECT=<clean clone> cargo test --manifest-path src-tauri/Cargo.toml research::real_tests::codex_researches_a_project -- --ignored --nocapture
```

| 项 | 首问 | 追问 |
|---|---|---|
| 问题 | Loom 如何启动本机 Agent CLI，用户取消时如何停止 | 这次运行有哪些限制（超时、输出大小），在哪里设置 |
| 状态 | passed（done） | passed（done） |
| 耗时 | 165 s | 116 s |
| 上下文 | 首问 | `native`（`codex exec … resume <thread>`，未回放历史） |
| 工具调用 | 12 | 4 |
| 引用 | 25 有效 / 0 无效 | 19 有效 / 0 无效 |
| 警告 | 无（运行前后 `git status` 与 HEAD 一致） | 无 |

- 调用为只读：`codex exec --json --skip-git-repo-check --cd <project> --sandbox read-only --config approval_policy="never" -- -`，提示词经 stdin；作业记录、stdout / stderr 日志写在研究目录 `jobs/<job>/`。
- 报告为简体中文，结构为结论 / 证据 / 引用 / 未解决问题。抽查事实与代码一致：`agent/runner.rs` 的 `DEFAULT_TIMEOUT_MS` 10 分钟、输出合计 8 MiB / 20,000 行 / 单行 512 KiB，`process_supervisor.rs` 启动时 `setpgid(0, 0)` 并按进程组发信号。
- 测试结束 `test result: ok. 1 passed`（281.63 s）；克隆项目 `git status --short` 为空。

## 未覆盖与限制

- Claude Code 真实调研：not-tested。`claude_researches_a_project` 已就绪；Learn 验收时本机 Claude 经代理认证失败，本轮未重试。
- 桌面人工验收：not-tested。待按 [测试指南 · Research](../testing.md#research) 检查功能栏进入、真实运行中停止、退出重启后“已中断”显示与重试、删除确认。
- 追问回放路径只由伪 CLI 单测覆盖，真实运行走的是原生续接。
- 无项目（网页 / 常识）调研未做真实运行；其引用只做 `loom-src:` 校验，网页链接不校验。
