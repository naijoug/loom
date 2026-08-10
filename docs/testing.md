# Loom 用户测试指南

## 启动

```bash
pnpm install
scripts/start-local.sh desktop
```

验证结束后运行 `scripts/start-local.sh stop`，避免遗留 Vite/Tauri 开发进程。

也可以直接运行本轮生成的本地发布版：

```text
src-tauri/target/release/bundle/macos/Loom.app
src-tauri/target/release/bundle/dmg/Loom_0.1.0_aarch64.dmg
```

当前产物没有 Apple Developer ID 签名与公证，适合本机开发验收，不应作为已公证的公开下载包分发；如要推进邀请制 Beta，先阅读 `docs/release/README.md`、`docs/release/beta-scope.md`、`docs/release/beta-safety-notes.md`、`docs/release/beta-feedback-template.md`、`docs/release/install-uninstall-smoke-record.md`、`docs/release/beta-first-run-smoke-record.md`、`docs/release/diagnostic-bundle-smoke-record.md` 和 `docs/release/beta-release-review-checklist.md`，并确认同一候选 artifact 的安装 / 卸载记录、首次启动记录、诊断包真实桌面导出记录与分发结论不再是 `Distribution decision: Hold`。

## 建议验收路径

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

## 安全用例

- 尝试在项目外 cwd 运行命令，应被拒绝。
- 安装依赖、删除、`git reset/clean` 或生产部署应要求显式审批。
- 停止长时间 PTY，确认整个进程组退出。
- 暂停/阻塞/取消运行中的任务，确认关联命令停止且原因进入时间线。

## 报告问题

请附上：Loom 版本、操作系统、项目技术栈、任务 id、复现步骤、期望行为，以及 `.loom/logs/<task-id>/` 中相关 run 的脱敏日志。不要上传项目凭据或未脱敏的 `.env`。

完整自动回归可运行 `pnpm e2e:complete`；加上 `LOOM_E2E_RELEASE=1` 会同时构建 Tauri 发布产物。
