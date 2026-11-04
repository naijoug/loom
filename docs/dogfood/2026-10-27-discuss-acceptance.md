# Discuss P7 验收记录（2026-10-27）

- 计划：[Agent 调用层与功能栏](../plans/2026-10-26/22:42-agent-layer-and-activity-rail.md) P7；契约：[Discuss](../architecture/discuss-contracts.md)。
- 基线：`main` @ `1ff6e6b` 上的 P7 工作区改动；开工时工作区干净。本轮未提交、未推送或发布。
- 用户授权按计划完成 P7、验证并更新进度；按默认 2–4 人、两轮 + 汇总、首位主持执行。
- 结论：功能实现、定向测试、合成网页和双 CLI 真实讨论已通过；最终完整门禁与桌面 release 构建通过。桌面人工操作仍为 not-tested，不代表 P8 总体验收完成。

## 实现与验证

| 范围 | 结果 / 证据 |
|---|---|
| 持久化与编排 | 独立 discuss/，同轮并行与轮次屏障，先检查启动再经 agent::run，2–4 人和调用数后端校验，单项失败不丢失其他回答 |
| 生命周期 | 一个活动编排、全部停止、逐项重试、上游变更导致后续过时、保留各次尝试；重启把未完成项标中断，不自动调用 |
| 界面 | 功能栏第 4 项/快捷键；参与者、模型、项目、语言；开始/重试调用确认；轮次对照、汇总、缺席与过时提示、引用、日志、删除确认 |
| Rust 定向 | 首轮 9 项通过；追加四参与者 9 次调用与只读参数、项目引用有效/逃逸场景后由完整门禁覆盖 |
| 前端与契约定向 | 9 项通过；本轮新增 3 个 Discuss 组件用例，既有功能栏及契约扩展 |
| 完整 `pnpm check` | passed：前端 143、Rust 322，0 failed / 30 ignored；包含分层、fmt、Clippy、build、契约与 release 文档检查 |
| `pnpm tauri build --no-bundle` | passed；最终代码 release 二进制 `src-tauri/target/release/loom`；未签名、公证或分发 |
| `pnpm smoke:visual` | passed；Chat、功能栏、Skills、Learn、Research、Discuss、OpenUI 全部合成矩阵；[Discuss 结果](samples/discuss-2026-10-27/visual.json) |
| 原生桌面 | not-tested；待验证真实运行切功能、退出中断、恢复重试与原生窗口交互 |

合成截图位于 `.designs/exports/discuss/`。人工查看宽屏独立回答和 700px 中文深色截图：并排/堆叠符合布局，无横向溢出。旧 Skills 截图重复生成的差异未计入本次代码修改。

## 双 CLI 真实讨论

使用与桌面命令相同的 `Pipeline`，无项目、Loom 受管空目录、只读权限；议题为小型桌面应用的并发/串行讨论及失败处理，不读取项目或执行建议。原生 UI 未参与。

- Codex CLI 0.164.0：登录状态为 ChatGPT。
- Claude Code 2.1.311：登录状态为 firstParty/oauth_token。本轮真实成功，不沿用 Learn 旧记录中的认证阻塞结论；未修改用户 CLI 或账号配置。
- 清除宿主继承的 ANTHROPIC_/CLAUDE_ 变量及 CLAUDECODE 后运行，使 CLI 使用自己的已配置环境；不改 HOME/CODEX_HOME。
- 预计 5 次，实际完成 5 次；每项退出码 0、工具调用 0，总用时 136.69 秒。各轮原生会话独立，互评/汇总上下文由 Rust 显式组装。

| 调用 | CLI | 耗时 | 结果 |
|---|---|---|---|
| 第一轮 p1 | Codex | 47.3 s | done |
| 第一轮 p2 | Claude Code | 29.4 s | done |
| 第二轮 p1 | Codex | 37.1 s | done |
| 第二轮 p2 | Claude Code | 44.4 s | done |
| 主持汇总 | Codex | 44.3 s | done |

汇总按共识、分歧、风险、建议和缺席参与者组织，明确区分 p2 提出的具体超时值与双方共识。建议内容仅作为讨论产物展示，没有被执行。真实用例不涉及源码引用，源码引用链路由共享后端与新增 fixture/界面用例验证。

证据：[真实日志](samples/discuss-2026-10-27/real.txt)、[讨论状态](samples/discuss-2026-10-27/real-discussion.json)、[5 次调用记录](samples/discuss-2026-10-27/real-jobs.json)、[最终汇总](samples/discuss-2026-10-27/summary.md)。同目录保存独立回答和互评报告。CLI 运行发生在最终注册表排序修复/新建初始状态修正之前；讨论编排与提示词未随后修改，无重复模型调用。

## 失败记录与处理

- 初次完整检查：Clippy 报运行注册表类型复杂；提取类型别名解决。
- 注册表测试发现新命令在 Tauri handler 中的顺序与 canonical fixture 不同：调整为 Research 后的 Discuss，定向测试通过。第一次沙箱外复跑仍用了修正前编译的 fixture；最终完整复验单独记录。
- 默认沙箱禁止读取进程列表和本地端口绑定：合成预览与两个本地 HTTP fixture 无法运行。通过环境审批后运行沙箱外检查；没有把环境失败当作通过。
- 一次沙箱外全量测试出现两项已有 Skills Git fixture 的 `killpg EPERM`（此前 P1/P4 已记录同类偶发失败）；保留失败日志，降低同时构建负载后复跑。未修改共享 supervisor，不把历史推断作为本次根因证明。

## 与计划/现有代码的差异

1. 上位计划 §5.4 预期 `rounds/<n>/<agentId>.md`，实现 `discuss/store.rs` 用内部 `p1`–`p4` 路径键，防止路径注入并支持同 Agent 不同模型。契约与实施进度已记录。
2. 上位计划 §5.1 预期 `Permission/AgentRunSpec` 抽象；现有 `agent/mod.rs` / `agent/invocation.rs` / `agent/runner.rs` 仍导出 `AdapterInvocationRequest/AgentStage/RunSpec`。本阶段遵循已落地接口，`discuss/jobs.rs` 传 explore；只补通用进程归属和关停状态导出，未扩大重构。S1 旧功能的历史例外留 P8。
3. P7 补充了显式过时传播和逐项重新生成：计划只要求单项重试，但上游变化后不能让旧汇总冒充最新结果。保持已完成报告和旧尝试，新增调用仍分别确认。

没有触碰旧 Task 数据、冻结内核或用户技能安装。真实项目只读/原生退出与重启的桌面验收仍属于下一阶段；本轮不宣称全部产品验收完成。

最终证据：[完整检查](samples/discuss-2026-10-27/check.txt)、[桌面构建](samples/discuss-2026-10-27/build.txt)、[合成回归](samples/discuss-2026-10-27/visual.txt)、[机器摘要与哈希](samples/discuss-2026-10-27/summary.json)。最终完整门禁一次运行 143/322 全过；早先两次失败未从证据中删除。

完整门禁后仅增强了 symlink 用例的直接路径断言，定向复验 1/1 通过（[日志](samples/discuss-2026-10-27/path-final.txt)）；产品代码没有变化，未重复消耗真实模型额度。
