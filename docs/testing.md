# Loom 测试与验收

启动桌面使用 `scripts/debug.sh desktop`，网页使用 `scripts/debug.sh web`。启动前和结束后都执行 `scripts/debug.sh stop`，固定端口 1420。

## 开发变更验证矩阵

| 范围 | 默认检查 | 扩大条件 |
|---|---|---|
| 文档 | 相对链接、需求一致性；计划同轮更新索引 | 发布资料运行 release 文档检查 |
| 前端局部逻辑/样式 | `pnpm format`、相关 `pnpm test -- tests/unit/<file>.test.cjs`、`pnpm build`；视觉变化查看页面 | 共享状态扩大前端回归 |
| Rust 局部逻辑 | fmt、相关 cargo test、适用 Clippy | 共享模块扩大 Rust 回归 |
| IPC/权限/持久化/进程 | 两端契约、失败与恢复、进程 fixture | CLI/OS 保证必须补真实运行 |
| 里程碑 | `pnpm check`、`pnpm tauri build --no-bundle` | 按目标补真实桌面验收 |
| 冻结内核 | 手动选择 `pnpm check:core` | 不属于默认 Chat 门禁 |

`pnpm check` 包含 Prettier 格式检查（`src`、`scripts`、`tests`，冻结 Workbench 与 fixture 见 `.prettierignore`）、前端测试/build、Rust fmt/clippy/test 和 release 文档检查。通过后仅因新改动、失败或未解决风险扩大检查。真实模型测试可能消耗账户额度；未运行、登录失败或额度不足分别记录，不写成 passed。

## 工程与浏览器检查

- `pnpm smoke:chat`：生产仓储/runtime 运行真实本地 fixture 子进程，验证双路输出、UTF-8、停止/超时、进程组清理、幂等和恢复；不请求模型。
- `pnpm smoke:visual`：启动合成网页并运行 Chrome 交互；检查两个项目、新会话的模式与 Agent、发送确认、工具卡片、深浅主题、中英文与窄窗口。截图在 `designs/exports/simple-chat/`。同轮运行左侧功能栏矩阵（zh/en × 深浅 × 700px：宽度、当前项、快捷键切换、无横向溢出；1280px 下打开会话写草稿、切到 Learn 再回来仍保持），截图 `designs/exports/activity-rail/`；还运行 Skills 视觉矩阵（显式 DEV fixture，截图 `designs/exports/skills/`），包括来源状态、安装/翻译、命令面板和 Chat 草稿往返；以及 Learn 视觉矩阵（`learnFixture`，截图 `designs/exports/learn/`）：Wiki 目录与页面、源码范围高亮、架构图跳转、无效引用、未生成页、作业日志、运行中/失败/存储错误、新建表单校验与生成、中英文 × 深浅主题、700px 窄窗口，以及学习路径（已读 / 继续阅读 / 翻页 / 按页生成）、架构图悬停与 `learnFixture=large`（10 模块 / 4 层）全名与默认无边、真实按键的菜单 / 弹层 / 抽屉 / 架构图键盘操作；以及 Research 视觉矩阵（`researchFixture`，截图 `designs/exports/research/`）：问答与报告、追问上下文标签、源码范围高亮与无效引用、追问运行与完成、无项目研究、停止与重试、失败摘要与回放警告、存储错误、新建表单、中英文 × 深浅主题、700px 窄窗口。功能栏矩阵同时检查 ⌘/Ctrl+3 进入 Research。这不是桌面或真实模型证据。
- 真实 CLI 的 opt-in 测试在 `src-tauri/src/chat/real_agent_tests.rs`。例如 `LOOM_REAL_CHAT=1 cargo test --manifest-path src-tauri/Cargo.toml claude_three_turns_resume_after_repository_reopen -- --ignored --nocapture`。
- Learn 真实研究的 opt-in 测试在 `src-tauri/src/learn/real_tests.rs`，走与桌面命令相同的作业流水线（拉取 → 架构 → 全部页面与术语表）并打印阶段耗时和引用统计，会消耗额度：`LOOM_REAL_LEARN=1 LOOM_REAL_LEARN_ROOT=<dir> cargo test --manifest-path src-tauri/Cargo.toml learn::real_tests::codex_studies_hexyl -- --ignored --nocapture`。可用 `LOOM_REAL_LEARN_URL`、`LOOM_REAL_LEARN_MODEL`、`LOOM_REAL_LEARN_CODEX` / `LOOM_REAL_LEARN_CLAUDE`（CLI 路径）覆盖默认值。
- Research 真实调研的 opt-in 测试在 `src-tauri/src/research/real_tests.rs`，走与桌面命令相同的作业（首问 + 一次追问，默认研究本仓库）并打印耗时、上下文、引用统计、警告与报告，会消耗额度：`LOOM_REAL_RESEARCH=1 LOOM_REAL_RESEARCH_ROOT=<dir> cargo test --manifest-path src-tauri/Cargo.toml research::real_tests::codex_researches_a_project -- --ignored --nocapture`。可用 `LOOM_REAL_RESEARCH_PROJECT`、`_QUESTION`、`_FOLLOWUP`、`_MODEL`、`_LANGUAGE`、`_CODEX` / `_CLAUDE`（CLI 路径）覆盖默认值；在有未提交改动的工作区上运行时，自己的编辑会触发“项目状态变化”警告，可指向一个干净的本地克隆。

