# Loom Beta First-run Smoke Record

- **状态**: template; no candidate artifact pass recorded yet
- **适用阶段**: M2 安装与首次启动 smoke
- **关联文档**: `docs/release/beta-smoke.md`、`docs/release/beta-release-review-checklist.md`、`docs/release/macos-install.md`、`docs/release/local-data-and-uninstall.md`

## 目标

把 `docs/release/beta-smoke.md` 的 15 分钟首次启动流程落到同一候选 artifact 的可复核记录里，避免把维护者本机 release app smoke、旧 DMG 或未公证 artifact 的结果混用为邀请制 Beta 分发证据。

本记录只保存可复核结论和相对路径；不要写入试用者邮箱、真实项目名称、Apple ID、Agent 账号、token、本机绝对路径或未脱敏日志。

## 使用规则

- 每个候选 artifact 至少需要一条已填写记录；记录里的 commit、artifact、checksum 必须和 `docs/release/release-build-record.md` / `docs/release/notarized-dmg-gate.md` 对齐。
- Credential 或 notarized DMG gate 缺失时，本记录只能保持 `Beta gate: hold`，不能把普通本机启动成功改写为邀请制分发 pass。
- 使用临时 Git 项目和 dummy / fixture command；首次 smoke 默认不要求真实 Codex、Claude Code、OpenClaw、Hermes 或其他付费 Agent 账号。
- 如果发现安装、启动、项目打开、`.loom/` 边界、验证命令、重启持久化、诊断包脱敏任一项失败，按 `docs/release/beta-feedback-template.md` 标记 blocker / security / bug，并保持 hold。

## 记录模板

```md
# First-run smoke record

- Date:
- Reviewer:
- Loom commit:
- App version:
- Candidate artifact:
- Artifact checksum:
- Platform:
- Install source:
- Gatekeeper / signing state:
- Temporary project fixture:
- Agent mode: dummy / fixture / real agent manually accepted

## Steps

| Check | Result | Evidence |
| --- | --- | --- |
| App installs / launches | pass / fail | |
| Temporary Git project opens | pass / fail | |
| Project-local `.loom/` created and ignored | pass / fail | |
| Low-risk validation command runs and shows output | pass / fail | |
| Stop / retry behavior understood | pass / fail / not checked | |
| Summary or local record can be found | pass / fail / not checked | |
| Restart keeps recent project or task history readable | pass / fail | |
| Diagnostics export reviewed before sharing | pass / fail / skipped | |
| Cleanup path understood | pass / fail | |

## Decision

- Beta gate: pass / hold
- Blocking feedback path:
- Notes:
```

## 当前 2026-08-11 结论

当前没有针对已公证候选 DMG 的首次启动 smoke pass 记录；`docs/release/local-desktop-smoke-record.md` 只证明维护者本机 release app bundle 能启动 / 重启，不能替代本记录。`loom-beta-notary` credential 缺失期间，邀请制 Beta 分发结论继续保持 hold。
