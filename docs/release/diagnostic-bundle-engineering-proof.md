# Loom Beta 诊断包工程脱敏证据

- **状态**: recorded
- **适用阶段**: M3 诊断包导出验证
- **关联文档**: `docs/release/diagnostic-bundle-review.md`、`docs/release/diagnostic-bundle-smoke.md`、`docs/security-policy.md`

## 结论

2026-08-10 08:46 复跑并扩展诊断包相关 Rust 单元测试，结果通过：

- 默认不勾选日志尾部时，诊断包 `logs` 为空，`omittedLogCount` 为 `0`。
- 勾选日志尾部时，只读取项目内 `.loom/logs/` 引用；项目根路径替换为 `[PROJECT_ROOT]`；指向项目外目录的日志引用不会进入导出 JSON，也不会因为最近若干条无效引用占满 20 条限额而挤掉更早的有效项目内日志。
- 命令、阻塞原因和日志尾部中的 token / secret / Authorization / Bearer 类文本会被替换为 `[REDACTED]`。
- 测试 JSON 中不包含临时项目真实路径，也不包含 fake secret 原文。

这条证据只能证明后端 bundle 构造和脱敏 fixture 仍然有效；它不替代 `docs/release/diagnostic-bundle-smoke.md` 要求的 UI 手工 smoke。Beta 扩大分发前仍需要从 Settings 或 Done pane 实际导出 JSON，并按清单人工搜索。

## 执行命令

```bash
cd src-tauri
cargo test diagnostic_bundle --lib
```

## 输出摘要

```text
running 4 tests
test diagnostics::tests::diagnostic_bundle_ignores_logs_outside_project_log_dir ... ok
test diagnostics::tests::diagnostic_bundle_invalid_recent_log_refs_do_not_starve_valid_logs ... ok
test diagnostics::tests::diagnostic_bundle_omits_logs_unless_user_selects_them ... ok
test diagnostics::tests::diagnostic_bundle_redacts_secrets_paths_and_log_tails ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 189 filtered out; finished in 0.02s
```

## 覆盖点

| 覆盖点 | 工程证据 | Beta 语义 |
|---|---|---|
| 默认不导出日志 | `diagnostic_bundle_omits_logs_unless_user_selects_them` | 用户未主动选择时不把 stdout/stderr 尾部写入诊断包 |
| 路径脱敏 | `diagnostic_bundle_redacts_secrets_paths_and_log_tails` | 项目根路径以 `[PROJECT_ROOT]` 出现，不暴露真实目录 |
| secret 脱敏 | `diagnostic_bundle_redacts_secrets_paths_and_log_tails` | fake token / Authorization / secret 样例不以原文出现在 JSON |
| 日志来源边界 | `diagnostic_bundle_redacts_secrets_paths_and_log_tails` 使用项目内 `.loom/logs/` 引用；`diagnostic_bundle_ignores_logs_outside_project_log_dir` 证明项目外日志引用被忽略 | UI smoke 仍需确认开关文案和保存文件符合试用者预期 |
| 日志限额语义 | `diagnostic_bundle_invalid_recent_log_refs_do_not_starve_valid_logs` 证明项目外 / 不可读引用先被过滤，20 条限额只作用在可导出的项目内日志上 | 最近的无效日志引用不会把有效诊断证据挤出 JSON；`omittedLogCount` 只说明可导出日志超过限额后的省略数量 |

## 当前仍不能解除的门禁

- **UI 可发现性未证明**：本轮没有从桌面 UI 点击导出，只跑了后端单元测试。
- **保存后的 JSON 人工复核未完成**：还没有一条已填写的 smoke record。
- **真实反馈流程未演练**：尚未用 `security` 分类记录“疑似敏感信息时不附诊断包”的样例。
- **分发 gate 仍受 signing / notarization 影响**：即使诊断包后端测试通过，当前 Beta 产物仍处于 `docs/release/signing-gate-decision.md` 记录的 Gatekeeper hold。

## 下一步

1. 保留本文件作为工程 fixture 证据。
2. 下一轮若不具备 notarization credential，优先执行 `docs/release/diagnostic-bundle-smoke.md` 的 UI 手工 smoke：从 Settings 或 Done pane 导出无日志 / 含日志尾部两份 JSON。
3. UI smoke 记录通过后，再更新 `docs/release/diagnostic-bundle-review.md` 的 Beta 发布门禁状态。
