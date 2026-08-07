# Loom Beta 发布资料

本目录记录 Loom 从个人 dogfood 稳定版进入邀请制 Beta 前需要给试用者阅读、确认和回传的信息。

## M0 三件套

- [Beta 范围说明](beta-scope.md)：说明适合谁试用、支持平台、已知限制、非目标和退出条件。
- [Beta 安全说明](beta-safety-notes.md)：说明试用者需要承担的本机项目、命令执行、真实 Agent 账号和机密信息边界。
- [Beta 反馈模板](beta-feedback-template.md)：把反馈统一归类为 blocker / bug / confusion / request / security，并要求携带可复核证据。

## M2 安装与首次启动

- [macOS Beta 安装说明](macos-install.md)：说明 DMG / App bundle 安装、Gatekeeper 处理原则和首次启动检查。
- [本地数据与卸载清理说明](local-data-and-uninstall.md)：列出项目内 `.loom/`、全局 app data、诊断包和完全清理步骤。
- [Beta 首次启动 Smoke](beta-smoke.md)：提供一个不依赖真实 Agent 凭据的 15 分钟临时项目验证流程。

## M3 隐私、账号与诊断包边界

- [Beta 隐私说明](privacy-note.md)：说明默认本地数据、可能离开本机的路径、诊断包脱敏边界和隐私停止线。
- [Agent / CLI 账号边界](agent-account-boundary.md)：说明 dummy、Codex、Claude Code、OpenClaw、Hermes 和自定义 CLI 的账号、费用、数据发送责任。
- [诊断包安全复核](diagnostic-bundle-review.md)：说明诊断包内容、已有脱敏证据、导出前自查、接收方处理规则和 Beta 发布门禁。

## 使用顺序

1. 先读 `beta-scope.md`，确认自己是否属于本轮目标试用者。
2. 再读 `beta-safety-notes.md`，确认不会把 Loom 用在生产关键仓库或含机密材料的任务中。
3. 继续读 `privacy-note.md`、`agent-account-boundary.md` 和 `diagnostic-bundle-review.md`，确认隐私、诊断包和真实 Agent 账号责任边界。
4. 按 `macos-install.md` 完成安装，并记录 Gatekeeper 或权限提示。
5. 按 `beta-smoke.md` 在临时项目里完成首次启动 smoke；首次 smoke 默认使用 dummy / fixture，不要求真实付费 Agent。
6. 如需卸载或清理，按 `local-data-and-uninstall.md` 处理项目内和全局数据。
7. 完成试用后按 `beta-feedback-template.md` 回传反馈。

## 发布门禁

这些文档覆盖邀请制 Beta 的 M0 边界、M2 安装/卸载/smoke 路径，以及 M3 隐私、Agent 账号责任和诊断包安全复核边界。公开下载、签名/公证、可复现 release build 和正式 UI 导出诊断包 smoke 仍需后续里程碑补齐；在这些材料完成前，不应把 DMG 作为公开稳定版分发。
