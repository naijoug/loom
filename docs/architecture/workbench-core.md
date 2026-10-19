# Workbench 新内核：M1 首个切片

- 状态：已实现独立 Rust 数据/执行状态内核和本地 JSONL 入口；桌面基础桥接与前端投影已落地，见 [桌面契约](workbench-desktop.md)；旧数据导入仍待完成。
- 源码：`crates/loom-core/`；生成的 TypeScript：`packages/protocol/src/workbench.ts`。
- 宿主无关：不依赖 Tauri、SDK、旧 Task、`.ref` 或实验 crate。

## 领域与 API

Workspace 可不包含代码项目；Project 在登记时由 Rust 解析真实目录并绑定 Workspace。Session 可选择 Project，也可独立会话。Session 的 status/归档/旗标/标签与 Turn 的 queued/running/cancelling/终态独立。

`CoreCommand` 只含用户意图：Workspace/Project 创建和读取、Session 创建/列表/读取/元数据、发送、取消、事件分页、已读位置。`CoreRequest` 使用 `protocolVersion/requestId/command`，未知版本、额外字段和非法 ID 会返回结构化错误，并尽可能保留 requestId。命令字段统一 camelCase，method 使用 snake_case。

运行时输出和终态没有可反序列化的用户命令。可信 Rust 宿主通过 `claim_next_turn` 获得不能克隆/反序列化的 `TurnLease`，再调用 `apply_runtime` 或 `finish_turn`。Lease 属于精确 Core 实例、Workspace、Session、Turn 和运行代次。旧/跨实例 Lease 无法修改其他会话。

目前提供 `loom-core-host <data-directory>` 的 JSONL 本地入口，用于生产 API 的协议验证；不监听网络、不连接模型，也不是 M7 完整 CLI。调用者必须提供新的独立目录，不指向旧 `.loom/` 数据。

## 发送、排队与运行

- clientRequestId 在 Session 内唯一。发送文本和附件引用组成摘要；相同 ID/内容返回原 receipt，不再创建消息或 Turn。相同 ID/不同内容返回冲突。归档和重启后仍适用。
- 接收时一次事件保存用户消息、助手占位、Turn 和 receipt。每个 Turn 保存当时的权限与 connection/model/thinking 选择；排队期间改设置不会提升已接受回合的权限。
- 同一 Session 只有一个运行 Lease；按接收顺序领取 queued Turn。不同 Session 可以分别拥有运行权。
- 运行时帧也有 inputId 和摘要；重复帧不追加文本，内容不同则冲突。工具结果必须匹配同一 Turn 的运行中 tool-call；有未结束工具时不能标记 Completed。
- 取消 queued Turn 不会启动任何进程；取消运行中 Turn 进入 Cancelling，Lease 可读取取消状态。物理 SDK/进程停止将由 M2 runtime 执行，本切片不能证明进程已停止。
- 完成 Lease 不能再追加输出。未正常结束的 Lease 被 drop，或重开数据发现 Running 无本机 owner，会记录 Interrupted，保留已有文本/工具状态，不调用 SDK、不重跑副作用。

## 存储布局与提交顺序

```text
<data-directory>/
  manifest.json                  格式和 schema 标记
  writer.lock                    进程生命周期内的单 writer 锁
  catalog/
    events.jsonl                 Workspace/Project 事实源
    commit.json                  已提交序号下界及校验
  workspaces/<workspace-id>/sessions/<session-id>/
    events.jsonl                 Session 事实源
    commit.json                  已提交序号下界及校验
    summary.json                 可重建的轻量列表缓存
    snapshot.json                可重建的会话快照
    incomplete-tail-<hash>.bin    不完整尾记录的保留副本
```

事件包含 schemaVersion、stream、seq、timestamp、previousChecksum、payload 和 checksum。读取验证版本、作用域、连续序号、校验链和领域状态转换。

成功提交顺序为：校验领域转换 → 完整 JSON+换行写入 → flush/sync_data → 原子写并同步 commit.json → 更新内存投影 → 尝试更新派生缓存 → 发布事件/返回成功。写入或 commit watermark 更新失败会使当前 journal writer 进入结果不确定状态，拒绝继续写入；重开后按完整日志和 watermark 对账，幂等回执防止重接收。

commit.json 是提交下界而非消息内容副本：日志比已提交下界短、或该序号的 hash 不匹配时拒绝载入，不能静默丢弃已确认历史。日志比 watermark 长可视为完整但未确认的提交，校验后收敛；不会因此执行工具。

不完整的末尾行先保存并同步到 tail 文件，再截去未提交尾部。完整坏行、错误作用域、错误 seq/checksum、未知版本不能自动跳过。summary/snapshot 是缓存，丢失可从 journal 重建；缓存写失败不把已持久化的业务提交变成失败，但进入 diagnostics。当前冷加载仍完整验证该 Session 的日志，snapshot 尚不用于跳过校验。

