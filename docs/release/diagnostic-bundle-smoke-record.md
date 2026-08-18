# Loom Beta Diagnostic Bundle Smoke Record

- **状态**: record template ready; no real desktop export pass yet
- **适用阶段**: M3 诊断包 UI 导出验证
- **关联文档**: `docs/release/diagnostic-bundle-smoke.md`、`docs/release/diagnostic-bundle-review.md`、`docs/release/privacy-note.md`、`docs/release/beta-release-review-checklist.md`

## 目标

把 `docs/release/diagnostic-bundle-smoke.md` 中的手工 UI smoke 流程沉淀成一份可填写记录，确保邀请制 Beta 前至少有一次真实桌面导出的 JSON 被人工复核，而不是只引用工程测试、headless harness 或 Web preview 的 UI 入口 smoke。

本记录只保存复核结论、命令和相对路径；不要粘贴诊断包原始 JSON，不要记录真实项目路径、真实 token、Apple 凭据、试用者邮箱或本机绝对路径。

## 当前 2026-08-18 状态

- 已有工程证据：`docs/release/diagnostic-bundle-engineering-proof.md` 记录后端脱敏 fixture 与 headless 文件写入 / 读回 harness 通过；2026-08-18 15:00 复跑 `cargo test diagnostic_bundle --lib`，6 个 diagnostic bundle 测试通过。
- 已有 UI 入口证据：2026-08-18 15:00 复跑 `pnpm smoke:interaction`，Settings / Done pane 的诊断入口、日志开关和 Web preview 约束通过。
- 仍缺门禁证据：尚未针对同一 Beta 候选 artifact 完成真实桌面“保存诊断包 JSON → 人工搜索 → 删除临时文件”的记录；headless 文件 harness 与 interaction smoke 不能替代系统保存对话框后的人工 JSON 复核。

**Diagnostic bundle beta gate: Hold**

在本记录出现至少一条 `Beta gate: pass` 前，不能把诊断包 UI smoke 视为已满足邀请制 Beta 分发门禁。

## 记录模板

```md
## YYYY-MM-DD HH:mm

### 候选信息
- Loom commit:
- App version:
- Candidate artifact:
- Artifact checksum:
- Platform:
- Export surface: Settings / Done pane / both
- Temporary project fixture:
- Reviewer:

### Path A: no logs
- Result: pass / fail
- Export file reviewed manually: yes / no
- Export file deleted after review: yes / no
- `logs` empty: yes / no
- `omittedLogCount` expected: yes / no / not applicable
- Real project path absent: yes / no
- HOME path absent or rewritten to `~`: yes / no
- Fake secrets absent: yes / no
- Search terms checked: `token`, `secret`, `password`, `Authorization`, `Bearer`, `OPENAI_API_KEY`, project fixture name
- Notes:

### Path B: with fake-only log tails
- Result: pass / fail / skipped
- Export file reviewed manually: yes / no
- Export file deleted after review: yes / no
- Log tail count:
- Log tail source limited to project `.loom/logs/`: yes / no
- Real project path absent: yes / no
- HOME path absent or rewritten to `~`: yes / no
- Fake secrets absent: yes / no
- Limit behavior observed: yes / no / not applicable
- Notes:

### Decision
- Beta gate: pass / hold
- Stop reason if hold:
- Follow-up issue or doc path:
```

## 复核步骤

1. 只使用临时项目 fixture；不要打开生产仓库、客户仓库或含真实凭据的目录。
2. 在临时项目中创建 fake-only 日志，例如只包含 `OPENAI_API_KEY=fake-token-for-smoke` 和 `Authorization: Bearer fake-bearer-for-smoke`。
3. 先导出不含日志的诊断包，人工打开 JSON 并按 Path A 搜索；检查完立即删除导出文件。
4. 再导出包含日志尾部的诊断包，人工打开 JSON 并按 Path B 搜索；检查完立即删除导出文件。
5. 只把 pass / fail、搜索词、是否删除临时 JSON 和必要 follow-up 写入本记录；不要提交导出的 JSON。
6. 任一路径出现真实路径、真实凭据、fake secret 原文、生产数据或 `.loom/logs/` 外文件内容时，立刻把 `Beta gate` 写为 `hold`，并按 `docs/release/beta-feedback-template.md` 的 `security` 分类记录问题。

## 发布门禁

只有同时满足以下条件，才允许把 `Diagnostic bundle smoke` 从 `Wait` 改为 `Pass`：

- Path A 通过，且导出的无日志 JSON 已人工复核并删除。
- Path B 通过，或明确记录该候选构建不暴露日志导出承诺且不会向试用者提供日志尾部功能。
- 记录绑定同一候选 artifact 的 commit、checksum 和安装来源。
- 记录中没有本机绝对路径、真实 token、试用者信息或诊断包原始 JSON。
- 若失败，已经留下 `security` / `S0 停止分发` 或等价 follow-up。

这条门禁不能解除 `loom-beta-notary` credential、notarized DMG、Gatekeeper 或首次启动 smoke 的 hold；它只证明诊断包 UI 导出路径可被人工安全复核。
