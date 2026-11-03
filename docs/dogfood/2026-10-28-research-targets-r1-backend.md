# Research R1/R2 后端实施记录

2026-10-28，America/Los_Angeles。**实施中，不是整体验收通知。** R0–R2 后端阶段已通过相应门禁；R3–R7 与 E0–E2 尚未完成。以下早期记录保留过程证据，最新状态见末尾“R1/R2 阶段出口”。入口为 [实施计划](../plans/2026-10-28/04:41-research-targets-and-experiments.md)。

## 基线与范围

工作区仍为 `main`，HEAD `8e56fbdfc3d101348884c091ee0aad3d876681a7` 加既有 Agent／Discuss／WebView 等未提交改动。本轮保留这些改动，未提交、重置或部署；未触碰旧 Task 数据、系统 Node 或用户全局 MCP 配置。目标创建／执行通过开发环境变量 `LOOM_RESEARCH_TARGETS=1` 开启，默认关闭；当前 UI 未接入。

已接入：

- v2/workflow；v1 无副作用读取与首次写入备份；未来 schema、无效备份、旧命令对 target 输入均拒绝。
- 跨进程 writer lease；取消绑定 run/job 与 lease token，迟到取消不影响同一 run 的新 attempt；只读实例不改活动状态。
- 预检冻结配置／目标与工具身份、5 分钟有效期、单次消费；持久化 requestId 幂等和失败记录；重复请求不重启模型。
- 不可变 snapshot/report/evidence；报告 manifest 摘要绑定；材料 ID 归属校验、内容 hash、分页补读和交换预算；只读追问固定父报告材料且采集额度为 0。
- 独立 `tool_process` 复用既有 supervisor，限制请求／帧／累计输出，处理 EOF、超时、取消、父进程与后代回收；不套用 Agent 流解析。
- 固定 REA 6.8.0 私有串行 MCP，校验协议、catalog、所用工具的 schema 摘要和 Node 支持范围；App 静态识别／JS 分析／Evidence 保存／模型报告链路。
- 14 个 target IPC、Rust/TS 契约样本与事件；脱敏导出默认不带原始 bundle，显式原始导出校验 hash，目录由 Rust 派生，导出持有同一 writer lease。
- 分层守卫扩展共享模块与 experiments，补 grouped/aliased import fixture；Learn fetch 的原例外暂保留，待 R3 真正抽取时移除。

## 真实工具与模型证据

工具沿用 [R0](2026-10-28-research-targets-r0.md) 的隔离安装：REA 6.8.0、Node 26.12.1；Claude Code 2.1.311，经用户本机已配置 `auto/gpt-6.1-sol[1m]` 路由。未把该结果记为 Codex／OpenCode／Grok 可用。

真实测试 `research::target_runtime::tests::real_app_target_report_survives_reopen` 使用受控临时 `.app`，其 JS 只做数组过滤。走生产预检仓储／admission／工具进程／REA／材料分页／`prepare_invocation → agent::run`／报告持久化，再重新打开仓储。

- 第一轮失败：模型请求一行源码的 1–200 行，旧材料窗口将 EOF 当越界，返回 `source_range_missing`。改为裁到 EOF，仍拒绝起点越界和 >200 行窗口；与共享源码读取习惯一致。
- 第二轮通过：3 条 Evidence、1 个源码快照、2 次模型调用、1 次补读、1 份源码引用报告，约 64.43 秒。
- 第三轮通过：固定 schema／Node 检查、进程组消失确认、content-addressed 报告及只读追问均进入真实链路，总约 97.66 秒。首问 4 次采集与 2 次模型调用；测试先确认源码未变，再主动移除自身创建的临时 App，追问以 1 次模型调用完成，采集 0 次，snapshot/evidence ID 与父报告完全相同。