本机验证针对 macOS 的文件系统和崩溃退出。没有把文件同步调用当成所有硬件/文件系统的断电保证，也没有把 Unix 的 O_NOFOLLOW 行为当成 Windows 已验证能力。

## 列表、并发与补流

列表读取带校验/文件版本的 summary，必要时回放单个 Session，错误会话单独返回 error。长 notes 属于 SessionView，不塞进每行列表元数据。缓存有数量上限并优先淘汰无运行 Lease 的会话；活动会话的数量仍由后续 runtime 并发策略限制。

用户元数据修改使用 expectedSeq，但只与最近用户元数据修订冲突；中间的流式事件不导致重命名失败。notes、标签、归档等仍按同一个元数据修订序号保护。已读位置只能前进，空的已取消助手占位不会生成未读提示。

`subscribe` 在同一 mutation 锁中取得 snapshot 并注册 bounded channel。慢订阅者不会阻塞磁盘提交：缓冲满时断开，消费者必须通过 snapshot/read_events 续接。read_events 使用已校验的字节位置索引、条数/字节预算和 seq 游标；不会读取全部日志只为补一个活动页面。

## Rust/TypeScript 契约

`bindings` 功能使用固定 ts-rs 12.0.1 从真实 Rust/serde 类型生成声明，不加入默认运行时依赖。大整数映射 number 的前提是核心序号、时间戳、policy version 限制在 JavaScript safe integer 范围；通用工具 JSON 的语义仍由适配器校验。

```sh
cargo run -p loom-core --features bindings --bin export-workbench-types
pnpm check:core
```

检查包含 Rust fmt、严格 Clippy、实际文件/进程/协议测试、生成声明漂移和 TypeScript 编译。它已接入 `pnpm check`，旧 `src-tauri` 保持独立 workspace 和原检查入口。

## 旧数据只读预检

`preview_legacy_import` 通过 Workspace/Project ID 解析登记目录，不能由 UI 传任意读取路径。`legacy` 模块不依赖旧 Chat/Task 仓储，支持项目内 Chat v1 JSON、v2 journal 以及 Task v1 裸对象/版本封套。

- v2 同 ID 优先；损坏或无法枚举 v2 不回退 v1。回放检查版本、seq、项目/会话归属、消息 ID、收据和回合不可变字段。残尾只报告，快照只用于检测丢失历史和提示不一致，均不修复源文件。
- Unix 下从根目录句柄逐层 `openat`，禁止符号链接；仅只读普通文件，拒绝 FIFO/设备/路径越界。项目根被改成符号链接也拒绝。单文件 32 MiB、每次读取 128 MiB、目录项总计 10,000 的预算超限会明确报告，不默默省略。
- 返回逐项错误/警告、消息数、来源文件 SHA-256、缺失资源及整个预检指纹，不返回旧 invocation、凭据或 resume 数据。fingerprint 是当前观察结果标识，尚不是持久化导入授权，也不保证扫描期间多个文件的事务快照。
- 工作台选中项目后可打开预检报告，支持重试、缺失资源和损坏项展开。只有执行“导入可读取的记录”后才创建新 Session。
- 目前在 macOS 实测；Linux 使用同类 Unix 描述符路径但尚未运行，Windows 明确返回不支持，需补独立安全读取实现。独立导出包还未作为源格式接入。

## 实际导入与历史副本

`prepare_legacy_import` 再次扫描并比较预检指纹，保存有校验和的计划；`execute_legacy_import` 捕获不可变来源字节，按计划验证，再写入私有 staging。发布前重新扫描确认来源未变。单独的导入互斥锁限制并行导入，正常会话操作不被整个扫描过程占用领域锁。

每个来源项目/旧类型/旧 ID 映射稳定 Session ID。会话 journal、历史转录和资源先同步到 staging，以单次目录 rename 发布，再同步父目录、刷新缓存和通知工作区。发布后的目录是完成依据；报告逐项原子保存并校验。中断后重试验证已有目录，返回 `already_imported`，只补齐剩余项；旧内容修订返回冲突，不能覆盖已有会话里的新笔记、消息或配置。未发布 staging 不出现在列表，下次执行会清理这次计划的暂存副本。

`HistoryImported` 只记录来源与 manifest 摘要，不创建运行 Turn/SendReceipt。`SessionView.importedHistory` 引用独立不可变历史，Chat 消息按需分页读取，未结束消息标为 interrupted。新会话使用 Explore、空连接和空 Sources；旧 native resume、PID、审批与 invocation 仅可作为原始历史资料存在，不能进入执行配置。Task 默认归档，完整 JSON 保留需求、事件、Review 等内容，没有虚构新 DAG。

