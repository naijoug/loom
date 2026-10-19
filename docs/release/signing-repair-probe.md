# Loom Beta Signing Repair Probe

- **状态**: completed for ad-hoc signing probe
- **适用阶段**: M1 signing / Gatekeeper hold 修复探针
- **关联记录**: `docs/release/artifact-integrity-check.md`、`docs/release/signing-gate-decision.md`

## 目标

验证当前 `code has no resources but signature indicates they must be present` 是否可以通过 Tauri v2 的 macOS signing 配置修复，并把“可本机验证”和“可分发”拆开。

本探针不尝试公证，也不把产物提升为 Beta 分发物；它只回答一个问题：Tauri 重新打包时显式配置 macOS signing identity 后，app bundle 的严格 `codesign` gate 是否能恢复为 pass。

## 探针输入

| Field | Value |
| --- | --- |
| Git commit at probe time | `d91969b` |
| Probe command | `pnpm tauri build --bundles app --config '{"bundle":{"macOS":{"signingIdentity":"-"}}}'` |
| Signing mode | ad-hoc signing identity `-` |
| Bundle path | `src-tauri/target/release/bundle/macos/Loom.app` |
| DMG path | 未重打包；本探针只生成 / 覆盖 app bundle |
| Notarization | skipped; build output reported no Apple notarization credentials in environment |

## 结果摘要

| Check | Result | Meaning |
| --- | --- | --- |
| Tauri build with ad-hoc signing | pass | Tauri v2 accepts `bundle.macOS.signingIdentity` from `--config` overlay |
| `codesign --verify --deep --strict --verbose=2` | pass | 原先 invalid-signature / sealed resources 错误可被重新签名修复 |
| `codesign -dv --verbose=4` | observed | bundle is ad-hoc signed with hardened runtime; `TeamIdentifier=not set` |
| `spctl --assess --type execute --verbose=4` | rejected | ad-hoc signing is not sufficient for Gatekeeper distribution |
| DMG distribution gate | still hold | 需要 Developer ID signing + notarization，或另写 unsigned/ad-hoc 分发接受决策 |

## Command log

```bash
pnpm tauri build --bundles app --config '{"bundle":{"macOS":{"signingIdentity":"-"}}}'
codesign --verify --deep --strict --verbose=2 src-tauri/target/release/bundle/macos/Loom.app
spctl --assess --type execute --verbose=4 src-tauri/target/release/bundle/macos/Loom.app
codesign -dv --verbose=4 src-tauri/target/release/bundle/macos/Loom.app
```

Observed key output:

```text
Signing with identity "-"
Warn skipping app notarization, no APPLE_ID & APPLE_PASSWORD & APPLE_TEAM_ID or APPLE_API_KEY & APPLE_API_ISSUER & APPLE_API_KEY_PATH environment variables found
```

```text
src-tauri/target/release/bundle/macos/Loom.app: valid on disk
src-tauri/target/release/bundle/macos/Loom.app: satisfies its Designated Requirement
```

```text
src-tauri/target/release/bundle/macos/Loom.app: rejected
```

```text
Signature=adhoc
TeamIdentifier=not set
Sealed Resources version=2 rules=13 files=1
```

## 结论

本探针解除的是“bundle 自身签名结构损坏 / sealed resources 不一致”的问题，不解除 Gatekeeper 分发门禁。

因此当前推荐路径是：

1. 在不提交二进制产物的前提下保留本探针作为本机修复证据。
2. 下一份候选分发物必须使用 Developer ID Application identity 重新构建或签名。
3. 如果要面向非维护者分发，还必须完成 notarization / staple，或显式记录为何接受未公证分发。
4. 生成新 DMG 后重新记录 checksum 和 size，并重跑：
   - `hdiutil verify src-tauri/target/release/bundle/dmg/Loom_0.1.0_aarch64.dmg`
   - `codesign --verify --deep --strict --verbose=2 <mounted Loom.app>`
   - `spctl --assess --type execute --verbose=4 <mounted Loom.app>`

## 下一步

优先做 Developer ID / notarization 凭据探针，而不是继续扩大 Beta：

- 确认是否使用环境变量 `APPLE_SIGNING_IDENTITY`，还是把 `bundle.macOS.signingIdentity` 写入仓库配置。
- 验证 notarization credential 是否可用；不要在文档中记录 Apple ID、Team ID、API key、issuer 或密码原文。
- 若凭据可用，重跑完整 `pnpm tauri build --bundles dmg`，生成新的 DMG checksum，并新增一段 artifact integrity 记录。
- 若凭据不可用，保持 `signing-gate-decision.md` 的 hold 状态，不分发当前候选 DMG。