摘要：[app-backend-real.json](samples/research-targets-2026-10-28/app-backend-real.json)；生成报告：[app-backend-report.md](samples/research-targets-2026-10-28/app-backend-report.md)。原始资料仅在 `/private/tmp/loom-target-runtime-acceptance-3/`，未把任意用户 App 资料提交到仓库。真实测试后新增的导出、轮数计数与附加执行日志由 fixture/编译验证，未为这些局部变更重复消费模型额度。

可复验命令（输出目录必须全新；临时工具存在与版本需先核实）：

```sh
LOOM_REAL_TARGET=1 \
LOOM_REAL_TARGET_ROOT=/private/tmp/loom-target-runtime-new \
LOOM_REAL_TARGET_NODE=/private/tmp/loom-research-r0-tools/node-v26.12.1-darwin-arm64/bin/node \
LOOM_REAL_TARGET_REA=/private/tmp/loom-research-r0-tools/node_modules/rea-agents/scripts/rea.mjs \
LOOM_REAL_TARGET_MODEL='auto/gpt-6.1-sol[1m]' \
cargo test --manifest-path src-tauri/Cargo.toml \
  research::target_runtime::tests::real_app_target_report_survives_reopen \
  --lib -- --ignored --exact --nocapture
```

## 局部验证

- Research Rust：29 passed，3 个 opt-in 真实模型测试在普通命令中 ignored；上述目标真实测试单独运行通过。
- 工具进程：3 passed，包含 EOF／限额／超时／取消、stderr 洪泛、正常退出后的子进程回收。最初发现 macOS 父进程退出与 SIGTERM 的竞态，已改为 reap 后重试并确认进程组消失。
- IPC 契约：5 passed，含注册列表、两端样本和 target run/event。
- Research 前端回归 + layer fixture：7 passed；TypeScript、分层守卫、fmt 和 Clippy all-targets/all-features `-D warnings` 通过。
- 共享回归阶段全库：346 passed、2 failed、32 ignored；仅两个既有 Skills HTTP fixture 因沙箱不能绑定 loopback 失败。宿主定向重跑两项均通过，不能写成单次全库全绿。其后新增／修改部分以定向验证为准。
- 未运行目标 UI／原生桌面、`pnpm check`、打包构建或 A/B 全量验收。当前无视觉改动，不以无头后端测试替代桌面证据。

## 下一步与未通过出口

继续实施，不需要用户重复授权；目标保持 active，尚不能通知产品验收。

1. R1/R2：直接 invoke、退出、profile 隔离、文件目标与部分证据均已补齐并通过阶段门禁，见末尾；MockRuntime 不代替原生桌面 UI 验收。
2. R3/R4：实现共享 repository_sources 与受管 Chromium。GitHub／网站当前仍明确 unsupported，不回退旧 Job。
3. R5–R7：目标表单、Evidence/截图/导出 UI、有限循环、三目标真实桌面与最终完整门禁。
4. E0–E2：独立实验域、固定 Node 计时/有理数判定、受检候选/最终复测与实验 UI。完整目标不变。

## 本轮核实并同步的文档差异

- [Research 契约](../architecture/research-contracts.md) 原称 `schemaVersion=1`、进程内单 writer、读取时恢复；对应 `research/models.rs`、`store.rs`、`admission.rs` 已改为新建 v2、跨进程锁、只读投影与受锁迁移。本轮更新存储段，保留旧问答业务描述。
- 同一契约的扩展状态原称“尚未注册对应产品 IPC”，[需求](../requirements.md) 原称仅 evidenceOnly 已落地；对应 `research/target_commands.rs`、`src/api/researchTargets.ts` 和 `lib.rs` 已注册后端接口。本轮改为默认关闭、UI 未接入的实施状态，未写成完整产品支持。
- [计划 §4/§9](../plans/2026-10-28/04:41-research-targets-and-experiments.md) 原将新增模块统一描述为尚不存在，R1 数据/IPC/快照仍待实施；对应新增 `tool_process/`、`rea/`、`research_sources/` 和 target 模块已存在。本轮按实际进度更新计划与索引，R1/R2 仍不标完成。


