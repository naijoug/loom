# Loom Beta Release Review Checklist

- **状态**: review checklist ready; distribution gate still hold
- **适用阶段**: credential 缺失或 credential 刚注入后的 release review，不替代 notarized DMG gate
- **关联记录**: `docs/release/README.md`、`docs/release/notarization-credential-preflight.md`、`docs/release/notarized-dmg-gate.md`、`docs/release/local-desktop-smoke-record.md`、`docs/release/install-uninstall-smoke-record.md`、`docs/release/beta-first-run-smoke-record.md`、`docs/release/diagnostic-bundle-smoke-record.md`

## 目标

把邀请制 Beta 前的“是否可以继续推进”拆成一页可复核 checklist，避免在 signing / notarization hold 期间因为本机 smoke 通过就误判为可分发。

本 checklist 只记录发布复核状态，不记录任何 Apple ID、app-specific password、API key、issuer、private key、试用者邮箱或本机绝对路径。

## 使用时机

- Credential 仍缺失时：只能填写到 `Distribution decision: Hold`，用于确认下一步是否继续本机验证或等待维护者注入 `loom-beta-notary`。
- Credential 刚注入后：先按 `docs/release/notarized-dmg-gate.md` 重打新 DMG，再用本 checklist 汇总 pass / fail 证据。
- 准备扩大邀请制 Beta 前：必须把本 checklist、notarized DMG gate、首次启动 smoke、诊断包 smoke 和反馈入口全部补齐。

## Review 表

| Gate | Required evidence | Current status | Stop rule |
| --- | --- | --- | --- |
| Scope / safety | `docs/release/beta-scope.md`、`docs/release/beta-safety-notes.md` 已确认仍适用 | Pass | 目标试用者、生产仓库限制或安全边界不清时停止 |
| Feedback path | `docs/release/beta-feedback-template.md` 可直接复制给试用者 | Pass | 反馈分类或必要证据字段缺失时停止 |
| Artifact identity | 新候选 DMG 的 commit、SHA-256、size、build command 已记录 | Wait | 不能复用旧 hold checksum 冒充新候选 |
| Credential preflight | `xcrun notarytool history --keychain-profile loom-beta-notary` 可读取 profile | Hold | 返回 `No Keychain password item found` 时停止 |
| Developer ID identity | `security find-identity -v -p codesigning` 可见 `Developer ID Application: Honoululu Inc. (N7VU72TZB8)` | Review | identity 不可见时停止 |
| Notarized DMG gate | `hdiutil verify`、严格 `codesign`、`spctl`、staple validate 证据齐全 | Wait | 任一 gate fail 时不进入普通试用者分发 |
| Maintainer local smoke | `pnpm smoke:desktop` 或等价维护者本机 smoke 已记录 | Pass | 只能证明本机可启动，不能替代 notarization / Gatekeeper pass |
| First-run beta smoke | 针对同一候选 artifact 完成 `docs/release/beta-smoke.md`，并填写 `docs/release/beta-first-run-smoke-record.md` | Wait | 未对同一 artifact smoke，或只复用维护者本机 smoke / 旧 artifact 记录时不扩大试用范围 |
| Diagnostic bundle smoke | `docs/release/diagnostic-bundle-smoke.md` 已执行，且 `docs/release/diagnostic-bundle-smoke-record.md` 至少一条真实桌面导出记录为 pass | Wait | 诊断包导出、脱敏、日志开关或导出 JSON 删除记录未人工复核时停止 |
| Install / uninstall smoke | `docs/release/macos-install.md`、`docs/release/local-data-and-uninstall.md` 与真实签名状态一致，且 `docs/release/install-uninstall-smoke-record.md` 针对同一候选 artifact 记录安装 / 清理 pass | Wait | 文档仍提示旧 invalid-signature / unnotarized 状态但 artifact 已变更，或未记录同一 artifact 的安装 / 卸载结果时停止 |
| Privacy / account boundary | `docs/release/privacy-note.md`、`docs/release/agent-account-boundary.md` 已复核 | Pass | 真实 Agent 账号、费用、外发数据责任不清时停止 |

## 当前 2026-08-11 结论

- 2026-08-11 10:02 文档复核：`beta-scope.md`、`beta-safety-notes.md`、`beta-feedback-template.md`、`privacy-note.md`、`agent-account-boundary.md` 五份文档结构完整、交叉引用一致、覆盖 checklist 对应 gate 的 stop rule 条件；Scope / safety、Feedback path、Privacy / account boundary 三项从 Review 更新为 Pass。
- `docs/release/notarization-credential-preflight.md` 记录：Developer ID signing identity 可见，但 `loom-beta-notary` credential profile 仍缺失。
- `docs/release/local-desktop-smoke-record.md` 记录：维护者本机 release app 两轮启动 / 退出 smoke 通过。
- `docs/release/install-uninstall-smoke-record.md` 当前只是模板，没有针对已公证候选 DMG 的安装 / 卸载 pass 记录。
- `docs/release/beta-first-run-smoke-record.md` 当前只是模板，没有针对已公证候选 DMG 的首次启动 pass 记录。
- `docs/release/diagnostic-bundle-smoke-record.md` 当前只是模板，没有针对同一候选 artifact 的真实桌面导出 pass 记录。
- 这些证据只支持继续本机验证和文档复核，不支持公开下载或默认邀请制分发。

**Distribution decision: Hold**

继续等待维护者配置 `loom-beta-notary`，然后按 `docs/release/notarized-dmg-gate.md` 生成新候选 DMG 并写回完整证据；credential 缺失期间不要重打普通 Beta DMG，不要要求试用者绕过 Gatekeeper，也不要把本机 smoke pass 改写为分发 pass。

## 下一次可填写记录模板

```text
Review date:
Candidate commit:
Candidate artifact:
Credential preflight:
Developer ID identity:
DMG checksum / size:
hdiutil verify:
Strict codesign:
spctl assessment:
stapler validate:
Maintainer local smoke:
Install / uninstall smoke:
First-run beta smoke:
Diagnostic bundle smoke record:
Distribution decision: Hold / Invite-only / Public
Reviewer:
```

只有 `Credential preflight`、`DMG checksum / size`、`hdiutil verify`、`Strict codesign`、`spctl assessment`、`stapler validate`、`Install / uninstall smoke`、`First-run beta smoke` 和 `Diagnostic bundle smoke` 都有针对同一候选物的 pass 证据时，才允许把 `Distribution decision` 从 `Hold` 改为 `Invite-only`。公开发布还需要额外的下载页、撤回机制和更新说明，不由本 checklist 自动解锁。
