# Loom 当前进度复核（2026-10-26）

结论：当前范围内的主要工程实现已完成，整体产品验收仍为 partial。本轮继续补齐浏览器验收缺口与进度资料，没有恢复已取消的旧数据迁移或冻结内核。

## 基线与范围

- 开始工作时工作区干净，本地 `main` 为 `ffe3127909129ccf220f7d8e8894103b15cfe576`。
- `git branch --contains 3dd9a9f` 包含 `main`；本地 `origin/main` 也指向 `ffe3127`。本轮未 fetch，不把本地 tracking ref 当作实时远端状态。
- 本轮仅改合成浏览器脚本和文档，不改产品代码、旧 Task 数据、用户 Skills 安装或账号配置。没有调用真实模型、提交、推送或发布。

## 已完成与剩余工作

| 范围 | 当前状态 | 证据与剩余项 |
|---|---|---|
| 简化 CLI Chat | 主要工程完成，产品验收 partial | 默认入口为 Chat/Settings；项目分组、权限选择、流式/停止/续聊、工具卡片、GFM 与语言主题已有实现。当前离线门禁通过；P9 真实桌面完整矩阵仍由用户接手 |
| Skills 管理 | 功能分项迁入，真实验证有阻塞 | 来源、安装/更新/卸载、共享指令、v2 恢复、CLI/TUI/ng、翻译已有工程证据；四 provider 账号未核实，M3-04 保持 blocked |
| 旧 Manager 退役 | 历史记录显示完成 | 按 10-25 更新范围删除管理代码，取消旧数据迁移；本轮不重新操作源 skills 仓库 |
| OpenUI 卡片 | 实验，不转正 | 默认关闭，合成交互本轮通过；历史真实出卡率未达标，桌面交互验收未收口 |
| 分发与升级 | 未完成发布验收 | 当前 no-bundle 构建通过；签名、公证、安装与两版本真实升级不在本轮完成证据中 |
| Workbench / 新内核 / SDK worker | 冻结 | 不进入本轮推进范围；Task/Board/Planning 不属于当前产品 |

账号阻塞依据 [M3-04 记录](samples/skills-migration/m3-provider-acceptance.json)：此前用户选择不调用未核实账号。G1 桌面源码隔离的历史阻塞保持不变；CLI 隔离不能替代桌面。OpenCode 原安装签名与 Grok 402 属于 [10-20 历史限制](2026-10-20-simple-chat-acceptance.md)，本轮没有复验，不断言当前环境仍然如此。

## 本轮继续完成

1. 执行当前 main 的完整门禁与桌面 release no-bundle 构建，未发现失败。
2. 在 `scripts/simple-chat-smoke.mjs` 增加双项目折叠/展开刷新恢复。先反转两个项目的默认状态，等待 UI 生效，再刷新核对各项目的持久化状态；默认值重置无法通过此断言。
3. 修改脚本后重新执行完整合成矩阵。只补齐浏览器刷新证据，不将其写成应用重启通过。
4. 校正两份 Chat 计划和索引中的当前分支状态，保留历史交付记录，回填 P9 的合成证据。

## 本轮验证

| 命令 | 结果 | 边界 |
|---|---|---|
| `pnpm check` | passed：前端 125、Rust 265，0 失败，25 ignored | 包含 build、fmt、Clippy、release 文档和 OpenUI 提示词检查；在补充 smoke 脚本之前运行，之后产品代码未变 |
| `pnpm tauri build --no-bundle` | passed | 生成 `src-tauri/target/release/loom`；不含安装包、签名、公证或升级验收 |
| `pnpm smoke:visual` | passed | 补充脚本后运行；Chat 10 项、Skills 5 项、OpenUI 10 项分类检查通过，均为 synthetic-web-only |
| `scripts/debug.sh status` | 已清理 | smoke 使用 debug.sh 启停；网页与桌面 dev 均未运行 |

持久化证据：[机器摘要](samples/progress-2026-10-26/summary.json)、[完整门禁日志](samples/progress-2026-10-26/check.log)、[构建日志](samples/progress-2026-10-26/build.log)、[合成回归日志](samples/progress-2026-10-26/visual.log)。各矩阵 JSON 同目录保存。日志 SHA-256 和补充脚本 SHA-256 写入摘要；截图生成于 `designs/exports/`，本轮无界面修改，未保留重复生成的 PNG 差异。

## 文档与代码差异及处理

| 文档位置 | 对应代码/版本证据 | 具体差异 | 处理 |
|---|---|---|---|
| `docs/PLANS.md` 的 10-20 实施计划摘要 | 本地 main @ ffe3127 含 3dd9a9f；`src/App.tsx` 的 Chat/Settings 入口 | 当前摘要仍称“未合入 main” | 改为已核实本地 main 包含实现，保留桌面未验收状态 |
| `docs/plans/2026-10-20/01:01-simple-chat-implementation.md` 开头状态 | 同上；`src/main.tsx` 默认挂载 App | 当前状态仍称实现仅在分支、未入 main | 更新开头状态，追加本轮证据；历史过程文字不改写 |
| `docs/plans/2026-10-20/00:44-simple-cli-chat-refactor.md` 开头状态 | 同上 | 当前状态仍称未入 main | 更新开头与索引摘要，追加本轮证据；“本轮不自动合并”保留为当时决定 |

10-20 dogfood 的“未合入 main”明确改为历史基线描述并追加当前复核；没有追溯修改旧门禁计数或将桌面 partial 改成 passed。

## 提交补记

上文"没有提交、推送"指复核当时状态。随后 `main` 先提交了 `0059f43`（smoke 的 Chrome 防节流参数与窄屏事件诊断），本复核的脚本补充与文档再在其后单独提交。摘要中的 `smokeScriptSha256` 对应 `ffe3127` 加本轮补充，不等于最终提交版本；合并后的脚本已在提交前重跑 `pnpm smoke:visual` 并通过（synthetic-web-only）。
