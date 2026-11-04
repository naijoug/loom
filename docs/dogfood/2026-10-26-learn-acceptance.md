# Learn 验收记录（2026-10-26）

- 对应计划：[Learn 实施计划](../plans/2026-10-26/19:26-learn-repo-study-module.md) L7；契约见 [Learn 契约](../architecture/learn-contracts.md)，L0 见 [探针记录](2026-10-26-learn-probe.md)。
- 基线：`main` @ `d5b8c4b` 之上的未提交工作区改动（未提交、未推送）。
- 环境：macOS（Darwin 25.5.0，arm64）；Codex 0.153.0（VS Code agent-host 缓存二进制，未修改用户全局安装）；Claude Code 2.1.309（经 `~/.claude/settings.json` 指向本机代理）。
- 结论：**工程门禁、合成网页矩阵与 Codex 真实研究通过；Claude Code 真实研究 blocked（本机 CLI 认证失败）；用户桌面验收待进行**。

状态口径同 [简化 Chat 验收](2026-10-20-simple-chat-acceptance.md#状态口径)：`passed` / `partial` / `blocked` / `not-tested`。

## 工程门禁

| 检查 | 结果 | 证据 |
|---|---|---|
| `pnpm check`（前端测试/build、Rust fmt/clippy/test、release 文档检查） | passed | 前端 130 项（含 `tests/unit/learn.test.cjs`），`cargo test --lib` 290 项通过、27 项需显式开启（2026-10-26 启动前检查修复后复跑） |
| `pnpm tauri build --no-bundle` | passed | `src-tauri/target/release/loom` |
| `pnpm smoke:visual`（含 Learn 矩阵） | passed（synthetic-web-only） | `.designs/exports/learn/result.json` 与 15 张截图 |

Learn 后端单测覆盖：URL / ref 校验、加固 git 拉取与上限、清单、源码读取的路径逃逸与 symlink、研究存储与中断恢复、架构 JSON 提取与校验、引用解析、作业流水线（伪 CLI：成功、坏 JSON 修复后失败、checkout 被写入检出、取消、计划）。

## 合成网页矩阵

DEV 专用 `learnFixture`，不调用 IPC、git 或模型，不能作为桌面或真实模型证据。

| 场景 | 截图 |
|---|---|
| 就绪研究、Overview 填满窗口 | `ready-en-light.png` |
| 源码引用打开查看器并高亮 10–14 行 | `source-range.png` |
| 架构图（节点可点击进入模块页） | `architecture.png` |
| 模块页、无效引用警告样式 | `module-page.png` |
| Wiki 链接指向未生成页面 | `page-not-generated.png` |
| 作业日志 | `logs.png` |
| 中英文 × 深浅主题 | `ready-{en,zh-CN}-{light,dark}.png` |
| 700px 窄窗口（侧边栏堆叠） | `ready-zh-narrow.png` |
| 运行中 / 失败 / 存储错误 | `running.png` / `failed.png` / `error.png` |
| 新建表单（非法 URL 禁止提交）与生成完成 | `new-study.png` / `created-ready.png` |

本轮视觉检查发现并修复两处问题：`.learn-page` 同时用于根容器和文章区导致根容器继承内边距、高度未填满（文章区改名 `learn-article`，Learn 使用 `ChatShell` 的 100vh 外壳）；窄窗口下共享的 `.chat-project-tree` 规则把 Learn 侧边栏变为无关闭控件的浮层（改为在 760px 以下堆叠到顶部）。

## 真实 CLI 研究

命令（显式开启，消耗额度；清除宿主会话继承的 `ANTHROPIC_*` / `CLAUDE_*` 变量，等同桌面应用启动环境）：

```bash
LOOM_REAL_LEARN=1 LOOM_REAL_LEARN_ROOT=<scratch> LOOM_REAL_LEARN_CODEX=<codex 路径> cargo test --manifest-path src-tauri/Cargo.toml learn::real_tests::codex_studies_hexyl -- --ignored --nocapture
```

### Codex — passed

`sharkdp/hexyl` @ `6ecc29b9`，语言 zh-CN，深度 standard，CLI 默认模型。两个作业：新建链路（fetch → inventory → architecture → Level 1 页）与 `all`（其余模块页 + 术语表）。

| 阶段 | 耗时 | 有效 / 无效引用 |
|---|---|---|
| fetch / inventory | 2 s / 0 s | —（清单排除 1 个二进制或超限文件） |
| architecture | 182 s | Overview 5/0，Architecture 37/0 |
| 8 个模块页 | 62–76 s / 页 | 12–21 / 0 每页 |
| glossary | 95 s | 38/0 |
| 合计 | 825 s（两个作业 324 s + 501 s），工具调用 27 + 45 | 211/0 |

- 架构：8 个模块（L1 2 个、L2 3 个、L3 3 个），带依赖关系；源码覆盖 7/7。
- 页面：中文，由浅入深讲解，引用落在正确行段（抽查 `command-line` 页的参数模型、输入分支、跳过与限长）。
- 只读：每次运行前后指纹一致，结束时 checkout `git status` 干净、HEAD 仍为固定 commit。
- 无修复回合、无警告。

### Claude Code — blocked

架构阶段立即失败：`[claude-code:unrecognized_model] {"model":"auto/glm-5.3[1m]"}`，stream 中 401 `authentication_failed`。在同一环境下脱离 Loom 直接运行 `claude -p --output-format json --permission-mode plan` 得到相同错误，本机代理端口可连通。判断为本机 Claude CLI / 代理认证状态问题，非 Loom 缺陷；L0 当天早些时候同一配置可用。未修改用户的 Claude 或代理配置。恢复认证后可用 `learn::real_tests::claude_studies_hexyl` 重跑。

OpenCode / Grok 本轮未运行真实研究（not-tested）；L0 记录了它们的架构阶段表现与限制。

## 用户桌面验收

partial。步骤见 [测试指南](../testing.md#learn)。

- 2026-10-26 用户用 Codex 研究 `SWE-agent/mini-swe-agent`：下载、清单通过，架构阶段失败，原因是本机全局 Codex 0.164.0 缺少平台包 `@openai/codex-darwin-arm64`（`codex --version` 同样报错，属于环境问题）。暴露的 Loom 缺陷已修复：
  - 创建研究和重试前先运行 `<agent> --version`（`agent_diagnostics::check_startable`），坏掉的安装在启动前就给出一行摘要；
  - 原始 stderr 收进可展开的详细信息（`ErrorText`）。
  - 已有单测（伪造坏 CLI / 好 CLI、`splitError`），合成 fixture 也看过。
- 桌面上的完整研究流程待用户修复 Codex 安装后继续。
