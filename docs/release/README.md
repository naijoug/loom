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
- [Signing Repair Probe](signing-repair-probe.md)：记录 ad-hoc signing 探针，确认重新签名可修复严格 `codesign` 错误，但仍不能通过 Gatekeeper 分发门禁。
- [Developer ID / Notarization Probe](developer-id-notarization-probe.md)：记录 Developer ID signing 探针，确认本机可完成 Developer ID 签名，但因缺少 notarization credential 仍被 Gatekeeper 判定为 `Unnotarized Developer ID`。
- [Notarization Credential Preflight](notarization-credential-preflight.md)：记录 `notarytool` 与 keychain profile 预检，确认本机有 notarization CLI，但尚未配置 `loom-beta-notary` credential profile。
- [Notarized DMG Gate](notarized-dmg-gate.md)：把 credential 可用后的重打 DMG、checksum、`hdiutil`、严格 `codesign`、`spctl` 和 staple validate 拆成下一轮可执行命令梯。
- [Local Desktop Smoke Record](local-desktop-smoke-record.md)：记录 signing hold 期间不扩大分发的维护者本机 release app 启动 / 重启 smoke，作为本机验证证据而不是分发 pass。
- [Beta Release Review Checklist](beta-release-review-checklist.md)：把 scope、安全、artifact、credential、notarized DMG、smoke、诊断包、安装文档和隐私边界汇总成分发前复核表；当前结论仍为 `Distribution decision: Hold`。

## M2 安装与首次启动

- [macOS Beta 安装说明](macos-install.md)：说明 DMG / App bundle 安装、Gatekeeper 处理原则和首次启动检查。
- [本地数据与卸载清理说明](local-data-and-uninstall.md)：列出项目内 `.loom/`、全局 app data、诊断包和完全清理步骤。
- [Beta 首次启动 Smoke](beta-smoke.md)：提供一个不依赖真实 Agent 凭据的 15 分钟临时项目验证流程。
- [Beta First-run Smoke Record](beta-first-run-smoke-record.md)：把首次启动 smoke 填成同一候选 artifact 的可复核记录；当前尚无已公证候选 DMG 的 pass 记录。

## M3 隐私、账号与诊断包边界

- [Beta 隐私说明](privacy-note.md)：说明默认本地数据、可能离开本机的路径、诊断包脱敏边界和隐私停止线。
- [Agent / CLI 账号边界](agent-account-boundary.md)：说明 dummy、Codex、Claude Code、OpenClaw、Hermes 和自定义 CLI 的账号、费用、数据发送责任。
- [诊断包安全复核](diagnostic-bundle-review.md)：说明诊断包内容、已有脱敏证据、导出前自查、接收方处理规则和 Beta 发布门禁。
- [诊断包工程脱敏证据](diagnostic-bundle-engineering-proof.md)：记录后端诊断包单元测试复跑结果，确认默认无日志、路径替换、fake secret 脱敏、项目外日志引用过滤、无效引用不挤占日志限额，以及 headless 文件写入 / 读回 harness 仍通过。
- [诊断包 UI Smoke](diagnostic-bundle-smoke.md)：覆盖默认不导出日志和主动选择日志尾部两条手工验证路径；`pnpm smoke:interaction` 已覆盖 Settings / Done pane 的诊断入口与日志开关存在性，但真实导出 JSON 仍需桌面手工记录。

## 使用顺序