## 后续推进：磁盘、恢复与材料上下文

这部分继续推进同一完整目标，没有缩小 A/B 两条交付线。

- 新增 `storage_budget.rs` / `target_storage.rs`。每 run 的 `disk.json` 在写入前登记容量；metadata/journal 和最终报告预留独立空间。`diskBytes` 为已确认字节，`diskReservedBytes` 为尚未使用/确认的额度，两者不能混称实际文件大小。日志、上下文、源码、Evidence、报告与暂存目录都通过账本分配。
- Host 写入先核对容量；REA 工作目录及模型 cwd 在运行中采样检查，并在关闭后复查。工具超额时中止且回收进程，不声称这是 OS 文件系统配额或恶意代码沙箱；外部工具写入可能在采样间隔内短暂超额。
- 发布意图先记录暂存位置、最终位置、种类与 ID，再 rename。恢复在持有 writer 时校验完整 hash 与所属 run，并补上丢失的完成关联；不会重新读取原 App，也不会自动调用模型。未完成写入继续保留容量预留，重启不能将其清零。
- 首次研究和 initial run 在隐藏暂存容器完成后一次 rename。预检消费与 request 意图先落盘；发布后 request 回执未写完时，重复请求只返回同一已保存 run，不重新派发。Spec 写失败的 fixture 没有暴露半条研究。
- 材料 manifest、较早补读与新补读以 ≤48,000 字符上下文持久化；容量不足仅移除完整旧页并注明数量，不默默将旧材料视为不存在。context 绑定 run/scope/hash；新增 `evidence_index` 分页读取，公开材料响应使用计划中的 `nextCursor`。
- `Research`/摘要读取提供 `activeOwner`；取得已空闲的 OS lock 后清除崩溃残留 owner，避免把新恢复租约误认为旧 run 仍活跃。未来 schema、目标与 workflow 不一致、损坏根目录都显示错误，不重置存储。
- CLI 版本探针也走统一工具进程监督。先领取幂等 request/writer，再在运行的 preflight 阶段重新验证 evidenceOnly；冻结身份包含已解析可执行文件、大小、修改时间、受测版本和 Agent 配置。未验证配置仍拒绝。

新增 fixture 覆盖：容量拒绝的原子性、重启保留预留、磁盘满注入、rename 后丢事件、损坏快照、导出与删除争抢、上下文保留/逐页淘汰/篡改、旧 owner 恢复、首次创建中断、pending 请求回放、fake CLI 的 argv/stdin/环境与失败/信号/超时路径。

真实第五轮首问 + 零采集追问 + 活动 REA 取消 **2 项通过，总约 117.41 秒**，见 [结构化摘要](samples/research-targets-2026-10-28/r1-durability-real.json)。同一受控 App 的首问/追问在生产无工具调用路径中完成；取消测试先在 supervisor 看见本次工具 PID，再发绑定 run 的控制请求，确认该组退出且模型调用为 0。原子创建、根目录读取校验和 `nextCursor` 命名是此次真实测试后的局部变更，另由 fixture/契约验证；R7 仍需最终版本完整真实门禁。

最新相关检查：Research **41 passed / 4 ignored**，工具进程 **4 passed**，fake profile runner **1 passed**，IPC 契约 **5 passed**，前端定向 **7 passed**；TypeScript、分层与 Clippy 通过。最终共享 Rust 全库在宿主 **365 passed / 0 failed / 33 ignored**（28.91 秒）；此前一次 Skills Git 刷新断言失败，定向重跑与最终全库均通过，未声称已查明或修复其根因。摘要见 [检查记录](samples/research-targets-2026-10-28/r1-durability-checks.json)。

