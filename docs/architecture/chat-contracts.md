# Chat 当前契约与重构目标

- 核对日期：2026-09-22；产品范围见 [需求](../requirements.md)，当前修复见 [进展 Review 修复计划](../plans/2026-09-22/14:29-chat-progress-review-fixes.md)，整体验收见 [重构计划](../plans/2026-09-17/10:15-local-agent-chat-rebuild.md)。
- 前端：`src/domain/chat.ts`、`src/api/chatClient.ts`、`src/features/chat/useChatBridge.ts`、`state/ChatProjection.ts`。
- 后端：`src-tauri/src/chat/{mod,models,repository,journal,service,runtime}.rs`。
- 当前持久化为 **schema v2**；v1 只读迁移。浏览器 Mock 仍可使用 v1 外形，不代表真实桌面或模型验收。

## 持久化与恢复

```text
<project>/.loom/chat/
  writer.lock
  index.json                     # 旧 v1 文件，迁移不改写
  sessions/<id>.json             # 旧 v1 文件，迁移不改写
  exports/<exportId>/            # 独立 JSON/Markdown/manifest 与日志副本
  v2/
    index.json                   # 可重建的 id 缓存
    sessions/<id>/
      events.jsonl               # 唯一权威日志，完整换行提交
      snapshot.json              # 可重建投影缓存
      migration.json             # v1 来源的迁移记录（如适用）
      runs/<turnId>/stdout.log    # 有界、脱敏、分流的执行输出
      runs/<turnId>/stderr.log
```

- 会话绑定已登记项目的 canonical path，作为 `projectKey`；id 仅允许有界单段字符。拒绝静态 symlink 目录/文件，校验磁盘中的 project/id/schema。
- 项目内 mutation 串行执行，跨进程使用 OS writer 锁；锁文件存在不等于被占用，不能删除锁文件绕过占用。
- `events.jsonl` 以完整 JSON + 换行为提交边界，写入并 flush 后才发布事件。单会话 seq 跨回合递增且不超过 JavaScript 安全整数范围；`lastSeq` 和 `revision` 表示已应用的最后事件。
- 创建写一次初始快照记录；元数据/回合开始结束写字段与消息 patch；流式正文/工具片段仅追加增量，避免每个 token 复制整个转录。
- `snapshot.json` 是派生缓存。流式期间可以落后，加载时按日志重建；缓存写入失败不会撤销已经提交的日志。权威日志写入失败则停止回合并报告错误，不伪装成功。
- 只截断不带结尾换行的未提交尾部；完整坏行、序号缺口、未来版本或错项目记录都拒绝读取，不跳过后继续。缓存序号领先日志时拒绝回退历史。
- 迁移扫描 v1/v2 会话并去重，只读解析 v1 后写 v2。空目录或首次追加中断可以重试；已提交创建记录标明 v2 权威来源。旧文件逐字保留，不移动 Task、Review、反馈或验证证据。
- 已有快照却没有权威日志时不自动用旧 v1 覆盖，避免把丢失的已提交历史当成未迁移。未知未来 schema 保持原样并报错。
- 读取活动回合不会当作重启恢复；只有没有本进程活动归属的遗留 streaming 会标记 aborted/interrupted_by_restart，保留已提交 partial，再写恢复事件。恢复幂等，不能自动重跑模型或命令。
- 损坏会话在列表中显示 `storageError` 和“无法读取的会话”，健康会话继续可用；不能把坏数据静默隐藏成空列表。
- 当前恢复保证针对进程退出，**没有硬断电 fsync 保证**。静态路径检查也不等于内核级抗并发目录替换防护。

日志单条记录上限 32 MiB；补流每页 1–500 条，通常不超过 2 MiB。为保证游标前进，首条大记录可单独返回（仍受记录上限约束）。活动回合首次补流完整校验并建立内存字节索引，后续按游标定位记录；空补流不解析历史。索引只在本进程活动 lease 内使用，提交流式记录后更新；元数据更新使其失效，日志/快照被替换、截断或修改时重新完整校验。磁盘格式不变，重启、非活动会话和列表读取仍完整重放；长历史冷启动与日志保留策略仍需后续专项验证。

## 会话与消息模型

`ChatSession` 保留 id/projectPath/agentId/title/permissionMode/messages、时间、可选 resumeCommand/activeTurnId/promotedTaskId、active/archived、flagged，新增 `lastSeq`、`revision`、`resumeHandle` 和 `schemaVersion: 2`。

