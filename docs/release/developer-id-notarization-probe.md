# Loom Beta Developer ID / Notarization Probe

- **状态**: completed for Developer ID signing availability; notarization still blocked
- **适用阶段**: M1 signing / Gatekeeper hold 修复探针
- **关联记录**: `docs/release/artifact-integrity-check.md`、`docs/release/signing-gate-decision.md`、`docs/release/signing-repair-probe.md`

## 目标

验证维护者本机是否具备 Developer ID Application 签名能力，并把 Gatekeeper hold 从“签名结构损坏”继续缩小到“缺少 notarization / staple”。

本探针不提交二进制产物，不记录 Apple ID、密码、API key、issuer 或其它敏感凭据；只记录证书类别、命令形态和可复核的 gate 结果。

## 探针输入

| Field | Value |
| --- | --- |
| Git commit at probe time | `94989df` |
| Build command | `pnpm tauri build --bundles app --config '{"bundle":{"macOS":{"signingIdentity":"Developer ID Application: Honoululu Inc. (N7VU72TZB8)"}}}'` |
| Signing identity class | `Developer ID Application` |
| Bundle path | `src-tauri/target/release/bundle/macos/Loom.app` |
| DMG path | 未重打包；本探针只生成 / 覆盖 app bundle |
| Notarization credential environment | all expected Apple notarization environment variables were unset |

Expected notarization environment variables checked:

- `APPLE_ID`
- `APPLE_PASSWORD`
- `APPLE_TEAM_ID`
- `APPLE_API_KEY`
- `APPLE_API_ISSUER`
- `APPLE_API_KEY_PATH`

## 结果摘要

| Check | Result | Meaning |
| --- | --- | --- |
| Code signing identity lookup | observed | Keychain has a valid `Developer ID Application` identity available for local signing |
| Tauri build with Developer ID identity | pass | Tauri v2 accepts `bundle.macOS.signingIdentity` overlay with the Developer ID label |
| `codesign --verify --deep --strict --verbose=2` | pass | Bundle is valid on disk and satisfies its designated requirement |
| `codesign -dv --verbose=4` | observed | Authority chain is Developer ID Application → Developer ID Certification Authority → Apple Root CA; `TeamIdentifier=N7VU72TZB8`; hardened runtime present |
| `spctl --assess --type execute --verbose=4` | rejected | Gatekeeper rejection changed to `source=Unnotarized Developer ID` |
| Distribution gate | still hold | Developer ID signing is available, but notarization / staple remains required before normal Beta distribution |

## Command log

```bash
security find-identity -v -p codesigning
pnpm tauri build --bundles app --config '{"bundle":{"macOS":{"signingIdentity":"Developer ID Application: Honoululu Inc. (N7VU72TZB8)"}}}'
codesign --verify --deep --strict --verbose=2 src-tauri/target/release/bundle/macos/Loom.app
codesign -dv --verbose=4 src-tauri/target/release/bundle/macos/Loom.app
spctl --assess --type execute --verbose=4 src-tauri/target/release/bundle/macos/Loom.app
```

Observed key output:

```text
Signing with identity "Developer ID Application: Honoululu Inc. (N7VU72TZB8)"
Warn skipping app notarization, no APPLE_ID & APPLE_PASSWORD & APPLE_TEAM_ID or APPLE_API_KEY & APPLE_API_ISSUER & APPLE_API_KEY_PATH environment variables found
```

```text
src-tauri/target/release/bundle/macos/Loom.app: valid on disk
src-tauri/target/release/bundle/macos/Loom.app: satisfies its Designated Requirement
```

```text
Authority=Developer ID Application: Honoululu Inc. (N7VU72TZB8)
Authority=Developer ID Certification Authority
Authority=Apple Root CA
TeamIdentifier=N7VU72TZB8
Runtime Version=26.5.0
Sealed Resources version=2 rules=13 files=1
```

```text
src-tauri/target/release/bundle/macos/Loom.app: rejected
source=Unnotarized Developer ID
```

## 结论

本探针确认：Loom 当前本机环境已经可以用 Developer ID Application 对 app bundle 完成有效签名，上一轮 ad-hoc 探针发现的 sealed resources / invalid-signature 问题不是最终阻塞。

剩余阻塞已缩小为 notarization：由于 Apple notarization 环境变量均未配置，Tauri 明确跳过公证；Gatekeeper 因 `Unnotarized Developer ID` 拒绝执行评估。因此当前候选仍不能作为普通邀请制 Beta 分发物。

## 下一步

1. 准备 notarization credential，二选一：
   - Apple ID path: `APPLE_ID` + app-specific `APPLE_PASSWORD` + `APPLE_TEAM_ID`
   - App Store Connect API key path: `APPLE_API_KEY` + `APPLE_API_ISSUER` + `APPLE_API_KEY_PATH`
2. 不把凭据写入仓库；只通过本机 shell、CI secret 或临时 keychain 配置注入。
3. 先按 `docs/release/notarization-credential-preflight.md` 做 keychain profile / `notarytool` 预检；credential 缺失时继续保持 release hold。
4. 凭据可用后重跑完整 DMG 构建：
   - `pnpm tauri build --bundles dmg --config '{"bundle":{"macOS":{"signingIdentity":"Developer ID Application: Honoululu Inc. (N7VU72TZB8)"}}}'`
5. 对新 DMG 重新记录 checksum、size 和完整 gate：
   - `hdiutil verify src-tauri/target/release/bundle/dmg/Loom_0.1.0_aarch64.dmg`
   - `codesign --verify --deep --strict --verbose=2 <mounted Loom.app>`
   - `spctl --assess --type execute --verbose=4 <mounted Loom.app>`
6. 只有 notarization / Gatekeeper pass 后，才推进 `docs/release/beta-smoke.md` 与 `docs/release/diagnostic-bundle-smoke.md` 的真实试用前记录。
