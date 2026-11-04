# Agent 层与功能栏 P8 收尾验收

开始：2026-10-27；收尾：2026-10-28（America/Los_Angeles）。基线 `main` @ `1ff6e6b` + 本会话 P7 未提交工作区；P8 继续保留其改动。结论：P8 工程文档与本轮验收已收尾，原生重启空白的一次异常仍作为未定位风险保留。范围是 Agent 层/功能栏计划 P8，不含发布分发、Skills 真实账号解阻、Learn L6 或冻结内核。

## 本轮改动

- 新增 `agent/lifecycle.rs`，薄封装 begin_operation / begin_shutdown / request_stop / force_stop / shutdown_snapshot / is_shutting_down；Chat、Learn、Research 的生产 Agent 生命周期统一走公共接口。保留原始结果/信号/锁/退出预算，不改 Chat v2 或 CLI 参数。
- 修复 `check-layers.mjs` 的漏扫：旧实现遇首个内联测试模块便截断整个文件，`chat/runtime.rs` 后半部的生产 runner 未被检查。新辅助模块屏蔽测试、字符串与注释，保留其后代码，并禁止功能直接引用 supervisor / Agent 内部模块（Learn Git 具名例外）。新增 2 个回归。
- 新增 [Agent 层契约](../architecture/agent-layer.md)：当前接口、数据流、进程归属、错误、缓存、边界与例外清单。同步架构、适配器、安全、Learn 契约、需求、测试、脚本说明和计划索引。

## 完整工程门禁

| 检查 | 当前结果 | 边界 |
|---|---|---|
| `pnpm check` | passed：前端 145，Rust 322，0 failed / 30 ignored | 包含层级、契约、特征快照、fmt、Clippy、build、release 文档检查；原生测试另列 |
| `pnpm tauri build --bundles app` | passed | 当前 release Loom.app，供原生验收；未作分发签名/公证验收，未发布 |
| `pnpm tauri build --no-bundle` | passed | 当前 release 二进制；与原生包采用相同产品代码 |
| `pnpm smoke:visual` | passed | 本轮重新运行全部 Chat / 功能栏 / Learn / Research / Discuss / Skills / OpenUI 合成矩阵；debug.sh 前后清理 |
| 文档链接与差异检查 | passed | 新/修改文档相对链接、release 文档、计划索引、源码哈希与 git diff --check |

验收文件在 [samples/p8-2026-10-27](samples/p8-2026-10-27/starting-worktree.json)，目录日期为开始日期；原生截图在 `.designs/exports/p8-desktop/`。

## 原生桌面（真实 IPC、真实 CLI）

产物固定为本仓库 `src-tauri/target/release/bundle/macos/Loom.app`，通过 debug.sh 启动；调用已配置的 Codex 0.164.0 和 Claude Code 2.1.311，不改 CLI/凭据配置。临时 profile、临时项目路径保存在机器摘要。`LOOM_PROFILE_DIR` 不覆盖功能数据目录：新建的 Research/Discuss 记录仍在应用 data 目录；既有 Learn 记录只读打开，未重新生成或删除。

| 场景 | 观察结果 | 状态 |
|---|---|---|
| 启动首页 | 实际启动进入 Chat，四功能与 Settings 可见 | passed |
| Chat 运行中切页 | 真实 Codex 回合启动后，依次 Learn/Research/Discuss/Settings，返回原会话，回答包含 P8-CHAT-DONE，耗时 32.9 s | passed |
| Chat 停止后再发 | 原生 Stop generation 终止回合（16.2 s）；下一轮显示 P8-STOP-RECOVERED（27.4 s） | passed |
| Chat 草稿/历史 | 草稿切到 Discuss 再回保持；应用重启后打开原会话，历史与草稿仍在 | passed |
| Chat 重启续聊 | 原会话发送后返回 P8-RESTART-OK（29.7 s） | passed |
| Learn | 原生打开既有公开仓库 Overview，引用定位 `src/minisweagent/__main__.py:7`；未发起新生成 | passed（阅读/引用） |
| Discuss 双 Agent | 原生表单明确确认 5 次，Codex/Claude 两轮均完成；切页后台运行正常 | passed |
| Discuss 停止/重试 | 点击 Stop all 时第二轮刚完成，实际停止的是汇总；仅重试汇总并确认 1 次，最终 ready、5 项 done、汇总两次尝试 | passed |
| Discuss 重启/日志 | 重启后两轮与汇总可读，尝试选择器可读取第一次已停止日志 | passed |
| Research 首问 | 无项目、无工具，原生显示 P8-RESEARCH-DONE 与结构化报告 | passed |
| Research 运行中退出 | 追问显示 Running 后立即 Cmd+Q，磁盘第二问为 interrupted；重启原生显示 Interrupted，未自动重启 | passed |
| Research 重试/停止 | 点击 Retry 后进入真实 running；长输出验收请求主动停止，不把它当完整成功报告 | passed（控制链路） |
| Research 停止后追问 | 新发短追问成功返回 P8-RESEARCH-RECOVERED，UI 明示 Earlier questions replayed；第三问状态 done/context=replay | passed |

所有新增议题仅是无敏感内容的队列概念题，禁止工具/读取文件，不请求修改项目。Discuss 建议只作为报告展示，没有被执行。

## 异常与限制