- ChatSession 不要求 taskId，不自动推进 Task。升格只复制转录/需求创建 Task 草稿。
- 会话运行兼容状态仍为 idle/streaming/error；消息为 complete/streaming/aborted/error。新增 `turns` 保存独立回合状态和执行证据；旧会话缺省为空，不反推或伪造旧运行记录。
- message.parts 支持 text/tool/error；tool 的 inputSummary/outputSummary 输出 camelCase，并兼容读取旧 snake_case。
- 换 Agent/权限、清 resume、归档和升格在后端拒绝活动回合操作；运行时可以改标题/旗标，结束保存不能覆盖这些新元数据。
- 已归档会话需要恢复后才能发送。元数据操作不应清空转录；改 Agent/有效权限会清除不相容的续聊句柄。

`sendReceipts` 是追加且不可修改的接收回执，包含 clientRequestId/turnId/userMessageId/assistantMessageId/requestedPermissionMode；与两条消息在同一个 journal patch 提交。旧 v1/v2 会话缺少时读为空。请求标识在 canonical 项目与会话内唯一，限 1–160 个字母、数字、连字符或下划线。

后端在同一写锁内先查回执：重复标识且正文、请求权限完全一致时返回原 turnId 和最新会话，不再次准备/启动 Agent；不同内容或权限报冲突。已完成、停止、中断、归档或配置改变后依然适用。未接收的新请求才检查活动回合/归档限制。重启恢复只标记中断，不重跑副作用。

`ChatTurn` 与用户/助手消息、clientRequestId 关联，接受时原子保存 `starting` 和不可修改的 invocation（Agent、adapter、程序/argv、cwd、权限、输出协议、stdin、配置指纹）。正文参数以 `[PROMPT]` 替代；凭据选项和已知敏感行脱敏。随后记录真实启动时间/PID，进入 running；停止请求记录 cancelling，运行结果分别为 completed/failed/cancelled/timed_out。无本进程归属的未完成回合恢复为 interrupted。终态包含结束时间、可用的退出码、原因/错误；没有得到退出码时保留 null，不猜测为 0。

回合配置、消息关联和接收时间不可改写；终态只提交一次。PID 只用于诊断，重启不按历史 PID 盲杀。停止先设置活动 lease 的取消标记，即使保存 cancelling 失败也不阻止 runtime 清理子进程。

## IPC

| Command | Input | Output |
|---|---|---|
| chat_list_sessions | `{ projectPath }` | summary[]，含可选 storageError |
| chat_create | `{ input: { projectPath, agentId, title?, permissionMode? } }` | ChatSession |
| chat_get | `{ projectPath, sessionId }` | 已重放/恢复的 ChatSession |
| chat_read_events | `{ projectPath, sessionId, afterSeq, limit? }` | `{ events, hasMore, lastSeq }` |
| chat_read_run_logs | `{ projectPath, sessionId, turnId, stream: "stdout" \| "stderr", offset, limit? }` | `{ text, nextOffset, hasMore }` |
| chat_export | `{ input: { projectPath, sessionId } }` | `{ directory, jsonPath, markdownPath, snapshotSeq, inProgress, warnings }` |
| chat_send | `{ input: { projectPath, sessionId, clientRequestId, text, permissionMode? } }` | `{ turnId, session }` |
| chat_abort | `{ input: { projectPath, sessionId, turnId } }` | null |
| chat_set_agent | `{ projectPath, sessionId, agentId }` | ChatSession |
| chat_update_meta | `{ input: { projectPath, sessionId, title?, permissionMode?, status?, flagged?, titleFromFirstMessage? } }` | ChatSession |
| chat_clear_resume | `{ projectPath, sessionId }` | ChatSession |
| chat_promote_to_task | `{ input: { projectPath, sessionId } }` | `{ taskId, task, session }` |

前端均通过 `chatClient.ts`。Rust/TS 共同验证 `contracts/tauri-contract.json` 的命令、事件、schema 与会话/事件样本；生产 wrapper 的实际调用参数有独立回归。

## 事件与前端投影

默认 Chat 使用 `ChatShell`，复用项目导航并保留高级工作流入口，不挂载 Task Header、任务列表加载或 Planning 订阅。`useAgentCatalog` 只提供 Agent 配置/诊断；高级工作流 `WorkflowShell` 挂载时读取 Tasks，并通过 `usePlanningEvents` 订阅一次状态与日志，离开时清理，返回时重新读取持久化任务。`useAgentBridge` 保留高级工作流操作，不隐式添加订阅。