1. 先读 `beta-scope.md`，确认自己是否属于本轮目标试用者。
2. 再读 `beta-safety-notes.md`，确认不会把 Loom 用在生产关键仓库或含机密材料的任务中。
3. 继续读 `privacy-note.md`、`agent-account-boundary.md`、`diagnostic-bundle-review.md` 和 `diagnostic-bundle-engineering-proof.md`，确认隐私、诊断包、后端脱敏 fixture 和真实 Agent 账号责任边界；维护者在发布前按 `diagnostic-bundle-smoke.md` 填写至少一条 UI smoke record。
4. 维护者用 `release-build-record.md` 对齐候选构建的 commit、工具链、checksum 和剩余门禁，再用 `artifact-integrity-check.md` 判断 DMG 完整性、bundle 元数据和签名 / Gatekeeper 状态。
5. 如果签名 / Gatekeeper 状态不是 pass，先读 `signing-gate-decision.md`；当前候选 DMG 只能保留为本机验证 artifact，不进入公开或默认邀请制分发。
6. 如需修复签名，先读 `signing-repair-probe.md`：ad-hoc signing 已证明严格 `codesign` 错误可修复，但 Gatekeeper 分发仍需要 Developer ID / notarization 或明确接受未公证边界。
7. 再读 `developer-id-notarization-probe.md`：Developer ID signing 已可用，当前剩余 blocker 是 notarization credential 未配置导致 `Unnotarized Developer ID`。
8. 如果准备解除 hold，先按 `notarization-credential-preflight.md` 检查 `notarytool` 和 keychain profile；credential 缺失时不要重打普通 Beta DMG。
9. Credential 可用后按 `notarized-dmg-gate.md` 重打新候选 DMG，并把 pass / fail 证据写回 release record；不要覆盖旧 hold 证据。
10. Credential 缺失但需要继续本机验证时，只能参考 `local-desktop-smoke-record.md` 这类维护者本机 smoke；它不能替代普通试用者分发 gate。
11. 每次准备解除 hold 或扩大邀请制 Beta 前，先填写 `beta-release-review-checklist.md`；credential 缺失或 notarized DMG gate 未通过时，结论必须保持 `Distribution decision: Hold`。
12. 只有签名 gate 通过或被明确接受后，才按 `macos-install.md` 完成安装，并记录 Gatekeeper 或权限提示。
13. 按 `beta-smoke.md` 在临时项目里完成首次启动 smoke，并把同一候选 artifact 的结果填入 `beta-first-run-smoke-record.md`；维护者本机 smoke 或旧 artifact 记录不能混用为邀请制分发证据。
14. 如需卸载或清理，按 `local-data-and-uninstall.md` 处理项目内和全局数据。
15. 完成试用后按 `beta-feedback-template.md` 回传反馈。

## 发布门禁

这些文档覆盖邀请制 Beta 的 M0 边界、M1 可复现构建记录、产物完整性复核、签名 gate 决策、ad-hoc 修复探针、Developer ID / notarization 探针、notarization credential 预检、credential 可用后的 notarized DMG gate 命令梯、signing hold 期间的维护者本机桌面 smoke、分发前 release review checklist、M2 安装/卸载/smoke 路径和同一候选 artifact 的首次启动 smoke record，以及 M3 隐私、Agent 账号责任、诊断包安全复核、工程脱敏证据和 UI smoke 门禁。公开下载、notarization / staple、正式 UI 诊断包导出记录和针对已公证候选 DMG 的首次启动 smoke 结果仍需后续里程碑实际填写；当前 `artifact-integrity-check.md` 已确认 DMG 可校验和挂载，但签名 / Gatekeeper assessment 处于 hold，`signing-gate-decision.md` 已明确不接受当前 invalid-signature DMG 作为公开或默认邀请制分发物，`signing-repair-probe.md` 确认 ad-hoc signing 可以修复严格 `codesign` 错误但不能解除 Gatekeeper 分发门禁，`developer-id-notarization-probe.md` 进一步确认 Developer ID signing 可用但缺少 notarization credential，`notarization-credential-preflight.md` 确认本机有 `notarytool` 但缺少 `loom-beta-notary` keychain profile；`notarized-dmg-gate.md` 已把下一次 pass 所需命令和证据字段拆好，`local-desktop-smoke-record.md` 仅证明当前 release app 可在维护者本机启动 / 重启两轮，`beta-first-run-smoke-record.md` 当前仍无已公证候选 DMG 的 pass 记录，`beta-release-review-checklist.md` 将当前分发结论固定为 `Distribution decision: Hold`，`diagnostic-bundle-engineering-proof.md` 证明后端脱敏 fixture 与 headless 文件写入 / 读回 harness 通过，`pnpm smoke:interaction` 现在覆盖诊断 UI 入口 / 日志开关存在性，但仍不能替代分发 gate 或真实桌面导出 JSON 复核。