本阶段同步的文档差异：本记录原“下一步”及计划 §9 仍把磁盘账本、产物恢复、owner 和上下文列为未实现，已按上述源码/测试更新；另外材料代码原返回 `nextOffset`，与计划 §4.3 的 `nextCursor` 不同，现已统一 Rust/TS 和契约样本为 `nextCursor`，普通 Research 的日志 `nextOffset` 不变。


## R1/R2 阶段出口

本次以当前未提交工作区作为实现基线，不把源码 HEAD 单独当作证据。源码摘要见 [manifest](samples/research-targets-2026-10-28/r1-r2-source-manifest.json)，结构化结果见 [阶段验收](samples/research-targets-2026-10-28/r1-r2-acceptance.json)。

- 生产命令支持泛型 Tauri Runtime；仅测试构建注入独立数据目录/假 CLI。`ipc_tests.rs` 经真实 `generate_handler!` 和 invoke 消息进入命令，覆盖新旧分流、全部 Research 命令的主窗口约束、正向 start/followup 幂等、请求冲突、报告/材料归属、第二个 App 的只读/取消、版本探针取消，以及独立子进程中的 shutdown。共 7 个主场景和 1 个子进程 helper；这里没有启动原生 WebView。
- Research 遵循已有 `LOOM_PROFILE_DIR` 验收隔离约定；未设置时仍读原 appData/research，不迁移用户目录。普通读报告/源码入口不再为缺失 workspace 创建目录。
- 运行构造时就持有 supervisor admission，跨过 async dispatch 空档；退出拒绝新运行，已接受请求幂等回放仍可读，活动运行回收后标 interrupted。CLI 版本探针也绑定本次 research/run，并响应取消。
- 独立 JS 文件和 ASAR 先由 Rust 捕获选定源码，再让固定 REA 分析受管目录。ASAR 检查 Pickle/JSON 边界、重复键、路径、大小和 SHA-256；只读取合法声明的源码/companion，不跟随链接、不执行包或 App。快照保留原始捕获字节；未捕获资产和运行行为明确 unknown。打包/解包 fixture 由官方 `@electron/asar 4.2.0` 生成，不只用自写打包器自测。
- REA provider_timeout 后会尝试有预算的 Evidence bundle 读取，保留 partial；失败/停止保留已捕获源码并可生成宿主部分报告。真实 1ms provider timeout 取得 1 条可导出 Evidence；端到端 IPC fixture 确认错误仍为 failed、模型调用 0，部分 Evidence 和源码均可读取。
- 真实独立 JS 与官方 ASAR 两例均完成：各 5 次采集、2 次模型调用、1 次补读、1 个源码快照；引用可定位，原文件不变，执行标记未出现。新增版本归属后的真实 REA 取消也通过，模型调用为 0。
- 共享回归定位了 macOS 已 reap 父进程的进程组偶发 EPERM：Git 和工具通道现在只在有界只读探针确认组消失时接受清理，否则保留错误。增加消失/存活/不可确认 fixture。并发读取会忽略已被原子 rename 清除的临时 metadata 文件，但不会忽略权限或数据完整性错误。

最终门禁：`pnpm check` 通过，前端 **149 passed**、Rust **381 passed / 35 ignored**，fmt/Clippy/分层/release 文档检查通过；`pnpm tauri build --no-bundle` 通过，产物 `src-tauri/target/release/loom`。完整日志在 `/private/tmp/loom-research-r1-r2-final-verified-{check,build}.log`。真实 App、ASAR、部分证据与取消测试分别运行，未将 ignored 计作普通门禁通过。没有发布或安装应用，目标 UI 仍未开放。

本轮文档与代码差异已同步：早期记录将 IPC/退出与 profile 隔离列为待办，现对应 `research/{commands,target_commands,ipc_tests,target_runtime}.rs` 已实现；早期 App 代码接受文件路径却直接交给只接目录的 REA inspector，现 `research_sources` 先归一化已选源码并记录 provenance。计划与索引已更新 R1/R2 状态，R3–R7/E0–E2 继续执行。
