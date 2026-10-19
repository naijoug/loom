# Loom 项目优化验收记录（2026-08-05）

## 结论

`docs/plans/2026-08-05/11:14-project-optimization.md` 的 M0–M5 已全部完成。工程基线、持久化迁移、Tauri 契约、进程监督、任务仓储、前端流程拆分、分层测试、界面认知层、脱敏诊断和性能预算均已有可复跑证据。

## 本轮 M5 交付

- 核心界面采用中文默认文案，保留 Agent、Review、CLI、stdout/stderr 等技术专名；新建项目/任务、阶段回看、生命周期、状态与退出码同步收口。
- 规划确认、进入测试验收、确认验收分别保留单一主操作；不能继续时展示结构化原因。Testing 修复路径与历史默认降低层级，验收门禁优先显示。
- 新增 `export_diagnostic_bundle`：导出版本、系统、任务/run 元数据和策略判定；项目路径与敏感字段脱敏。日志默认不导出，用户选择后限制为最近 20 份、每份 200 行、最多读取 64 KiB 尾部。
- 视觉差异从 8×6 提升为 24×16 感知网格，并以 gzip+base64 存储 30 屏基线；平均差异预算 1.2%，变化网格预算 8%。
- 新增 `pnpm bench:local`，以非零退出码执行冷启动、项目扫描、日志 reducer、长时间线建模/渲染和 RSS 预算。

## 验证结果

| 门禁 | 结果 |
|---|---|
| `pnpm check` | 通过：前端 100/100；Rust 189/189，2 项显式忽略；Rust workflow integration 2/2；fmt、Clippy、契约一致性通过 |
| `pnpm smoke` | 13 个核心浏览器页面通过 |
| `pnpm smoke:interaction` | 项目/任务弹窗、规划、看板、Settings、实施与测试验收交互通过 |
| `pnpm smoke:visual` | 新基线建立后独立复跑 30/30 通过；最大自然波动平均 0.26%、变化网格 2.3% |
| `pnpm bench:local` | 6 项硬预算全部通过，详见 `performance-baseline-2026-08-05.md` |
| `pnpm tauri build` | `Loom.app` 与 arm64 DMG 构建成功 |
| `pnpm smoke:desktop` | release 应用启动/退出两轮通过 |

## 发布产物

- DMG：`src-tauri/target/release/bundle/dmg/Loom_0.1.0_aarch64.dmg`
- SHA-256：`a5c49ae939892498156ad6a07abb6a119fc658a39a4a95c8b416fa8e4e1bbccc`

## 安全与边界

- 诊断 secret fixture 验证命令、日志、阻塞原因中的 token 均不会出现在 JSON；项目根路径替换为 `[PROJECT_ROOT]`，HOME 替换为 `~`。
- 未执行真实付费 Agent canary；该路径继续保持显式手工触发。
- 产物未做 Apple Developer ID 签名、公证或公开分发；这些属于后续 Beta 发布计划，而非本轮稳定化完成条件。
- Windows/Linux 仍未纳入本轮发布矩阵。
