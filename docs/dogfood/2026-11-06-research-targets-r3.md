# Research R3：GitHub 目标与共享获取

2026-11-06。**实施中，不是整体验收。** R0–R3 后端阶段通过；R4–R7 与 E0–E2 未开始。入口为 [实施计划](../plans/2026-10-28/04:41-research-targets-and-experiments.md)。`LOOM_RESEARCH_TARGETS` 仍默认关闭，目标 UI 未接入。

## 交付

- Learn 的公开 GitHub 获取抽到 `src-tauri/src/repository_sources`。调用方传入 `ProcessOwner` 与进程种类：Learn 仍是 study/job + `ProcessKind::Learn`，Research 是 research/run + `ProcessKind::Tool`。Git 只经 `tool_process::run_bounded` 启动，分层检查不再放行 `learn/fetch.rs`。
- URL、ref、浅克隆、检出前 `ls-tree` 限额、加固配置、受控错误和 checkout 指纹与 Learn 原测试保持一致。Learn 包装层不再 spawn。
- Research 的 `github` 目标不要求 REA。预检规范化 `https://github.com/<owner>/<repo>`，拒绝 SSH、其他主机、选项式 ref 和路径逃逸。网站目标仍是 `unsupported_target_collector`。
- 采集把一个 commit 发布为不可变快照，并写入清单证据。文本路径可按 `tree` / `source` 分页；`research_list_report_tree` 与 `research_read_report_source` 只从 reportId 的 `defaultSourceSnapshotId` 解析。符号链接、二进制、vendored 目录和超过 1 MiB 的文件不进入快照，也不跟随符号链接。
- 磁盘统计遇到符号链接时只计链接自身、不递归目标。这是为了让含符号链接的检出不会被误判为整树非法；快照发布仍拒绝把符号链接当源码。

## 验证

本地 Git fixture，不访问 github.com，不消耗模型额度。测试专用 `file://` 覆盖只存在于 `cfg(test)`，生产预检仍先要求 `https://github.com/`。

- 两个分支指向不同 commit，同一 `src/item.rs` 行数相同、内容分别为 `{ 1 }` 与 `{ 2 }`。两份报告的 `report_source` 各读各的内容；用另一 run 的 snapshotId 读取返回 `material_scope_mismatch`。
- 删除旧快照目录后，旧报告正文仍可读取，源码返回 `snapshot_unavailable`。篡改新快照字节返回 `snapshot_integrity_mismatch`，不回退到工作区或另一 commit。
- `../`、绝对路径和检出里的符号链接读取失败，且响应不含仓外标记文本。legacy `source_files` 仍读项目当前文件。
- 假 evidenceOnly CLI 先请求一页树和一行源码，再给出 `loom-src:src/item.rs#L1-L1`。两次运行的补读次数均为 1。

命令与结果：

- `cargo +1.90.0 test --manifest-path src-tauri/Cargo.toml --lib`：**386 passed / 0 failed / 35 ignored**。随后一次全量 `cargo test` 里，`evidence_profile_rejects_changed_configuration_and_unverified_versions` 失败一次：替换后的假 CLI 与原文等长，mtime 未变时配置校验不会拒绝。单独重跑通过。这不是 R3 获取或快照逻辑的失败。
- `cargo +1.90.0 clippy --manifest-path src-tauri/Cargo.toml --lib --tests -- -D warnings` 通过。Rust 1.90 把 `app_update` 里未使用的 `ExitRequested.api` 绑定也算警告；该绑定改为不命名，退出处理逻辑没有变。
- `node scripts/check-layers.mjs` 通过；`tests/unit/layerCheck.test.cjs`、`contract.test.cjs`、`research.test.cjs` 共 11 项通过。
- 未运行真实 github.com 克隆、真实模型、原生桌面、`pnpm tauri build` 或 R4 浏览器链路。忽略的 real/opt-in 测试未计为通过。

本机默认 Cargo 1.83 无法解析当前锁文件里的 edition2024 清单；上述 Rust 结果使用 Rust 1.90.0。