## 用户验收

1. 启动桌面，进入 Settings → Agents，核对路径、版本、启用状态；缺失 CLI 不应可选。安装与登录仍在 CLI 中完成。
2. 添加两个可安全修改的项目；分别新建会话，验证分组、折叠、搜索、重启后历史和折叠状态。
3. 新会话只选 Read-only / Ask / Auto 权限；在输入框左下选择 Agent，发送后核对使用的 Agent。Enter 发送、Shift+Enter 换行，IME 确认不发送。
4. Claude Code / Codex 连续三轮追问；检查流式文字、读文件和命令工具卡片、退出码，以及 CLI 提供的编辑差异。
5. 在生成中按 Esc 停止后再发；切换会话与项目，旧输出不能污染当前视图。正常退出再启动后继续会话。
6. Read-only 下要求修改测试文件，确认文件保持不变；切换 Ask 后经回合确认完成修改。自动执行首次开启提示一次，不表示逐工具审批。
7. 换 Agent 提示清除原生续聊；确认后保留可见历史并使用有界重放。无原生续聊支持时会话“更多操作”菜单底部显示 History replay。
8. 重命名、归档/恢复、清除续聊、导出 JSON/Markdown 与执行日志；日志失败或缺失必须可见。
9. Settings → Appearance 切换简体中文与主题，立即生效且重启保留；从设置返回仍停留在先前会话。

### Learn

1. 左侧功能栏点 Learn（或 ⌘2），Chat 草稿与当前会话保持；点 Chat（或 ⌘1）返回仍在原处。
2. 新建研究：输入 `https://github.com/sharkdp/hexyl`，选 Claude Code 或 Codex，可留空模型；非法 URL 或模型名不可提交。
3. 进度依次经过 Download → Inventory → Architecture → Level 1 页面；可查看作业日志（输出 / 错误 / 提示词），运行中可取消，失败后可重试。
4. 打开 Overview / Architecture：架构图节点可点击进入模块页；“生成剩余”补齐其余模块页与术语表。
5. 点击页面中的源码引用，右侧查看器定位并高亮对应行；无效引用显示警告样式；可浏览文件树，并在 GitHub 打开固定 commit。
6. 生成全程 checkout 不变（指纹检测失败会使作业失败）；删除研究需二次确认，仅删除应用数据目录中的该研究。

Learn 证据与限制见 [Learn 验收记录](dogfood/2026-10-26-learn-acceptance.md)。

### Research

1. 左侧功能栏点 Research（或 ⌘3），Chat 草稿与当前会话保持。
2. 新建研究：输入问题，选 Claude Code 或 Codex，可留空模型，选一个已打开的项目；空问题不可提交。另建一个不选项目的研究。
3. 运行中可停止；停止或失败后可重试最后一问；可查看作业日志（输出 / 错误 / 提示词）。
4. 报告含结论、依据、引用和未解决问题；点击源码引用，右侧查看器定位并高亮对应行，无效引用显示警告样式。
5. 追问一次：标签显示“延续 Agent 会话”或“已回放此前问答”；回答结合了上一问。
6. 研究期间项目 `git status` 与 HEAD 不变（变化会显示警告）；退出再启动后报告仍可阅读，运行中被打断的问题显示“已中断”并可重试；删除研究需二次确认。

Research 证据与限制见 [Research 验收记录](dogfood/2026-10-27-research-acceptance.md)。

逐项证据与当前环境限制见 [本轮记录](dogfood/2026-10-20-simple-chat-acceptance.md)。OpenCode / Grok 未通过的真实场景不得用合成数据代替。

## 报告问题

附 Loom 版本、CLI 版本、项目/会话/回合 ID、模式、复现步骤和期望结果。工具卡片或错误信息可打开执行日志；也可导出会话或设置中的诊断包。分享前检查并脱敏，不上传凭据、`.env` 或无关私人转录。

