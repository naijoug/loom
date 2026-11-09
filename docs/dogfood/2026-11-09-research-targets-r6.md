# Research R6：有限连续调研

2026-11-09。**实施中，不是整体验收。** R0–R6 已接入；R7 与 E0–E2 未开始。入口为 [实施计划](../plans/2026-10-28/04:41-research-targets-and-experiments.md)。`LOOM_RESEARCH_TARGETS` 仍默认关闭。连续模式另需 macOS 上的 `LOOM_RESEARCH_ITERATION=1`。`researchExperiments` 保持关闭。

## 交付

- 单次研究仍走固定采集和原来的 `answer` / `need_material` 报告。`mode = iterative` 在开关关闭时预检拒绝。
- 连续循环只接受 `next_action`。动作限于 `inspect_app`、`analyze_js`、`capture_page`、`inspect_web_scripts`、`read_evidence`、`finish`，并且必须对应当前冻结目标。证据 ID 和假设引用必须已经发布。
- shell、任意路径、另一条 URL 或预算字段使 JSON 无效，最多纠正一次。目标不匹配的动作记为 `unsupported`，不执行。
- 假设只记录 `support`、`refute`、`unknown`，保存在 `runs/<runId>/iteration.json`。重复读取同一证据不算新覆盖；连续两轮没有新覆盖后写部分报告并停在 `no_progress`。轮数超过预检预算时停在 `budget_exhausted`，不执行这一轮。
- 失败后的新 attempt 沿用同一 run 的轮数和假设文件。追问仍被改回单次，采集额度为 0。

## 本轮验证

- 本地 Git 仓库夹具走生产循环：第一轮固定检出，第二轮只重读已发布证据，然后 `finish`。`usage.rounds >= 2`，停止原因 `objective_addressed`，报告同时含 support、refute、unknown。源码仍是固定提交里的 `src/item.rs`。
- 同一夹具连续重读后停在 `no_progress`。`max_rounds = 1` 时第二次动作不执行，停在 `budget_exhausted`，轮数仍是 1。对 GitHub 目标发出 `capture_page` 停在 `unsupported`，轮数不增加。
- Rust 1.90：`cargo test --lib` 并行结果 **397 passed / 1 failed / 36 ignored**。失败的是既有并行抖动 `evidence_profile_rejects_changed_configuration_and_unverified_versions`，单独重跑通过。`cargo clippy --all-targets --all-features -- -D warnings` 通过。定向的连续循环、迭代单元和单次 GitHub 回归通过。
- 前端 `research.test.cjs` 与 `contract.test.cjs` 12 passed。连续选项默认不出现；勾选后预检才变成 `iterative`，且该测试没有调用 start。
- 未跑真实模型、联网 GitHub、原生桌面、`pnpm check` 或 `pnpm tauri build`。假 CLI 只按夹具脚本返回 NextAction，不代表模型会稳定遵守该 JSON。
