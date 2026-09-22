# Loom 用户测试指南

## 启动

```bash
pnpm install
scripts/debug.sh desktop
```

验证结束后运行 `scripts/debug.sh stop`，避免遗留 Vite/Tauri 开发进程。

发布候选的签名、公证、安装与分发结论必须针对具体 artifact 复核，仅在准备发布时阅读 [发布入口](release/README.md)。历史本机 smoke 通过不代表当前候选可分发。

## 开发变更验证矩阵

| 变更范围 | 默认检查 | 扩大条件 |
|---|---|---|
| 文档 | 相对链接、Markdown、需求一致性；计划变更检查索引 | 发布资料运行 `pnpm docs:release:check` 与 `pnpm docs:release:test` |
| 前端局部逻辑/样式 | `pnpm test -- tests/unit/<相关文件>.test.cjs`（逻辑变更）、`pnpm build`；视觉变化查看相关页面 | 共享状态/组件扩大前端回归；IPC 同时验证 Rust |
| Rust 局部逻辑 | `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`、相关 `cargo test --manifest-path src-tauri/Cargo.toml <filter>`、适用 Clippy | 共享模块扩大 Rust 回归；影响桥接或桌面行为时补对应验证 |
| IPC/权限/持久化/进程 | 两端生产路径契约测试、相关失败和恢复用例 | 对实际 CLI/OS 的保证需真实桌面/临时项目验收 |
| 里程碑交付/发布候选 | `pnpm check`、`pnpm tauri build --no-bundle` | 按改动跑相关 Chat/Task 桌面验收；分发另过发布门禁 |

通过相关检查后，仅因新改动、失败或未解决风险扩大或重复验证。新增测试应覆盖行为或回归，不复制实现；文案小改不强制新增测试。`pnpm check` 已包含前端测试/build、Rust fmt/clippy/test 和 release 文档检查，不重复逐项再跑。真实 Agent 调用可能消耗账户额度；已有授权范围内执行，缺登录或环境时记录限制。

## 当前 Chat 验收路径

先运行 `pnpm smoke:chat` 验证生产仓储/runtime 的真实本地 fixture 进程；该命令不请求模型。真实 Grok 三轮 headless canary 的显式 opt-in 命令与证据范围见 [Runtime 验证记录](dogfood/2026-09-20-chat-runtime.md)，不能替代下列桌面、权限与工具验收。

1. 在可安全修改的临时项目检测至少一个本机 CLI，记录版本与登录/能力结果。
2. 创建会话，观察真实流式输出及错误；连续三轮追问，并检查原生 resume 不重复注入历史。
3. 生成中停止后再发，切换会话/项目，确认旧回合不会污染新会话。
4. Ask 取消确认时不发送，确认后才发送；Auto 不增加回合确认。当前 Ask 是 UI 回合授权，不是逐工具审批。
5. 在临时目录验证各 CLI 的真实只读/可写限制及 resume 一致性；仅看到 flag 不能标记安全验收通过。
6. 重命名、归档/恢复、搜索，重启后读取转录；中断和失败保留可读状态。无原生 resume 的长历史应有预算/截断提示，最新请求完整。
7. 记录 Loom commit、CLI 版本、权限、cwd、退出状态和证据。未实现目标记 failed/not tested，不用 fixture 填 passed。

目标细节和现有缺口见 [Chat 契约](architecture/chat-contracts.md)；提示词行为比较见 [Agent 行为验收](guides/agent-behavior-evaluation.md)。单 Agent Chat 验收不要求下列高级 Task 流程。

## 高级 Task 工作流验收路径

1. 打开一个可安全修改的本地 Git 项目。
2. 在 Settings → Agents 确认至少两个可用且已登录的不同 Agent；为 planning、implementation、review、debugging/testing 设置项目默认值。
3. 创建任务并选择多个 Planning Agent，观察起草、交叉 Review、合成和计划证据。
4. 确认计划，按 Todo 让主 Agent 实施；尝试切换主 Agent，确认必须填写原因。
5. 使用不同 Agent 执行实施 Review。创建 blocker 时确认 Testing 被阻止；修复并重审后再进入 Testing。
6. 配置一个会失败的 validation command，开启 Auto，观察“失败→修复→重跑”；至少完成两轮后再通过。
7. 在 Manual 模式输入问题、复现步骤和期望行为，引用一段日志并附加一张截图；确认反馈出现在会话和任务时间线。
8. 让最新 validation 成功并验收。完成页应显示真实 changed files、numstat、Review、验证、风险与建议。
9. 打开/定位 `summary.md`，分别导出 Markdown 与 JSON；重启 Loom 后确认 Task、历史日志和总结仍可读取。
10. 在测试项目中保留一处实施前脏改动，确认总结将其标为 `pre_existing`。

## 高级 Task 安全用例

- 尝试在项目外 cwd 运行命令，应被拒绝。
- 安装依赖按用户确认设置处理；删除、破坏性 `git reset/clean` 或生产部署按安全策略要求审批。
- 停止长时间 PTY，确认整个进程组退出。
- 暂停/阻塞/取消运行中的任务，确认关联命令停止且原因进入时间线。

## 报告问题

请附上：Loom 版本、操作系统、项目技术栈、会话/回合 id 或任务 id、复现步骤与期望行为。Task 附 `.loom/logs/<task-id>/` 中相关 run 的脱敏日志；Chat 可从回合“执行记录”查看 stdout/stderr，或从会话菜单导出 JSON/Markdown/manifest 和已有日志。v2 会话日志位于 `.loom/chat/v2/sessions/<id>/events.jsonl`，独立执行日志位于同目录的 `runs/<turnId>/stdout.log` 与 `stderr.log`。旧回合可能没有独立日志，缺失情况由导出 manifest 明示。转录包含对话内容，分享前必须检查并脱敏；不要上传项目凭据或未脱敏的 `.env`。

完整自动回归可运行 `pnpm e2e:complete`；加上 `LOOM_E2E_RELEASE=1` 会同时构建 Tauri 发布产物。