发布候选仍需针对具体 artifact 完成签名、公证、安装与升级验证，见 [发布入口](release/README.md)。

## Discuss（P7）

- `cargo test --manifest-path src-tauri/Cargo.toml discuss::`：伪 CLI 的实际并发/轮次屏障、部分失败、坏安装、取消、单项重试与过时传播、重启恢复、参数确认和路径保护；不调用模型。
- `pnpm test -- tests/unit/discuss.test.cjs tests/unit/activityRail.test.cjs tests/unit/contract.test.cjs`：参与者和模型、创建/重试调用确认、成功报告保留、过时汇总、删除确认、功能栏与 IPC 契约。
- `pnpm smoke:visual` 同轮运行 `scripts/discuss-smoke.mjs`：轮次、源码引用、日志、部分失败/停止/重试、过时汇总、创建确认、存储失败、en/zh-CN × 深浅、700px，以及 Chat 草稿切页保持；功能栏矩阵补 ⌘/Ctrl+4。截图在 `designs/exports/discuss/`，是合成网页证据。
- 真实用例：`LOOM_REAL_DISCUSS=1 LOOM_REAL_DISCUSS_ROOT=<dir> cargo test --manifest-path src-tauri/Cargo.toml discuss::real_tests::codex_and_claude_complete_two_rounds_and_summary -- --ignored --nocapture`。默认无项目、两个真实 CLI、预计 5 次调用，消耗额度。可指定 `LOOM_REAL_DISCUSS_CODEX` / `_CLAUDE` 二进制和 `_CODEX_MODEL` / `_CLAUDE_MODEL`。必须同时记录两家成功或具体阻塞，不以单家或 fixture 冒充完整真实通过。

桌面人工验收：进入第 4 项、新建 2–4 人讨论、核对模型与调用次数确认；真实进行中切到 Chat 再回来、停止全部、逐项重试、读取旧尝试；退出重启后检查报告与中断项；检查源码引用及删除确认。真实桌面未运行不得以 smoke 代替。

## P8 总体验收（2026-10-28）

公共 Agent 生命周期与错误/缓存契约见 [Agent 层](architecture/agent-layer.md)，按 S1–S8 的证据与例外汇总见 [P8 验收记录](dogfood/2026-10-28-p8-acceptance.md)。完整门禁为 `pnpm check`、`pnpm tauri build --no-bundle`、`pnpm smoke:visual`；真实模型和原生 GUI 的基线/日期单列，历史通过不自动覆盖改过的行为。

原生验收通过 `pnpm tauri build --bundles app` 生成本仓库的准确产物，再使用 `LOOM_PREVIEW_BINARY` 与 `scripts/debug.sh desktop` 启动；前后都 `debug.sh stop`。使用临时 `LOOM_PROFILE_DIR` 仅隔离设置、Agent 配置、最近项目和默认 Skills 库，**不隔离 WebView localStorage 或 Learn/Research/Discuss 应用数据**。功能测试只新建带 P8 前缀的记录；已有学习资料仅阅读，不生成、不删除。临时项目必须通过项目注册得到 canonical 路径；不要直接把 `/var` 非规范路径写入项目列表冒充已登记项目。

运行中切换所有功能再回 Chat，观察真实完成；测试 Chat 停止后再发、重启后读取与续聊；Discuss 验证调用确认、轮次、停止/单项重试、重启报告和旧尝试日志；Research 验证正常退出的 interrupted 状态、重启不自动调用、显式重试与停止后短追问。原生删除只检查确认/取消，永久删除不作为自动化收尾。保留一次空白 WebView 等异常观察，不把后续成功解释为根因已解决。

## 原生启动就绪与恢复（2026-10-28）

- `pnpm smoke:desktop` 必须等当前启动的 `interface_ready`，不再只以进程存活四秒判通过。通过 debug.sh 启停，失败/恢复耗尽会退出非零；它验证原生 IPC 就绪，像素与可访问性内容仍需原生观察。
- `cargo test --manifest-path src-tauri/Cargo.toml webview_startup::`、`pnpm test -- tests/unit/appStartup.test.cjs` 覆盖启动重试/代际隔离/已就绪不自动刷新/可见兜底。
- 故障注入：先 `pnpm build`，再 `node scripts/webview-startup-fixture.mjs once` 或 `always`，按脚本输出构建测试包；前者只跳过第一次入口，后者一直跳过。不得把故障包当交付产物，结束后 `pnpm tauri build --bundles app` 恢复正常包。
- 证据和已知限制见 [重启恢复验收](dogfood/2026-10-28-webview-restart.md)。五次正常观察不代表已经确认原始偶发底层原因。
