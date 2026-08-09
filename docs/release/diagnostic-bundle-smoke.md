# Loom Beta 诊断包 UI Smoke

- **状态**: draft
- **适用阶段**: M3 诊断包导出验证
- **关联文档**: `docs/release/diagnostic-bundle-review.md`、`docs/release/privacy-note.md`、`docs/release/beta-feedback-template.md`

## 目标

在邀请制 Beta 发出前，用一次手工 smoke 证明诊断包导出路径可被试用者理解，并覆盖两条关键路径：

1. **默认不导出日志**：确认 JSON 只包含项目、任务、run 摘要和策略判定，不含 `logs` 内容。
2. **主动选择日志尾部**：确认 JSON 只导出项目内 `.loom/logs/` 的尾部，且路径、token、secret、Authorization / Bearer 等内容被替换为 `[PROJECT_ROOT]`、`~` 或 `[REDACTED]`。

这不是完整安全审计；它是 Beta 发布前的可复核门禁。发现疑似敏感信息时停止分发，按 `security` 反馈处理，不附原始诊断包。

## 前置条件

- 使用临时项目，不使用生产仓库、客户仓库或含真实凭据的目录。
- Loom 当前 release 文档已读完：`beta-scope.md`、`beta-safety-notes.md`、`privacy-note.md`、`agent-account-boundary.md`、`diagnostic-bundle-review.md`。
- 已准备一个 fake-only 任务，任务描述、命令或 fixture 日志中可以包含这些假值：
  - `OPENAI_API_KEY=fake-token-for-smoke`
  - `Authorization: Bearer fake-bearer-for-smoke`
  - 项目根路径下的测试文件路径
- 导出目标放在临时目录；验证完成后删除导出的 JSON。

## 路径 A：默认不导出日志

1. 启动 Loom，并打开临时项目。
2. 进入 Settings 或已完成任务的交付页。
3. 确认“包含日志尾部”之类的开关处于关闭状态。
4. 点击“导出诊断包”，保存为 `loom-diagnostics-no-logs.json`。
5. 用文本编辑器打开 JSON，检查：
   - `project.path` 为 `[PROJECT_ROOT]`。
   - `logs` 为空数组。
   - `omittedLogCount` 为 `0` 或可解释的非负数。
   - 不出现临时项目的真实绝对路径。
   - 不出现 `fake-token-for-smoke`、`fake-bearer-for-smoke` 或其他 fake secret 原文。
6. 若任一检查失败：删除 JSON，记录 `security` / `S0 停止分发`，不要发送诊断包。

## 路径 B：主动选择日志尾部

1. 在临时项目中制造一条仅包含 fake secret 的本地日志，日志必须位于项目内 `.loom/logs/`。
2. 回到 Settings 或已完成任务交付页。
3. 打开“包含日志尾部”开关。
4. 点击“导出诊断包”，保存为 `loom-diagnostics-with-log-tail.json`。
5. 用文本编辑器打开 JSON，检查：
   - `logs` 最多 20 条，每条只包含尾部内容。
   - 日志中项目根路径显示为 `[PROJECT_ROOT]`，HOME 路径显示为 `~`。
   - fake token、fake bearer、password、secret、Authorization 原文不出现，只允许字段名或 `[REDACTED]` 出现。
   - 不包含 `.loom/` 目录以外的任意文件内容。
   - 如日志过多，`omittedLogCount` 能解释省略数量。
6. 若任一检查失败：删除 JSON，把反馈分类为 `security`，严重度至少标记 `S0 停止分发`。

## 记录模板

```md
# Diagnostic bundle smoke record

- Date:
- Loom commit:
- App version:
- Platform:
- Export surface: Settings / Done pane
- Temporary project type:

## Path A: no logs
- Result: pass / fail
- Export file reviewed manually: yes / no
- `logs` empty: yes / no
- Real project path absent: yes / no
- Fake secrets absent: yes / no
- Notes:

## Path B: with log tails
- Result: pass / fail / skipped
- Export file reviewed manually: yes / no
- Log tail count:
- Real project path absent: yes / no
- Fake secrets absent: yes / no
- Limit behavior observed: yes / no / not applicable
- Notes:

## Decision
- Beta gate: pass / hold
- Follow-up issue or doc path:
```

## 发布门禁

邀请制 Beta 可以继续向更大范围推进前，至少需要一条已填写的 smoke record，并满足：

- 路径 A 通过。
- 路径 B 通过，或明确说明本轮构建未暴露日志开关且不会向试用者承诺日志导出。
- 手工搜索未发现真实路径、真实 token、fake secret 原文或生产数据。
- 若失败，已在 `beta-feedback-template.md` 的 `security` 分类下记录停止分发结论。

## 与工程测试的关系

`src-tauri/src/diagnostics.rs` 已有单元测试覆盖 `build_bundle` 的脱敏、路径替换、日志默认不导出和日志尾部路径；2026-08-09 的复跑记录见 `docs/release/diagnostic-bundle-engineering-proof.md`。UI smoke 只补工程测试没有覆盖的部分：用户是否能找到入口、开关状态是否符合文档、保存后的 JSON 是否能由人类按清单复核。