正常事件统一为 `loom://chat-event`：

```text
{ schemaVersion: 2, projectKey, sessionId, seq, timestampMs, kind, payload }
```

| kind | payload |
|---|---|
| session_created | 初始 ChatSession（seq=1） |
| session_patch | `{ fields, changedMessages, appendedMessages }` |
| stream | `{ sessionId, turnId, messageId, delta, done, part? }` |

Patch 只允许指定的元数据字段，不允许更改身份/路径/schema，不能删除或重排已有消息。恢复、开始与结束状态以 patch 表达；完整 turn 状态语义仍按计划推进。

`loom://chat-error` 仅用于无法写入权威日志的非持久化故障通知，包含 projectKey/sessionId/turnId/messageId/status/errorSummary。旧 `chat-stream` 和 `chat-turn-finished` 已由生产端与消费端同轮替换，不再双订阅。

`useChatBridge` 和 `ChatProjection` 已用于实际 ChatPage：

- 建好监听再开放发送；加载快照时缓冲事件，按 seq 去重、排序与补缺。
- 快照/发送响应的旧游标不能覆盖新输出；跨项目/会话和旧请求结果不得污染当前视图。
- 回合运行中定期补流，窗口回到前台时同步，补回漏掉的结束事件；分页有数量与字节预算。
- 生成状态来自当前会话快照及其待提交请求；删除页面全局“结束事件清 sending”和旧 UI 超时杀进程路径。
- 草稿按项目和会话隔离；发送成功只清除该次已提交的文本，不覆盖其他会话或后来输入的草稿。
- `ChatSendController` 合并同内容的待提交请求；IPC 失败后保留请求标识，手动重发同内容/权限复用标识。明确收到回执后下一次提交分配新标识；不自动重跑 Agent。标识缓存跨 Chat 页面挂载保留，但不跨 renderer 重载持久化；重载后已提交转录从后端恢复，手动新写消息属于新请求。
- Shift+Tab 保留反向焦点导航，不修改权限；消息发送与标题保存均防止把 IME 确认回车当作提交。
- 已记录的助手回合显示状态、耗时和“执行记录”；展开查看固定配置、时间/退出码及 stdout/stderr。日志分页、重载和错误可见；切换流/会话/项目时忽略迟到响应。内容作为纯文本展示，不执行 HTML。

## 导出与回滚

`chat_export` 在后台文件线程从已提交会话快照生成独立目录，含 `session.json`、`transcript.md`、`manifest.json` 与可用运行日志；不接受调用者提供任意目标路径，不覆盖旧导出。staging 全部写入并 flush 后才 rename 发布；正常失败清理 staging，进程中断留下的 `.partial-*` 不算成功产物。

快照固定在 `snapshotSeq`，各日志单独捕获完整换行前缀并记录来源字节界限、忽略尾部长度与捕获时间；运行中标为 inProgress，不声称所有文件属于同一瞬间或最终状态。源日志缺失/损坏/symlink 在 manifest 和 UI 明示，目标写入失败则整个导出报错。每份源日志上限 8 MiB；拷贝时不占用会话写锁。

JSON 保留完整转录和结构化证据，Markdown 用动态围栏保留字面内容，避免原文闭合代码块或执行 HTML。导出只在本机生成，不自动打开/上传；用户显式点击才定位目录。正文并非全面秘密检测后的内容。旧版不支持导入 v2 JSON；手动备份/回退/返回新版的限制见 [回滚指南](../guides/chat-export-rollback.md)。

## Runtime 与上下文

生产链路为 command → context/adapter → runtime → journal → UI 投影。`ProcessOwner::Chat` 使用 project/session/turn 独立归属，Task 构造与查询保持兼容。

`service::run_chat_turn` 是生产 runtime 与回合/日志仓储的接线边界：spawn 前创建独立日志，启动后提交 running，再将每条 stdout/stderr 脱敏写入相应文件。日志创建/写入失败使回合失败并停止执行，不能仅在 UI 隐藏错误。单回合两份日志总预算 8 MiB；写入 flush 不等于硬断电保障。

