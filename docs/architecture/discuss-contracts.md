# Discuss 契约（P7）

实现：`src-tauri/src/discuss/`、`src/features/discuss/`、`src/api/discuss.ts`。功能层相互独立，只复用 `agent` 与共享组件/源码服务。入口在功能栏第 4 项，⌘/Ctrl+4。

## 范围与调用确认

- 输入议题（1–8,000 字符）、2–4 个内建 Agent/模型组合、可选已登记项目及 en/zh-CN 报告语言。相同 Agent 可以使用不同模型；同一 Agent/模型组合不可重复。首位参与者固定为主持人。
- 开始前显示预计 `2N+1` 次模型调用（5/7/9）；前端冻结确认时的输入，后端要求 `confirmedCalls` 与参与者数量匹配。单项重试固定确认 1 次。该字段是交互确认校验，不是不可伪造的审批凭证，也不是 CLI 逐工具授权。
- 第一轮并行独立回答 → 等待全部终态 → 第二轮并行互评修订 → 等待全部终态 → 主持汇总。最多 4 个并发进程，单次 15 分钟；沿用 runner 默认输出上限（8 MiB、20,000 行、单行 512 KiB）。
- 每次调用在 `agent::check_startable` 后经 `prepare_invocation` / `run` 启动，固定 `explore`；不续接原生会话，第二轮和汇总显式传入当次上下文。每份前轮报告最多 8,000 字符，截断明确标注；始终保留完整持久化报告。
- 后端先验证配置存在及内建适配器类型；禁用/缺失二进制/启动失败/模型失败分别记录在对应任务。一个参与者失败，其余继续。汇总只使用当前有效的第二轮回答，列出缺席者；没有有效回答时直接失败，不花费额外汇总调用。

## 状态、重试与恢复

- 讨论：`running / ready / partial / failed / cancelled / interrupted`。
- 任务：`pending / running / done / failed / cancelled / interrupted`，附 `stale`、`attempts`、错误、警告和引用统计。
- 每个讨论同一时刻只接受一个编排（全流程或一次重试）。锁保护 admission、元数据更新和删除；活动期拒绝第二次开始/重试和删除。
- 失败、停止、中断、未运行项或过时结果可单独重试，只发起该项 1 次调用；已有尝试保留。第一轮重试将全部第二轮与汇总标为过时；第二轮重试将汇总标为过时。不自动重跑下游，不把旧汇总视作最新结论。过时报告可以阅读，用户显式逐项更新。
- Stop all 设置共享取消标记，活动 runner 通过现有进程组停止逻辑终止；未开始项标 cancelled。已完成报告保留。应用退出由通用 supervisor 清理进程，下一次读取把没有活动 lease 的 running/pending 标为 interrupted，不自动调用模型。
- 进程内 lease 不跨应用实例协调；数据目录为本机应用私有目录，不支持多实例共享写入。正常退出后的记录与崩溃后的中断恢复分别测试。

## 存储

`<appData>/discuss/<discussionId>/`：

- `discussion.json`：schemaVersion=1、议题、项目、参与者、轮次任务与当前状态；原子替换。
- `rounds/1/p1.md`、`rounds/2/p1.md`：每个参与者/轮次最新成功报告；`summary.md`：最新成功汇总。
- `jobs/<jobId>/job.json`：单次调用的任务、run、时间、状态、脱敏参数、模型、工具数、退出码和错误。
- 同尝试目录下 `prompt.md`、`stdout.log`、`stderr.log`、成功时的 `result.md`。重试不覆盖旧尝试，便于追查。
- `workspace/`：无项目时的工作目录。所有输出由 Rust 写入讨论目录，不写用户项目。

参与者路径键固定为内部 `p1`–`p4`，不使用用户 Agent id 拼路径。讨论/作业 ID 受限单段字符；受管路径各组件拒绝 symlink、`..` 与分隔符。读取校验 schema、discussion ID 与固定任务结构。损坏记录在列表报错，不默默消失。

## IPC / 事件

10 个命令（只接受 main 窗口）：

- `discuss_list / discuss_create / discuss_get`
- `discuss_read_report / discuss_retry / discuss_cancel`
- `discuss_read_job_logs / discuss_read_source / discuss_list_tree / discuss_delete`

`loom://discuss-event`：`{ discussionId, taskId: string | null, status }`。创建、任务开始/结束、编排终态、删除触发；前端重新读取权威状态。文本报告按任务结束后展示，未提供逐 token 转录；打开日志时有界轮询。

日志支持 stdout/stderr/prompt/record，byte offset 每次最多 256 KiB，UTF-8 边界不截断；提供 nextOffset / hasMore。不存在或不可读日志明确报错。只能读取本讨论任务已经关联的尝试。

报告引用复用 `source_files` 校验与共享 `SourceViewer`。源码访问只允许当前仍登记的项目；项目移出列表时报告仍可读，引用变为未验证，源码读取拒绝。未选项目时仅能查看受管工作目录。外部链接不做网页真实性校验。

## 边界与计划差异

- 计划的 `rounds/<n>/<agentId>.md` 改为内部参与者键，支持同一 CLI 的不同模型并避免路径注入。
- `Permission / AgentRunSpec` 是上位计划的目标接口；P7 遵循已落地的 `AdapterInvocationRequest { chat_permission_mode: explore }` + `RunSpec`。新增 `agent` 的通用进程归属与关停状态再导出，Discuss 不直接引用 `process_supervisor`。
- 只读保障来自 CLI 策略，不来自提示词或 Loom 的独立 OS 沙箱。选项目时成功调用前后比对 Git status/HEAD，变化只警告，不能归因于单个并行参与者；失败调用不据此宣称项目不变。无 Git 项目无法用该指纹检测修改。
- 不恢复旧 Task/Planning，不执行讨论建议，不做多轮自由聊天、报告导出或自动续跑。
