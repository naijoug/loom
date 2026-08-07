# Loom Beta 发布资料

本目录记录 Loom 从个人 dogfood 稳定版进入邀请制 Beta 前需要给试用者阅读、确认和回传的信息。

## M0 三件套

- [Beta 范围说明](beta-scope.md)：说明适合谁试用、支持平台、已知限制、非目标和退出条件。
- [Beta 安全说明](beta-safety-notes.md)：说明试用者需要承担的本机项目、命令执行、真实 Agent 账号和机密信息边界。
- [Beta 反馈模板](beta-feedback-template.md)：把反馈统一归类为 blocker / bug / confusion / request / security，并要求携带可复核证据。

## M1 可复现构建

- [Release Build Record](release-build-record.md)：记录 Loom 0.1.0 邀请制 Beta 候选构建的 commit、工具链版本、构建命令、DMG 路径、checksum 和剩余发布门禁。
- [Artifact Integrity Check](artifact-integrity-check.md)：复核候选 DMG 的 checksum、只读挂载、bundle 元数据和当前签名 / Gatekeeper 状态。
- [Signing Gate Decision](signing-gate-decision.md)：记录当前候选 DMG 因 invalid-signature / Gatekeeper hold 不进入公开或默认邀请制分发，并给出下一步签名修复路径。

## M2 安装与首次启动

- [macOS Beta 安装说明](macos-install.md)：说明 DMG / App bundle 安装、Gatekeeper 处理原则和首次启动检查。
- [本地数据与卸载清理说明](local-data-and-uninstall.md)：列出项目内 `.loom/`、全局 app data、诊断包和完全清理步骤。
- [Beta 首次启动 Smoke](beta-smoke.md)：提供一个不依赖真实 Agent 凭据的 15 分钟临时项目验证流程。

## M3 隐私、账号与诊断包边界

- [Beta 隐私说明](privacy-note.md)：说明默认本地数据、可能离开本机的路径、诊断包脱敏边界和隐私停止线。
- [Agent / CLI 账号边界](agent-account-boundary.md)：说明 dummy、Codex、Claude Code、OpenClaw、Hermes 和自定义 CLI 的账号、费用、数据发送责任。
- [诊断包安全复核](diagnostic-bundle-review.md)：说明诊断包内容、已有脱敏证据、导出前自查、接收方处理规则和 Beta 发布门禁。
- [诊断包 UI Smoke](diagnostic-bundle-smoke.md)：覆盖默认不导出日志和主动选择日志尾部两条手工验证路径。

## 使用顺序

1. 先读 `beta-scope.md`，确认自己是否属于本轮目标试用者。
2. 再读 `beta-safety-notes.md`，确认不会把 Loom 用在生产关键仓库或含机密材料的任务中。
3. 继续读 `privacy-note.md`、`agent-account-boundary.md` 和 `diagnostic-bundle-review.md`，确认隐私、诊断包和真实 Agent 账号责任边界；维护者在发布前按 `diagnostic-bundle-smoke.md` 填写至少一条 UI smoke record。
4. 维护者用 `release-build-record.md` 对齐候选构建的 commit、工具链、checksum 和剩余门禁，再用 `artifact-integrity-check.md` 判断 DMG 完整性、bundle 元数据和签名 / Gatekeeper 状态。
5. 如果签名 / Gatekeeper 状态不是 pass，先读 `signing-gate-decision.md`；当前候选 DMG 只能保留为本机验证 artifact，不进入公开或默认邀请制分发。
6. 只有签名 gate 通过或被明确接受后，才按 `macos-install.md` 完成安装，并记录 Gatekeeper 或权限提示。
7. 按 `beta-smoke.md` 在临时项目里完成首次启动 smoke；首次 smoke 默认使用 dummy / fixture，不要求真实付费 Agent。
8. 如需卸载或清理，按 `local-data-and-uninstall.md` 处理项目内和全局数据。
9. 完成试用后按 `beta-feedback-template.md` 回传反馈。

## 发布门禁

这些文档覆盖邀请制 Beta 的 M0 边界、M1 可复现构建记录、产物完整性复核与签名 gate 决策、M2 安装/卸载/smoke 路径，以及 M3 隐私、Agent 账号责任、诊断包安全复核和 UI smoke 门禁。公开下载、签名/公证、正式 UI 诊断包 smoke 记录和针对候选 DMG 的首次启动 smoke 结果仍需后续里程碑实际填写；当前 `artifact-integrity-check.md` 已确认 DMG 可校验和挂载，但签名 / Gatekeeper assessment 处于 hold，`signing-gate-decision.md` 已明确不接受当前 invalid-signature DMG 作为公开或默认邀请制分发物。
