# Loom 0.1.0 Beta Release Build Record

本记录用于把邀请制 Beta 的 M1「可复现 release build」从口头计划变成可复核证据。它不是公开发布公告；在签名、公证、正式 UI smoke record 完成前，DMG 只适合维护者本机复核或小范围邀请制试用准备。

## Build target

| Field | Value |
| --- | --- |
| Product | Loom |
| Version | 0.1.0 |
| Git commit | `5efb082` |
| Platform | macOS arm64 / aarch64 |
| Tauri config | `src-tauri/tauri.conf.json` |
| Release binary | `src-tauri/target/release/loom` |
| App bundle | `src-tauri/target/release/bundle/macos/Loom.app` |
| DMG | `src-tauri/target/release/bundle/dmg/Loom_0.1.0_aarch64.dmg` |

## Environment snapshot

| Tool | Version |
| --- | --- |
| Node.js | `v22.22.3` |
| pnpm | `10.29.2` |
| rustc | `rustc 1.94.1 (e408947bf 2026-03-25)` |
| cargo | `cargo 1.94.1 (29ea6fb6a 2026-03-24)` |
| tauri-cli | `tauri-cli 2.11.1` |

## Command log

```bash
pnpm tauri build
```

Observed result:

- Frontend build completed through `tsc && vite build`.
- Rust release profile completed.
- Tauri produced:
  - `src-tauri/target/release/bundle/macos/Loom.app`
  - `src-tauri/target/release/bundle/dmg/Loom_0.1.0_aarch64.dmg`

## Artifact checksum

```text
d3fa3168567af5255b42df5b5c4132092f84f1b57e32161d65fbc0a2688d3d7b  src-tauri/target/release/bundle/dmg/Loom_0.1.0_aarch64.dmg
```

DMG size: `5022059 bytes`.

## Git cleanliness and artifact boundary

- `git status --short` after the build was clean.
- Build outputs are ignored by git:
  - `dist/`
  - `src-tauri/target/`
- This record is the tracked evidence. The DMG itself is not committed.

## Remaining release gates

Before treating this build as a public Beta artifact, finish these gates:

1. Run and fill at least one real record from `docs/release/diagnostic-bundle-smoke.md`.
2. Run the first-start flow from `docs/release/beta-smoke.md` against this exact build.
3. Decide whether invitation-only distribution accepts an unsigned DMG, or add signing and notarization evidence.
4. If the DMG is rebuilt, update the commit, environment snapshot, checksum and size in this file.
