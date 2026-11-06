# Research R5：单次证据界面

2026-11-06。**实施中，不是整体验收。** R0–R5 的单次阶段已接入；R6–R7 与 E0–E2 未开始。入口为 [实施计划](../plans/2026-10-28/04:41-research-targets-and-experiments.md)。`LOOM_RESEARCH_TARGETS` 仍默认关闭。`research_target_capabilities` 把 `iteration` 和 `experiments` 固定为 false，表单也只提交 `mode = single`。

## 交付

- macOS 且 `LOOM_RESEARCH_TARGETS=1` 时，新建页可以选择单次目标：GitHub、网站或本地 App。先预检，再开始。网站在预检前和预检结果中都写明会加载页面并运行脚本。
- `workflow = target` 的研究使用目标命令。停止调用 `research_cancel_run`，重试调用 `research_retry_run`，追问调用 `research_ask_run`。这些按钮不调用 `research_ask` / `research_retry` / `research_cancel`。
- 开关关闭时仍可阅读已保存的目标报告、证据和快照，也可以停止活动 run。追问、重试和恢复不再显示。
- 源码和证据都带当前 `reportId`。同一研究里的另一份报告读取自己的快照。证据区只显示脱敏文本；截图不画进页面。导出默认不含原始证据，勾选后显示目标数据仍可能留在包内。
- 普通问答表单和旧研究视图保持原命令。

## 本轮实际验证

合成界面，不是真实 CLI、真实 REA 或原生桌面：

- `pnpm exec tsc -p tsconfig.json --noEmit` 通过。
- `node scripts/run-tests.mjs tests/unit/research.test.cjs tests/unit/contract.test.cjs tests/unit/learn.test.cjs tests/unit/chatMarkdown.test.cjs`：22 passed。其中 Research 8 项覆盖旧问答、三类目标预检/开始、reportId 源码不串读、证据、追问、停止、导出和失败重试。
- Rust 1.90.0：`cargo test --manifest-path src-tauri/Cargo.toml --lib` 为 **392 passed / 0 failed / 36 ignored**。`cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings` 通过。
- `CHROME=/usr/local/bin/google-chrome scripts/simple-chat-smoke.sh` 通过。Research 合成矩阵包含旧问答、停止/重试、中英文主题、700px，以及 `researchFixture=targets` 的三类目标、快照切换、只读追问、停止/重试和中文深色窄窗口。Chat、Learn、Skills、Discuss 的同轮合成烟测也通过。这不是桌面 WebView。

## 单次交付门禁里本轮没有宣称的部分

- A1：界面夹具能走完三类目标。没有用真实 CLI 对真实 GitHub、网站或本地 App 生成报告。
- A2：夹具里两份报告的 `src/main.rs` 文本不同。真实 commit hash 不匹配仍以 R3 后端测试为准，本轮没有重跑。
- A3：旧 Research 与 Chat 的合成回归通过。没有重做 v1 研究迁移。
- A4：开关关闭时的阅读和停止有前端测试。没有做重启或第二实例。
- A5、A6：沿用 R3/R4 的后端边界。本轮只确认停止按钮走 `research_cancel_run`，没有新的进程回收实验。
- A7：连续调研未做，不宣称。
- A8：英文/中文、深色、700px 和键盘在合成网页通过。没有原生桌面，也没有单独把“工具缺失”做成一条视觉用例；网站和 App 在界面上必须填写 Node 与 REA 入口，后端预检仍会拒绝缺失工具。
- 未运行 `pnpm check` 与 `pnpm tauri build --no-bundle`。
