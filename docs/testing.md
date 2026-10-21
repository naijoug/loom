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
- `pnpm smoke:visual`：启动合成网页并运行 Chrome 交互；检查两个项目、新会话的模式与 Agent、发送确认、工具卡片、深浅主题、中英文与窄窗口。截图在 `designs/exports/simple-chat/`。这不是桌面或真实模型证据。
- 真实 CLI 的 opt-in 测试在 `src-tauri/src/chat/real_agent_tests.rs`。例如 `LOOM_REAL_CHAT=1 cargo test --manifest-path src-tauri/Cargo.toml claude_three_turns_resume_after_repository_reopen -- --ignored --nocapture`。

## 用户验收

1. 启动桌面，进入 Settings → Agents，核对路径、版本、启用状态；缺失 CLI 不应可选。安装与登录仍在 CLI 中完成。
2. 添加两个可安全修改的项目；分别新建会话，验证分组、折叠、搜索、重启后历史和折叠状态。
3. 新会话只选 Research / Code；在输入框左下选择 Agent，发送后核对使用的 Agent。Enter 发送、Shift+Enter 换行，IME 确认不发送。
4. Claude Code / Codex 连续三轮追问；检查流式文字、读文件和命令工具卡片、退出码，以及 CLI 提供的编辑差异。
5. 在生成中按 Esc 停止后再发；切换会话与项目，旧输出不能污染当前视图。正常退出再启动后继续会话。
6. Research 要求修改测试文件，确认文件保持不变；切换 Code 后经回合确认完成修改。自动执行首次开启提示一次，不表示逐工具审批。
7. 换 Agent 提示清除原生续聊；确认后保留可见历史并使用有界重放。无原生续聊支持时会话“更多操作”菜单底部显示 History replay。
8. 重命名、归档/恢复、清除续聊、导出 JSON/Markdown 与执行日志；日志失败或缺失必须可见。
9. Settings → Appearance 切换简体中文与主题，立即生效且重启保留；从设置返回仍停留在先前会话。

逐项证据与当前环境限制见 [本轮记录](dogfood/2026-10-20-simple-chat-acceptance.md)。OpenCode / Grok 未通过的真实场景不得用合成数据代替。

## 报告问题

附 Loom 版本、CLI 版本、项目/会话/回合 ID、模式、复现步骤和期望结果。工具卡片或错误信息可打开执行日志；也可导出会话或设置中的诊断包。分享前检查并脱敏，不上传凭据、`.env` 或无关私人转录。

发布候选仍需针对具体 artifact 完成签名、公证、安装与升级验证，见 [发布入口](release/README.md)。