- 第一次重启出现一次空白 WebView：窗口/菜单存在，但没有 HTML accessibility 内容，截屏为空白；进程仍存活且菜单响应。未清数据、未改代码，退出再启动恢复。之后正常退出/重启也恢复正常。**根因未定位**，后续成功不等于该问题已修复；作为原生稳定性剩余风险保留，不能据此宣称无故障发布。
- 初始临时 recent-projects fixture 使用 `/var` 别名，Chat 正确拒绝访问未规范登记的项目；随后通过 UI Add project 完成 canonical `/private/var` 注册。属于验收准备问题，未修改产品权限校验。
- 第一次试图“汇总运行中退出”时模型已先完成，因此只能计为完成后的正常退出；真正运行中退出由后续 Research 场景覆盖，不能混写。
- 本次未执行系统强杀、断电、真实删除确认提交、四家 CLI 的全量桌面矩阵或签名升级；故障恢复有离线测试，真实桌面只覆盖表中观察到的情形。
- 继承 P1/P4/P7 的 Skills Git fixture 偶发 killpg EPERM 风险未在本轮修改；P8 完整门禁本轮通过。

## S1–S8 证据映射

| 标准 | 结论与证据层级 |
|---|---|
| S1 Agent 边界 | 公共生命周期收口，层级检查通过；Learn Git、Skills 原始翻译、应用级入口的例外在契约逐项列出 |
| S2 功能隔离 | Rust/前端分层检查通过；Settings→Skills 为既定产品结构例外 |
| S3 Chat 行为 | 特征快照和完整回归通过；本轮原生真实运行/停止/再发/历史验证，重启续聊按上表结果 |
| S4 Learn 行为 | 当前离线/引用链路通过；P3 的真实 hexyl 全部 Done/引用有效记录复用，未重跑全量模型生成；本轮原生阅读/源码定位通过 |
| S5 坏 CLI | 现有坏安装、缓存和错误组件测试通过，版本探测不能代替登录验收 |
| S6 外壳 | 本轮真实 Chat 运行中切四功能/Settings、草稿保持；合成 en/zh×主题×窄窗按最终 smoke 记录 |
| S7 Research | P6 真实项目引用/原生续接证据；本轮原生首问、退出中断/重启/重试/停止，短追问按上表结果 |
| S8 Discuss | P7 伪 CLI 并发/部分失败/重试，真实双 CLI 5/5；本轮原生双 CLI 两轮、调用确认、停止汇总/单次重试与重启历史 |

## 已核实的文档/代码差异

| 文档位置 | 代码位置 | 差异与处理 |
|---|---|---|
| 计划 §3 S1、P3 收尾项 | `chat/{mod,shutdown,service,repository,runtime}.rs`、`learn/jobs.rs`、`research/jobs.rs` | 生产代码仍直用 supervisor；P8 新增 Agent 生命周期封装并改调用，Learn Git 明确例外 |
| 计划 P3/P5 的分层检查描述 | `scripts/check-layers.mjs`、`chat/runtime.rs` | 旧 scanner 直接截掉首个测试模块之后所有内容，实际未覆盖后半生产代码；修复并新增 2 个回归 |
| `docs/agent-adapter.md` 启动缓存描述 | `agent/registry.rs::load_agents_inner/save_agents` | 读取/合并 Agent 时也保存并清空缓存，不能假定跨每次请求一直命中；按代码补充文档，不改变缓存策略 |
| `docs/architecture/learn-contracts.md` 源码引用段 | `components/Markdown.tsx`、Research/Discuss 报告组件 | 文档仍称只有 Learn 开启源码链接，代码已共享给 Research/Discuss；修正文档及公共 source_files 说明 |
| 计划 §5.1 目标接口 | `agent/{mod,invocation,runner,event}.rs` | 实际保留 AdapterInvocationRequest/AgentStage/RunSpec、三类 AgentEvent 和单独 Observation；新契约按代码明确，不在收尾为命名做无需求重构 |

产品仍保留 Task 数据，不启用冻结内核。本轮不提交、推送、发布；分发验证及旧范围的未通过项不因 P8 收尾自动转 passed。

## 本轮原生证据

[观察摘要](samples/p8-2026-10-27/native-observations.json)、[Chat 持久化](samples/p8-2026-10-27/native-chat.json)、[Research 持久化](samples/p8-2026-10-27/native-research.json)、[Discuss 持久化](samples/p8-2026-10-27/native-discussion.json)。仅保存本轮创建的合成议题数据，不复制既有学习资料或账号配置。原生 AX 证据同目录，截图本地保存在 `.designs/exports/p8-desktop/`。

Research 的长输出（200 点）专用于运行中退出/重试/停止验收，重试约数分钟后主动停止；未把它标作报告生成成功。后续短追问真实完成。所有相关模型运行均未调用工具。

工程日志：[完整检查](samples/p8-2026-10-27/check.txt)、[app 构建](samples/p8-2026-10-27/bundle.txt)、[no-bundle 构建](samples/p8-2026-10-27/build.txt)、[合成矩阵](samples/p8-2026-10-27/visual.txt)、[机器摘要/哈希](samples/p8-2026-10-27/summary.json)。

## 后续跟进（2026-10-28）

空白窗口已取得限定 PID 的系统日志：主页面曾完成加载与首次绘制，底层触发仍未复现。随后修复应用缺少界面就绪判定/恢复入口的缺口，提供启动阶段一次自动重载、原生恢复提示与显式手动重载；两类故障注入和五次正常原生重启通过。详见 [后续验收](2026-10-28-webview-restart.md)，不能把恢复能力的验证改写成本记录中的原始故障已完全定因。
