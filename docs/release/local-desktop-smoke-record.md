# Loom Beta Local Desktop Smoke Record

- **状态**: local maintainer smoke pass; distribution gate still hold
- **日期**: 2026-08-09 20:00 CST
- **适用阶段**: M1 signing hold 期间的维护者本机验证；不是 M2 试用者首次启动 smoke pass
- **关联记录**: `docs/release/signing-gate-decision.md`、`docs/release/notarized-dmg-gate.md`、`docs/release/beta-smoke.md`

## 目标

在 notarization credential 尚未就绪、DMG 仍不能作为普通试用包分发的前提下，补一条不扩大 Beta 的本机桌面 smoke 证据：确认当前 release app bundle 至少能启动、保持存活并正常退出 / 重启两轮。

这条记录只回答“维护者本机 release app 是否能基本启动”，不回答以下问题：

- 是否可以绕过 Gatekeeper 分发给普通试用者。
- 是否已经完成 notarization / staple。
- 是否完成 `docs/release/beta-smoke.md` 的临时项目首次启动流程。
- 是否完成 `docs/release/diagnostic-bundle-smoke.md` 的诊断包 UI 手工复核。

## 前置状态

- `xcrun notarytool history --keychain-profile loom-beta-notary` 仍返回缺少 keychain profile，因此未进入 `docs/release/notarized-dmg-gate.md` 的重打 DMG 命令梯。
- `loom/` 启动 smoke 前 `git status --short` 为 clean。
- 使用现有 release app bundle：`src-tauri/target/release/bundle/macos/Loom.app`。
- 本轮没有提交或分发 `dist/`、`src-tauri/target/` 中的构建产物。

## 执行命令

```bash
pnpm smoke:desktop
```

Observed result:

```text
OK   desktop cycle 1
OK   desktop cycle 2
Desktop launch/restart smoke passed
```

脚本行为：

1. 停止本仓库旧的本地预览 / 桌面进程。
2. 启动 release app bundle 内的桌面二进制。
3. 等待启动存活窗口。
4. 检查 smoke 日志中没有 `panic`、`fatal error`、`segmentation fault` 等致命输出。
5. 终止进程并重复第二轮。

## 结果

| Check | Result | Evidence |
| --- | --- | --- |
| Release desktop binary exists | pass | `pnpm smoke:desktop` 未触发 “binary not found” |
| Cycle 1 startup / survival | pass | 输出 `OK   desktop cycle 1` |
| Cycle 2 startup / survival | pass | 输出 `OK   desktop cycle 2` |
| Fatal log scan | pass | 脚本未发现 `panic` / `fatal error` / `segmentation fault` |
| Smoke log content | pass | 两轮日志文件为空，未产生崩溃或致命输出 |
| Distribution readiness | hold | signing / notarization gate 仍按 `docs/release/signing-gate-decision.md` 保持 hold |

## 2026-08-11 03:30 CST 复跑

本轮在确认 `loom-beta-notary` credential 仍缺失后，只复跑维护者本机 release app 启动 / 重启 smoke，不重打 DMG、不扩大分发，也不把结果记为 notarization 或 Gatekeeper pass。

前置状态：

- `loom/` 复跑前 `git status --short --branch` 输出 `## codex/project-stabilization`，无未提交改动。
- 使用既有 release app bundle：`src-tauri/target/release/bundle/macos/Loom.app`。
- 本轮没有提交或分发 `dist/`、`src-tauri/target/` 中的构建产物。

执行命令：

```bash
pnpm smoke:desktop
```

Observed result:

```text
OK   desktop cycle 1
OK   desktop cycle 2
Desktop launch/restart smoke passed
Logs: /var/folders/wh/.../loom-desktop-smoke.1Jm6ZM
```

结果：

| Check | Result | Evidence |
| --- | --- | --- |
| Release app launch cycle 1 | pass | 输出 `OK   desktop cycle 1` |
| Release app launch cycle 2 | pass | 输出 `OK   desktop cycle 2` |
| Fatal log scan | pass | `pnpm smoke:desktop` 未报告 `panic` / `fatal error` / `segmentation fault` |
| Distribution readiness | hold | `docs/release/notarization-credential-preflight.md` 仍记录 `loom-beta-notary` credential 缺失 |

复跑结论：既有 release app bundle 仍可在维护者本机完成两轮启动 / 退出 smoke；这只支持继续做本机验证和文档复核，不能替代 `docs/release/notarized-dmg-gate.md` 的新 DMG、checksum、`hdiutil`、严格 `codesign`、`spctl` 与 staple validate。

## 结论

当前 release app bundle 通过维护者本机两轮桌面启动 / 退出 smoke，2026-08-11 03:30 CST 复跑仍通过，可继续用于本机验证 release 文档、安装说明和后续签名修复流程。

但这不是分发 pass：`loom-beta-notary` credential profile 仍缺失，DMG 仍不能作为公开或默认邀请制 Beta 分发物。下一次若 credential 可用，应先按 `docs/release/notarized-dmg-gate.md` 重打并验证 notarized DMG；若 credential 仍不可用，才继续做不依赖分发的本机验证。
