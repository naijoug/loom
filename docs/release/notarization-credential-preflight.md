# Loom Beta Notarization Credential Preflight

- **状态**: credential profile probe completed; notarization credential still missing
- **适用阶段**: M1 Developer ID signing pass 后、重打 DMG 前的低风险凭据检查
- **关联记录**: `docs/release/developer-id-notarization-probe.md`、`docs/release/signing-gate-decision.md`

## 目标

在不把 Apple ID、密码、API key、issuer 或 private key 写入仓库的前提下，确认当前机器是否已经具备可供 Tauri / Apple notarytool 使用的 notarization credential。

本预检只回答两个问题：

1. 本机是否有 `xcrun notarytool`。
2. 是否存在约定的 keychain profile，可直接进入下一轮 notarized DMG 构建。

## 当前探针结果

| Check | Result | Meaning |
| --- | --- | --- |
| `xcrun notarytool --version` | pass: `1.1.2 (41)` | Xcode notarization CLI 可用 |
| `xcrun notarytool history --keychain-profile loom-beta-notary` | missing credential | keychain 中没有名为 `loom-beta-notary` 的 notarization profile |
| Environment credential path | missing | 上一轮已确认常见 Apple notarization 环境变量未配置 |
| Next release gate | hold | 不能重打普通 Beta DMG；需要先注入 credential |

Observed output:

```text
1.1.2 (41)
Error: No Keychain password item found for profile: loom-beta-notary

Run 'notarytool store-credentials' to create another credential profile.
```

## 建议的凭据注入方式

优先使用 keychain profile，而不是仓库配置文件：

```bash
xcrun notarytool store-credentials loom-beta-notary \
  --apple-id "<apple-id>" \
  --team-id "N7VU72TZB8" \
  --password "<app-specific-password>"
```

或者使用 App Store Connect API key 方式，把 key 文件放在仓库外，并只通过 shell / CI secret 注入路径。无论哪种方式，都不要提交以下内容：

- Apple ID
- app-specific password
- App Store Connect API private key
- issuer ID
- key ID
- 临时 `.p8` 文件

## Credential 可用后的下一轮命令

credential 准备完成后，下一轮先做 profile smoke，再重打 DMG：

```bash
xcrun notarytool history --keychain-profile loom-beta-notary
pnpm tauri build --bundles dmg --config '{"bundle":{"macOS":{"signingIdentity":"Developer ID Application: Honoululu Inc. (N7VU72TZB8)"}}}'
```

如果 Tauri 构建仍不能自动使用 keychain profile，则改走显式环境变量或单独 `xcrun notarytool submit --wait` 路径，并把实际命令追加到新的 release record；不要覆盖现有 hold 证据。

## 通过标准

下一份可分发候选物至少需要同时满足：

1. 新 DMG 重新生成，并有新的 SHA-256 与文件大小记录。
2. `hdiutil verify src-tauri/target/release/bundle/dmg/Loom_0.1.0_aarch64.dmg` 通过。
3. 挂载后的 `Loom.app` 通过严格 `codesign`。
4. `spctl --assess --type execute --verbose=4 <mounted Loom.app>` 不再返回 `Unnotarized Developer ID`。
5. 如有 staple 步骤，记录 `xcrun stapler validate <mounted Loom.app>` 或等价检查。

## 2026-08-11 复检

本轮只做低风险凭据状态复检，不读取或写入 Apple ID、app-specific password、API key、issuer、private key，也不提交任何真实凭据。

| Check | Result | Meaning |
| --- | --- | --- |
| `security find-identity -v -p codesigning` | pass: 6 valid identities | 本机 code signing identity 可枚举，其中包含 `Developer ID Application: Honoululu Inc. (N7VU72TZB8)` |
| `xcrun notarytool history --keychain-profile loom-beta-notary` | missing credential | `loom-beta-notary` 仍未写入 keychain；不能进入 notarized DMG gate |
| Next release gate | hold | 继续等待维护者注入 notary credential 后再重打候选 DMG |

Observed output 摘要：

```text
6 valid identities found
Error: No Keychain password item found for profile: loom-beta-notary
Run 'notarytool store-credentials' to create another credential profile.
```

复检结论：Developer ID signing identity 可见，但 notary credential 仍缺失；不要把当前 DMG 重新标记为可公开或默认邀请制分发物。

## 结论

当前机器有 notarization CLI，也有可用 Developer ID signing identity，但缺少 notarization credential profile。因此 release gate 仍保持 hold；下一次优先动作是由维护者在本机 keychain 或 CI secrets 中注入 credential，然后重打 DMG 并追加完整 artifact integrity 记录。
