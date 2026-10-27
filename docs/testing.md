# Loom 测试与验收

启动桌面使用 `scripts/debug.sh desktop`，网页使用 `scripts/debug.sh web`。启动前和结束后都执行 `scripts/debug.sh stop`，固定端口 1420。

## 开发变更验证矩阵

| 范围 | 默认检查 | 扩大条件 |
|---|---|---|
| 文档 | 相对链接、需求一致性；计划同轮更新索引 | 发布资料运行 release 文档检查 |
| 前端局部逻辑/样式 | 相关 `pnpm test -- tests/unit/<file>.test.cjs`、`pnpm build`；视觉变化查看页面 | 共享状态扩大前端回归 |
| Rust 局部逻辑 | fmt、相关 cargo test、适用 Clippy | 共享模块扩大 Rust 回归 |
| IPC/权限/持久化/进程 | 两端契约、失败与恢复、进程 fixture | CLI/OS 保证必须补真实运行 |
| 里程碑 | `pnpm check`、`pnpm tauri build --no-bundle` | 按目标补真实桌面验收 |
| 冻结内核 | 手动选择 `pnpm check:core` | 不属于默认 Chat 门禁 |

`pnpm check` 包含前端测试/build、Rust fmt/clippy/test 和 release 文档检查。通过后仅因新改动、失败或未解决风险扩大检查。真实模型测试可能消耗账户额度；未运行、登录失败或额度不足分别记录，不写成 passed。

## 工程与浏览器检查

- `pnpm smoke:chat`：生产仓储/runtime 运行真实本地 fixture 子进程，验证双路输出、UTF-8、停止/超时、进程组清理、幂等和恢复；不请求模型。
- `pnpm smoke:visual`：启动合成网页并运行 Chrome 交互；检查两个项目、新会话的模式与 Agent、发送确认、工具卡片、深浅主题、中英文与窄窗口。截图在 `designs/exports/simple-chat/`。同轮还运行 Skills 视觉矩阵（显式 DEV fixture，截图 `designs/exports/skills/`），包括来源状态、安装/翻译、命令面板和 Chat 草稿往返；以及 Learn 视觉矩阵（`learnFixture`，截图 `designs/exports/learn/`）：Wiki 目录与页面、源码范围高亮、架构图跳转、无效引用、未生成页、作业日志、运行中/失败/存储错误、新建表单校验与生成、中英文 × 深浅主题、700px 窄窗口。这不是桌面或真实模型证据。
- 真实 CLI 的 opt-in 测试在 `src-tauri/src/chat/real_agent_tests.rs`。例如 `LOOM_REAL_CHAT=1 cargo test --manifest-path src-tauri/Cargo.toml claude_three_turns_resume_after_repository_reopen -- --ignored --nocapture`。
- Learn 真实研究的 opt-in 测试在 `src-tauri/src/learn/real_tests.rs`，走与桌面命令相同的作业流水线（拉取 → 架构 → 全部页面与术语表）并打印阶段耗时和引用统计，会消耗额度：`LOOM_REAL_LEARN=1 LOOM_REAL_LEARN_ROOT=<dir> cargo test --manifest-path src-tauri/Cargo.toml learn::real_tests::codex_studies_hexyl -- --ignored --nocapture`。可用 `LOOM_REAL_LEARN_URL`、`LOOM_REAL_LEARN_MODEL`、`LOOM_REAL_LEARN_CODEX` / `LOOM_REAL_LEARN_CLAUDE`（CLI 路径）覆盖默认值。

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

1. 侧边栏点 Learn，Chat 草稿与当前会话保持；返回 Chat 仍在原处。
2. 新建研究：输入 `https://github.com/sharkdp/hexyl`，选 Claude Code 或 Codex，可留空模型；非法 URL 或模型名不可提交。
3. 进度依次经过 Download → Inventory → Architecture → Level 1 页面；可查看作业日志（输出 / 错误 / 提示词），运行中可取消，失败后可重试。
4. 打开 Overview / Architecture：架构图节点可点击进入模块页；“生成剩余”补齐其余模块页与术语表。
5. 点击页面中的源码引用，右侧查看器定位并高亮对应行；无效引用显示警告样式；可浏览文件树，并在 GitHub 打开固定 commit。
6. 生成全程 checkout 不变（指纹检测失败会使作业失败）；删除研究需二次确认，仅删除应用数据目录中的该研究。

Learn 证据与限制见 [Learn 验收记录](dogfood/2026-10-26-learn-acceptance.md)。

逐项证据与当前环境限制见 [本轮记录](dogfood/2026-10-20-simple-chat-acceptance.md)。OpenCode / Grok 未通过的真实场景不得用合成数据代替。

## 报告问题

附 Loom 版本、CLI 版本、项目/会话/回合 ID、模式、复现步骤和期望结果。工具卡片或错误信息可打开执行日志；也可导出会话或设置中的诊断包。分享前检查并脱敏，不上传凭据、`.env` 或无关私人转录。

发布候选仍需针对具体 artifact 完成签名、公证、安装与升级验证，见 [发布入口](release/README.md)。
