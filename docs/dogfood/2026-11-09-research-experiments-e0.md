# Research E0：实验基础与基线

2026-11-09。**不是 B 线验收。** 只交付实验基础。E1 候选循环、E2 实验界面和 R7 原生桌面门禁都没有开始。

## 交付

- `src-tauri/src/experiments/` 不引用 Research。`LOOM_RESEARCH_EXPERIMENTS` 默认关闭；非测试构建还要求 macOS。能力字段 `experiments` 使用同一条件。没有实验界面，也没有新的实验 IPC。
- 快照只复制调用方列出的相对普通文本。`package.json`、`node_modules`、符号链接、`..` 和含 NUL 的文件被拒绝。复制后再次读取源文件，内容必须仍与副本一致。脏 Git 状态返回 `workspace_dirty`，不自动清理。
- `node-cpu-v1` 用整数纳秒和交叉乘法判定。固定表覆盖 keep、恰好 3% 的 keep、未达门槛的 discard、持平／退步、threshold_overlap、2% spread 的 no_improvement、unstable_samples，以及 0 和样本不足的 invalid。连续 3 次不确定或无效会给出停止原因；确定的 keep/discard 清零不确定计数。
- 校准从 `N=1` 按 2 倍增加，选第一个至少 50ms 的批次，上限 65536。基线先查正确性，再预热 3 批、采集 5 批。正确性失败、样本无效和 spread 超过 2/100 都有原因，不给出优化分数。
- Node 只通过 `tool_process` 启动，程序必须是绝对路径且文件名为 `node`。工作目录是受管副本。

## 验证

- `cargo test --manifest-path src-tauri/Cargo.toml --lib experiments::`：**6 passed**。其中包括判定表、校准、脏工作区不清理、正确性失败，以及对本机 Node 22.23.4 的基线计时。计时后原项目 `git status --porcelain` 为空。
- 本机基线可能是 Ready，也可能因 spread 被标成 Unstable；两种结果都带说明，测试接受这两种可解释结果。
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings` 通过。`node scripts/check-layers.mjs` 通过。
- 能力测试确认默认 `experiments` 为假。没有跑实验 UI、候选循环、真实模型或 macOS 桌面。
