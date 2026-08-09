# Loom Beta Notarized DMG Gate

- **状态**: runbook ready; blocked until notarization credential is available
- **适用阶段**: M1 signing / notarization 修复后、M2 首次启动 smoke 前
- **关联记录**: `docs/release/notarization-credential-preflight.md`、`docs/release/signing-gate-decision.md`

## 目标

把“credential 可用后重打 DMG”拆成一组可复制、可停止、可追加证据的命令，避免下一轮在凭据就绪时只凭记忆拼接 signing、notarization、staple、checksum 和 Gatekeeper 检查。

本 runbook 不记录任何 Apple ID、密码、API key、issuer、private key 或本机绝对路径。

## 启动前条件

只有同时满足以下条件，才进入本 runbook：

1. `xcrun notarytool history --keychain-profile loom-beta-notary` 不再返回 `No Keychain password item found`。
2. 本机仍可找到 `Developer ID Application: Honoululu Inc. (N7VU72TZB8)` signing identity。
3. `git status --short` 在 `loom/` 中没有需要混入 release 记录的未知源码改动。
4. 维护者明确要生成新的候选 DMG，而不是覆盖 `docs/release/artifact-integrity-check.md` 里的旧 hold 证据。

如果任一条件失败，停止并把失败输出追加到新的 probe / preflight 记录；不要扩大 Beta 分发。

## 命令梯

### 1. Credential smoke

```bash
xcrun notarytool history --keychain-profile loom-beta-notary
security find-identity -v -p codesigning
```

Pass 语义：notarytool 能读取 profile，且 codesigning identity 列表中包含 `Developer ID Application: Honoululu Inc. (N7VU72TZB8)`。

Fail 语义：credential 或 identity 不可用；回到 `docs/release/notarization-credential-preflight.md`，不要构建 DMG。

### 2. Rebuild signed DMG

```bash
pnpm tauri build --bundles dmg --config '{"bundle":{"macOS":{"signingIdentity":"Developer ID Application: Honoululu Inc. (N7VU72TZB8)"}}}'
```

Pass 语义：命令完成并生成新的 `src-tauri/target/release/bundle/dmg/Loom_0.1.0_aarch64.dmg`。

Fail 语义：记录 Tauri / codesign / notarytool 输出中的第一处失败；不要复用旧 DMG checksum。

### 3. Artifact identity and checksum

```bash
shasum -a 256 src-tauri/target/release/bundle/dmg/Loom_0.1.0_aarch64.dmg
wc -c src-tauri/target/release/bundle/dmg/Loom_0.1.0_aarch64.dmg
hdiutil verify src-tauri/target/release/bundle/dmg/Loom_0.1.0_aarch64.dmg
```

Pass 语义：得到新的 SHA-256、字节数，且 `hdiutil verify` 返回 valid。

Fail 语义：记录失败，不进入挂载后的 app gate。

### 4. Mounted app gate

```bash
hdiutil attach -readonly -nobrowse src-tauri/target/release/bundle/dmg/Loom_0.1.0_aarch64.dmg
codesign --verify --deep --strict --verbose=2 /Volumes/Loom/Loom.app
spctl --assess --type execute --verbose=4 /Volumes/Loom/Loom.app
xcrun stapler validate /Volumes/Loom/Loom.app
hdiutil detach /Volumes/Loom
```

Pass 语义：严格 `codesign`、Gatekeeper assessment 和 staple validate 都通过，并且 DMG 被正常 detach。

Fail 语义：在 detach 后记录失败命令、exit code 和关键输出；若 `spctl` 仍为 `Unnotarized Developer ID`，保持 `docs/release/signing-gate-decision.md` 的 hold。

## 证据写回位置

Credential 可用并成功生成新候选物后，追加或新增 release 记录时至少包含：

| Field | 写入位置 | 说明 |
| --- | --- | --- |
| Build commit | `docs/release/release-build-record.md` 或新候选记录 | 用 `git rev-parse --short HEAD` 读回 |
| Build command | 同上 | 保留完整 `pnpm tauri build ... --config ...` 形态 |
| Signing identity label | 同上 | 只写 identity label，不写私钥或 keychain 内容 |
| Notarization evidence | 新 notarization / artifact record | 写 notarytool submit / staple 结果和 request id（如有） |
| DMG SHA-256 | `docs/release/artifact-integrity-check.md` 或新候选记录 | 旧 hold checksum 不要覆盖成“已通过” |
| DMG size | 同上 | 用字节数记录 |
| Gatekeeper result | `docs/release/signing-gate-decision.md` 和新候选记录 | 只有 `spctl` pass 才能解除 hold |

## 解除 hold 的最低标准

`docs/release/signing-gate-decision.md` 只能在以下证据齐全后改为 pass：

1. 新 DMG 的 checksum、size 和 `hdiutil verify` 已记录。
2. 挂载后的 `Loom.app` 严格 `codesign` 通过。
3. `spctl --assess --type execute --verbose=4 /Volumes/Loom/Loom.app` 通过。
4. `xcrun stapler validate /Volumes/Loom/Loom.app` 通过，或明确记录 Tauri / notarization 输出足以解释为什么无须 staple。
5. `docs/release/macos-install.md` 已删除或降级旧 invalid-signature / unnotarized 试用提示，并保留普通试用者不需要绕过 Gatekeeper 的路径。

## 当前结论

截至本记录，`loom-beta-notary` credential profile 仍缺失，因此本 runbook 只是下一轮可执行命令梯，不是 release pass 证据。当前候选 DMG 仍保持 Gatekeeper hold，不进入公开或默认邀请制分发。
