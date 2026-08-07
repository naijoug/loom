# Loom 0.1.0 Beta Artifact Integrity Check

- **状态**: completed for current local candidate
- **适用阶段**: M1 可复现构建后的产物复核
- **关联记录**: `docs/release/release-build-record.md`

## 目标

在把 `Loom_0.1.0_aarch64.dmg` 用于邀请制 Beta 前，先用命令行复核 DMG 是否完整、是否能只读挂载、bundle 元数据是否与 release record 一致，以及当前签名 / Gatekeeper 状态是否允许继续扩大分发。

这不是首次启动 UI smoke；首次启动仍按 `docs/release/beta-smoke.md` 另行填写记录。

## 检查对象

| Field | Value |
| --- | --- |
| Git commit at check time | `9c75fa1` |
| DMG | `src-tauri/target/release/bundle/dmg/Loom_0.1.0_aarch64.dmg` |
| Expected SHA-256 | `d3fa3168567af5255b42df5b5c4132092f84f1b57e32161d65fbc0a2688d3d7b` |
| Expected size | `5022059 bytes` |
| App bundle inside DMG | `Loom.app` |
| Expected app version | `0.1.0` |
| Expected bundle identifier | `com.naijoug.loom` |

## Command log

```bash
hdiutil verify src-tauri/target/release/bundle/dmg/Loom_0.1.0_aarch64.dmg
hdiutil attach -readonly -nobrowse src-tauri/target/release/bundle/dmg/Loom_0.1.0_aarch64.dmg
/usr/libexec/PlistBuddy -c 'Print :CFBundleName' /Volumes/Loom/Loom.app/Contents/Info.plist
/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' /Volumes/Loom/Loom.app/Contents/Info.plist
/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' /Volumes/Loom/Loom.app/Contents/Info.plist
codesign --verify --deep --strict --verbose=2 /Volumes/Loom/Loom.app
spctl --assess --type execute --verbose=4 /Volumes/Loom/Loom.app
hdiutil detach /Volumes/Loom
```

## Observed result

- `hdiutil verify` returned `checksum of "src-tauri/target/release/bundle/dmg/Loom_0.1.0_aarch64.dmg" is VALID`.
- Read-only attach succeeded at `/Volumes/Loom`.
- Mounted DMG contains:
  - `Loom.app`
  - `Applications -> /Applications` symlink
  - `.VolumeIcon.icns`
  - `.DS_Store`
- App bundle metadata matched expectations:
  - `CFBundleName`: `Loom`
  - `CFBundleShortVersionString`: `0.1.0`
  - `CFBundleIdentifier`: `com.naijoug.loom`
- `codesign --verify --deep --strict --verbose=2` did **not** pass:
  - `/Volumes/Loom/Loom.app: code has no resources but signature indicates they must be present`
- `spctl --assess --type execute --verbose=4` did **not** pass with the same message:
  - `/Volumes/Loom/Loom.app: code has no resources but signature indicates they must be present`
- Detach completed successfully.

## Decision

- DMG integrity and mountability: **pass**.
- Bundle identity / version check: **pass**.
- Signing / Gatekeeper assessment: **hold**.

This candidate can remain a local verification artifact, but it should **not** be promoted as a public Beta download. For invitation-only testing, the maintainer must explicitly decide whether to accept this unsigned / invalid-signature state and disclose the Gatekeeper friction in `docs/release/macos-install.md`; otherwise fix signing and notarization before distributing the DMG.

## Follow-up gates

1. Fix or intentionally remove the invalid signature state, then rerun `codesign --verify --deep --strict --verbose=2` and `spctl --assess --type execute --verbose=4`.
2. If unsigned invitation-only distribution is accepted, add a short signed/notarized decision note so试用者 know the exact risk boundary.
3. Run `docs/release/beta-smoke.md` against the exact artifact that passes or is explicitly accepted by this gate.
4. Run `docs/release/diagnostic-bundle-smoke.md` and keep at least one completed smoke record before widening the Beta group.