资源包括预检成功读取的引用文件、来源文件和回放后的完整记录，使用受校验的资源 ID 访问，不能传路径读取任意文件。二进制按原字节复制，Unix 私有目录/文件权限为 0700/0600。读取校验 manifest、转录和资源 SHA-256；源目录离线也可访问副本。UI 提供历史消息、工具摘要和文本资源预览（单次 64 KiB，界面最多预览前 1 MiB），二进制富查看器仍待 M5。完整记录和附件不因预览上限被截断。

当前读取预算仍适用；生成的历史转录上限 64 MiB，单个重建记录/资源 32 MiB，计划/报告 8 MiB，超限报错。还没有导入修订合并、独立导出包源格式、Windows 导入器和导入报告列表页；报告可通过计划 ID 查询，重复预检相同来源会找到同一计划。M2 真实模型接入时须显式读取历史转录作为上下文，不得用旧 resume 句柄替代。

## 持久化工具审批

`request_tool_approval` 只接受当前 Rust `TurnLease`，原始参数仅参与摘要与绑定计算。审批 journal 保存脱敏预览、参数摘要、会话/回合/工具/request ID、运行绑定摘要、工作目录、模式、策略版本和期限。前端只有 `respond_tool_approval` 用户意图，不能创建请求或领取执行凭证。

状态为 pending → allowed/denied/expired/cancelled；allowed 必须由相同租约、工具及原始参数调用 `consume_tool_approval` 后才能取得不可序列化、不可克隆的 `ToolPermit`。消费事件与工具 Running 投影是同一条持久化事件。重复消费失败；相同请求重试不延长期限；已登记审批的工具不能通过普通 ToolStarted 事件绕过。工作目录、连接和策略版本来自不可变运行绑定。

用户决定先持久化再返回；写盘失败不返回授权。取消与结束事件原子撤销 pending/allowed；崩溃恢复终结旧回合时也撤销旧批准。到期在会话读取、决定或消费时持久化 expired，宿主应按期限唤醒，不能依赖用户继续打开页面。已消费工具按正常取消/工具终态规则收尾，取消不能回滚已发生的副作用。

每个会话最多 32 个未决审批，参数上限 1 MiB、预览上限 64 KiB、TTL 上限 5 分钟。预览隐藏正文、headers/env 和凭据字段，并识别常见密钥值；这不是任意内容的完整秘密识别器，后续 vault/worker 日志仍需按已知凭据脱敏。普通 `tasks/task-list.md` 不会因包含 `sk` 被误隐藏。

当前实现覆盖单次交互审批。自动只读/Execute 策略、Guarded 风险服务、记住授权、动态 Source 策略修订、完整工具分类和生产 SDK 宿主仍待接入；不能将此 broker 当作完整权限策略或 OS sandbox。

## 尚未交付

- Tauri commands、事件泵、前端投影和新 Shell 已完成基础接线；完整 Craft UI/交互仍继续实施。
- 旧 Chat/Task 项目格式的预检、导入、引用资源复制和重复/中断恢复已接入；独立导出包、跨平台、完整发布回退和大型历史优化仍待完成。
- 持久化单次工具审批已接入；生产 SDK adapter、真实身份/模型、完整 policy、凭据脱敏、进程 supervisor 与审计接线仍待完成。
- Source/Skill/Page/Task/Automation 等服务和完整产品交互。
- 当前发送会拒绝非空 attachmentIds，避免假装已接入附件存储；后续由资源仓库验证后开放。

## 模型连接目录与运行快照

模型连接保存在全局 catalog，含提供方、认证类别、端点、模型列表/默认模型、能力提示、消息中途行为和不透明凭据引用。当前配置类型覆盖 Claude、Pi 内置提供方与 Pi 兼容端点，以及 API key、bearer 和无认证；支持配置保存不代表 SDK/认证流程已经验证。

每次保存携带 operationId 与 expectedRevision。相同已完成操作可重试，旧修订不能覆盖新配置。`save_connection_metadata` 是可信宿主方法，不属于前端 CoreCommand；前端只可读取目录，含秘密的保存走专门 Tauri command，先完成 vault 保存再发布引用。

发送时将选中连接复制到 `TurnConfiguration.resolvedConnection`，同时冻结默认模型。运行时须提供相同快照 digest 和模型，不能在排队期间悄悄改用新端点、模型或凭据引用。第一次接收消息后锁定会话的连接身份，后续仍可选择该连接中的其他模型；导入的纯历史不占用这个锁。

新增的可选快照字段在 None 时不序列化，保持旧 journal 的 checksum 可回放；配置/绑定仍由 Rust 生成 TS 类型。标准 URL 解析要求 HTTPS 或 loopback HTTP，拒绝嵌入用户名/密码、query 和 fragment；需要 query 等配置的认证机制尚待专门接入。

这些是后续计划工作，当前内核测试通过不能抵扣相应功能验收。

Workspace 级订阅现已支持轻量列表失效通知，Session/Workspace 订阅均通过 Drop 清理。运行时帧 receipt 是 Rust 内部状态，不再发送给前端；日志仍保存可重建它们的 RuntimeApplied 事件。