日志读取只接受已登记项目中的已关联回合，不接受任意文件路径；仓储派生并校验目录/文件，拒绝 symlink。字节游标分页默认 32 KiB，上限 64 KiB，UTF-8 字符不跨页截断；越界/字符中间游标报错，空日志与读取失败不同。旧会话未记录的日志不伪造。当前没有自动删除历史日志，长时间使用的保留策略仍待定义。

- stdout/stderr 并发排流，stdin 并发写入并关闭。默认回合预算 10 分钟；停止先 TERM，1 秒后升级 KILL，再有界回收。父进程退出但子进程持管道、关管道但仍存活等路径有实际进程回归。
- 每回合输出预算 8 MiB、单行 512 KiB、20,000 行和 1,000 工具片段；超限明确失败。非零退出保留 partial 但标 error，不更新 resume。
- 正常或异常收口清理同进程组；正常应用退出已接 Tauri 关闭/退出和 Unix TERM/INT：先关闭新操作入口、取消活动回合，TERM 后升级 KILL，等待操作结束与持久化；失败保留窗口并可重试。真实 CLI 停止与原生桌面运行中退出/强制退出仍需独立验收，不能以本地进程测试代替。
- `prepare_chat_invocation` 以 adapter 的 resumed 为准：原生续聊只传新增输入；fallback 历史最多 12 条、单条 2000 字符、序列化 12000 字符并标截断。最新请求保留，超过 32000 字符先报错，不裁剪原始转录。
- Codex/Claude 请求已有 stdin 能力，只有 adapter 返回 stdin_prompt 才写入；Grok/自定义 positional 模板使用 argv。Grok 1.0.30 使用 `streaming-messages-json`。

`resumeHandle = { version: 1, adapterType, nativeSessionId, configFingerprint }` 由成功回合的 prepared 配置和解析出的原生 ID 生成。SHA-256 绑定 Agent id/type、程序、参数、工作目录策略、能力、读写/命令开关、项目路径、Chat 权限和当前兼容 stage；不保存指纹原文。标题/显示名称变化不影响它。指纹不检测 CLI 外部设置/登录身份或磁盘二进制替换。

续聊仅使用有效且配置匹配的结构化句柄；`resumeCommand` 作为诊断兼容字段，不作为 Chat 可执行参数。旧记录只有字符串时提示“续聊需重建”，配置变化/无效句柄也拒绝发送，要求用户“开新 CLI 会话”；清除后有预算地重放原有历史。不会在恢复失败时偷偷重跑新调用。

adapter 的旧字符串入口（高级 Task 仍使用）仅解析确切程序前缀、已知 resume 操作、单个 1–160 字符的字母数字/连字符/下划线 ID，首字符须为字母数字。额外参数、跨程序、选项式 ID 和相对路径均拒绝；含空格的程序路径按整个前缀匹配。Codex/Claude 的正文与参数用 stdin 或 `--` 分开，Grok 前导连字符正文使用附着的 `--single=...` 值。

## 权限与完成边界

Chat 使用独立的 `chat_permission_mode`，不再要求 Agent 具备 Task planning/debugging capability；启用/可用性、可写和命令开关仍由后端校验。Codex 每轮包括 resume 都明确 sandbox/cwd 和 `approval_policy="never"`；Claude/Grok Chat 每轮都明确 plan 或 acceptEdits。兼容 stage 仅用于旧模板和指纹，Task 自身的规划协议保持不变。Ask 是 Composer 的**每回合授权**，不是逐工具审批，后端也尚未校验独立授权凭证。具体 CLI 限制仍按真实证据判断；不得把 Task command runner 的 execution_policy 当成 Chat 工具的保障。

Grok 的真实无工具文本/原生续聊已有 canary；本轮隔离 QA App 验证了真实发送、后台会话隔离、正常重启续聊、重命名、归档/恢复和搜索，见 [v2 验收记录](../dogfood/2026-09-20-chat-v2.md)。这不代表 Release 或所有 CLI 完成。

仍须按计划完成：长历史冷启动与日志保留策略、完整权限/工具/停止和应用退出矩阵、Chat 主界面剩余交互验收，以及同一发布候选的签名/公证/安装及实际桌面回退门禁。回合/日志证据见 [执行记录验收](../dogfood/2026-09-20-chat-turn-logs.md)，导出与离线回滚样本见 [导出验收](../dogfood/2026-09-20-chat-export.md)。历史证据只代表当时切片，不用其中“下一步”清单覆盖本契约现状。
