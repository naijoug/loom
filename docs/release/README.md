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

## 使用顺序

1. 先读 `beta-scope.md`，确认自己是否属于本轮目标试用者。
2. 再读 `beta-safety-notes.md`，确认不会把 Loom 用在生产关键仓库或含机密材料的任务中。
3. 按 `macos-install.md` 完成安装，并记录 Gatekeeper 或权限提示。
4. 按 `beta-smoke.md` 在临时项目里完成首次启动 smoke。
5. 如需卸载或清理，按 `local-data-and-uninstall.md` 处理项目内和全局数据。
6. 完成试用后按 `beta-feedback-template.md` 回传反馈。

## 发布门禁

这些文档覆盖邀请制 Beta 的 M0 边界与 M2 安装/卸载/smoke 路径。公开下载、签名/公证、隐私说明、Agent 账号边界和诊断包脱敏复核仍需后续里程碑补齐；在这些材料完成前，不应把 DMG 作为公开稳定版分发。
