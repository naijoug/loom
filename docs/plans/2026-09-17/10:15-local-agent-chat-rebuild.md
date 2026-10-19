# Loom 本机 Agent Chat 重构实施计划

- 日期：2026-09-17 10:15（Asia/Shanghai）
- 2026-09-20 约束修订：产品范围以 `docs/requirements.md` 为准；当前 Ask 保留回合授权，后续逐工具审批独立验收。现状见 `docs/architecture/chat-contracts.md`；本计划中的原始代码审查记录保留历史基线。
- 状态：implementing；v2 journal/迁移/补流、发送幂等、完整回合记录、独立日志和导出已接入。正常退出处理已有代码和本地进程回归；真实权限/工具/停止、原生桌面运行中退出与最终发布门禁仍待完成，构建名称保持 Loom。
- 当前切片基线：Loom `0fe5f79` 加工作区；当前修复入口为 [2026-09-22 进展 Review 修复](../2026-09-22/14:29-chat-progress-review-fixes.md)，现状以该计划与 Chat 契约为准。第 12 节与第 13–20 节按时间保留历史证据，旧“下一步”清单不表示当前仍未实现。
- Review 结论：原稿 `needs-rework`；已修正前置门禁、测试覆盖、任务顺序与交付范围，详见第 12 节。文档修订不表示代码缺陷已修复。
- 参考基线：Craft Agents OSS `e8963854c3679edcceb105a42537a06749e6cb64`。
- 替代关系：本计划接替 [09:03 Craft Phase 1 计划](./09:03-craft-inspired-local-agent-chat.md) 的未完成工作；保留其 M0/M1 历史和已有实现，不继续叠加旧 M2–M5。更早的 [chat-first 计划](../2026-09-14/20:06-chat-first-refactor.md) 作为背景。

## 1. 目标与范围

把 Loom 的主产品重构为可以日常使用的本机 Agent 聊天客户端：打开本地项目，选择已安装的 Agent，发送消息、查看流式回答和工具执行、停止生成、连续追问，退出重开后可以继续会话。UI 参考 Craft 的会话导航和聊天信息层级。

“本机调用”指 Loom 在本机项目目录中启动本机 Agent CLI、复用其登录和配置；Agent 自身仍可能请求远端模型，不等同于离线推理。保持 Tauri + React + Rust 技术栈。

本次用户要求更新了交付顺序：**先交付本机 Chat，再考虑多 Agent 编排**。`docs/requirements.md` 的四阶段开发闭环仍是后续产品方向，本阶段验收以本计划为准；实施 M0 时同步写入需求文档，避免并存两份互相冲突的 MVP 定义。

### “彻底重构”的落点

- 重构默认 Shell、Chat 状态管理、Chat runtime、消息事件、会话存储与续聊边界；不只修改 CSS 或继续扩张现有 `ChatPage.tsx` / `chat.rs`。
- Chat 不依赖 Task 创建、阶段切换、Review 或调试流程；移除 Chat 主路径中的任务加载和工作流桥接。
- 提取可复用的 adapter、进程监督、项目选择、Agent 配置和诊断能力。旧工作流隔离为“高级工作流”，保留数据读取和原有能力；不要求重写所有经过验证的基础设施。
- 完成替换后删除旧 Chat 的重复启动、解析、监听和存储路径。演示数据只能通过显式预览入口启用，桌面故障不得回退模拟成功。

### 非目标

- 本阶段不实现多 Agent 讨论、自动 Review、修复循环的新编排，不重新设计 Task 状态机。
- 不搬入 Electron/Bun 服务端、Pi SDK、云账号、远程连接、Sources/MCP 管理、插件市场、自动化、浏览器操作。
- 不做完整 IDE、终端模拟器、文件树、多文件 Diff、附件转换或 AI 自动命名；先支持文本、Markdown/代码块、工具与命令记录。
- 不承诺所有 CLI、所有操作系统同时可用。首轮验收平台为当前 macOS；其他平台和未接入工具显示真实支持状态。
- OpenClaw/Hermes 保留适配器扩展位置，未验收前不作为已支持 Agent 展示。

## 2. 成功标准

1. 首次启动能完成“选择项目 → 检测本机 Agent → 新建会话 → 发消息”；缺失 CLI、未登录、版本不兼容分别给出可执行的下一步。
2. 至少一个真实本机 Agent 完整通过：流式回答、工具/命令可见、连续三轮追问、停止后再发、正常退出与重启后续聊。Mock/fixture 不替代真实调用证据。
3. UI 显示的权限与 adapter 的实际执行限制一致；新建和 resume 均验证限制，不支持的权限不伪装可用。
4. 同一会话只能有一个活动回合；切换会话/项目不串消息；旧回合结束事件不能覆盖新回合，重复提交不产生重复调用。
5. 停止在正常情况下 3 秒内结束该回合进程组；强制停止超时上限 5 秒并回收进程。非零退出、超时、用户取消、应用中断为不同状态。
6. 已提交的消息和已确认持久化的输出在重启后可读；异常退出后流式回合转为 interrupted，可由用户重试，不能永远停在 streaming。
7. 会话可重命名、归档/恢复、按标题搜索；命令、cwd、开始/结束时间、退出码和脱敏日志可查看。正常聊天无须进入高级工作流。
8. 前端/Rust/桌面构建检查通过，真实 Tauri 验收留存证据，旧项目、任务与设置可读取。只完成网页预览不得宣称本机聊天已完成。

## 3. 当前状态与真实缺口

下表保留初始审查记录，行号固定对应 `3127ca8`；当前代码按符号定位。第 12 节提供本次 `b8e35c7` 工作区的最新 Review 和验证证据。本轮没有执行真实 Agent 对话或桌面验收。

**提交前增量核对：** `b8e35c7` 已拆出 `ChatInbox.tsx`、`ChatTranscript.tsx`、`ChatComposer.tsx`、`ChatSessionHeader.tsx` 和 `chatPermission.ts`，新增 `chat_update_meta` 及 active/archived UI。因此 M3 复用这些组件，元数据 IPC 沿用 `chat_update_meta`；不重复实现旧 M1。页面内仍持有 API/事件监听和 sending，Rust runtime/存储缺口仍需处理。

**12:06 Review 补充：** 上述组件和后端元数据函数存在，但五个 Chat IPC 调用的参数信封错误，不能认定桌面端功能可用。当前只有归档入口，没有恢复、重命名、搜索 UI；parts 展示也没有 runtime 事件接入。`storage::atomic_write_json` 已存在，应复用；三项 Chat 前端测试复制了实现，不能作为生产功能验证。M0.0 先修 IPC 和测试基线，状态隔离提前至 M1.4。

| 位置（当前基线） | 已有能力 | 本次处理 |
|---|---|---|
| `src/App.tsx:30`、`src/features/chat/ChatPage.tsx:31` | 默认 Chat，仍挂在旧 AppState/Shell 下；初始页面混合 API、监听和视图，最新提交已拆视图组件 | 复用最新组件，新增 Chat Shell 与独立 store/bridge；旧工作流按路由挂载 |
| `src-tauri/src/chat.rs:100`、`:201` | ChatSession、项目级 `.loom/chat`、新建/列表/发送/停止/升格 | 保留领域隔离，拆成 models/repository/service/runtime/commands |
| `src-tauri/src/chat.rs:250`、`:544` | 整文件直接写入；发送前写占位消息，生成结束才写最终正文 | 原子快照、回合日志、单写入者、崩溃恢复与增量持久化 |
| `src-tauri/src/chat.rs:530` | registry 只按 sessionId 保存 PID；abort 忽略 projectPath 和 turnId，只调用 kill PID | 用项目/会话/回合精确定位，支持启动中取消及进程组停止 |
| `src-tauri/src/chat.rs:701` | 自建进程，stdout 读完、wait 后才读 stderr；无运行超时 | 并发排空两路输出，避免 stderr 管道堵塞，统一监督与清理 |
| `src-tauri/src/chat.rs:289`、`:775` | 通用字段猜测解析；有正文时非零退出可能被当作成功 | adapter 专属事件解析，保留部分输出但明确失败 |
| `src-tauri/src/chat.rs:412`、`:266` | 改 Agent 不清 resume；每次含历史 prompt，即便原生续聊 | 结构化续聊句柄、切换隔离；原生续聊只发送新增输入 |
| `src-tauri/src/agent_adapter.rs:54`、`:148`、`:209` | Codex/Claude/Grok/custom adapter；权限借用 AgentStage | 增加独立 Chat 请求/权限策略；核验 resume 与初始调用的限制一致性 |
| `src-tauri/src/agents/stream.rs:116` | 已有 CLI parser，但主要依赖 planning 上下文，且可见性为 `pub(super)` | 提取通用 parser/事件，保留旧 planning 包装与回归测试 |
| `src-tauri/src/process_supervisor.rs:29` | 有进程组管理；metadata 强制 taskId | 增加明确的 Task/Chat owner；保留 task 查询兼容层 |
| `src/domain/chat.ts`、`src/api/contract.ts`、`src-tauri/src/contracts.rs:199` | Chat 类型/IPC 名称已有；契约测试硬编码读取 `chat.rs` | 拆模块时同步契约 fixture、源文件扫描和前端测试编译范围 |

旧计划提出“原子写”和“统一 stream 解析”，但当前 Chat 路径未实现；不能据文档直接认定已具备。初始基线的 Ask 映射 Planning；2026-09-20 当前实现已为 Debugging + UI 回合授权，仍无逐工具审批；Claude 只读分支未显式指定只读策略，需 M0 实测后再标记能力。

## 4. Craft 参考与取舍

参考代码已只读获取；以下链接固定 commit，避免后续上游变化造成计划漂移。

| 参考来源 | 学习内容 | Loom 决策 |
|---|---|---|
| [README](https://github.com/craft-ai-agents/craft-agents-oss/blob/e8963854c3679edcceb105a42537a06749e6cb64/README.md) | 会话收件箱、持久化、流式工具展示 | Chat 成为主入口；先 active/archived，不复制完整工作流状态 |
| [AppShell](https://github.com/craft-ai-agents/craft-agents-oss/blob/e8963854c3679edcceb105a42537a06749e6cb64/apps/electron/src/renderer/components/app-shell/AppShell.tsx)、[SessionList](https://github.com/craft-ai-agents/craft-agents-oss/blob/e8963854c3679edcceb105a42537a06749e6cb64/apps/electron/src/renderer/components/app-shell/SessionList.tsx) | 导航、会话列表、内容区分层 | 项目导航 + 会话列表 + 聊天；减少旧阶段条干扰 |
| [ChatDisplay](https://github.com/craft-ai-agents/craft-agents-oss/blob/e8963854c3679edcceb105a42537a06749e6cb64/apps/electron/src/renderer/components/app-shell/ChatDisplay.tsx)、[TurnCard](https://github.com/craft-ai-agents/craft-agents-oss/blob/e8963854c3679edcceb105a42537a06749e6cb64/packages/ui/src/components/chat/TurnCard.tsx) | 按回合呈现文本和工具活动 | 消息正文、可折叠工具卡、执行证据抽屉 |
| [backend/types](https://github.com/craft-ai-agents/craft-agents-oss/blob/e8963854c3679edcceb105a42537a06749e6cb64/packages/shared/src/agent/backend/types.ts)、[factory](https://github.com/craft-ai-agents/craft-agents-oss/blob/e8963854c3679edcceb105a42537a06749e6cb64/packages/shared/src/agent/backend/factory.ts) | 统一事件、能力驱动 UI；当前实现为 Claude/Pi backend | Rust CLI adapter 抽象；不把 Craft 当作任意本机 CLI 启动器 |
| [SessionManager](https://github.com/craft-ai-agents/craft-agents-oss/blob/e8963854c3679edcceb105a42537a06749e6cb64/packages/server-core/src/sessions/SessionManager.ts) | 会话恢复、发送、取消、持久化与 flush | 学习生命周期边界，分模块实现，避免单个巨大管理器 |

只借交互和结构；继续使用 Loom 品牌及现有主题令牌。若实施中引入源代码片段，记录来源和许可证声明。设计稿及视觉验证资源统一放 `designs/local-agent-chat/`。

## 5. 目标架构与契约

### 5.1 模块边界

```text
ChatShell / SessionList / Transcript / Composer
                  ↓ intent / ↑ snapshot + sequenced events
chatStore + useChatBridge + src/api/chatClient.ts
                  ↓ Tauri commands / events
chat/commands → chat/service → chat/runtime → agent_adapter
                    ↓               ↓            ↓
              chat/repository   ProcessSupervisor   CLI parser
                    ↓                            本机 CLI
           .loom/chat/v2/                      项目 cwd / 原有登录
```

- Rust 是会话与回合的权威来源；React 只保存投影、草稿和选中项，不直接拼命令或决定权限。
- 将 `src-tauri/src/chat.rs` 替换为 `chat/mod.rs`、`models.rs`、`repository.rs`、`service.rs`、`runtime.rs`、`commands.rs`。旧公共导出通过 mod 兼容，功能迁移后删除旧实现。
- `agent_adapter.rs` 暴露独立的 Chat invocation 请求和能力描述，不再用 Planning/Debugging 表示 Chat 权限。旧 Task 调用仍可通过原接口进入同一适配实现。
- `ProcessSupervisor` 使用 `ProcessOwner::Task { task_id } | Chat { project_key, session_id, turn_id }`；保留 task_runs 等兼容查询，Chat 绝不构造假 taskId。
- 不引入第二套进程管理器、不强行把 Chat 包装成 CommandRun/Task。共享进程生命周期，分别保存 Chat 与 Task 的领域记录。

### 5.2 数据与事件

| 模型 | 最小字段/约束 |
|---|---|
| ChatSession v2 | id、canonical projectKey/path、title、agentId、active/archived、timestamps、revision、lastSeq、schemaVersion、resumeHandle、消息/回合投影 |
| ChatTurn | id、sessionId、clientRequestId、agent/config/permission 快照、status、started/finished、exitCode、error、日志位置 |
| ChatMessage / parts | role、turnId、text/tool/command/error；toolCallId 关联输入输出，结构化输出与日志分开 |
| ResumeHandle | adapterType、agentConfig 指纹、providerSessionId、cwd、有效权限指纹；保存数据，不保存可直接执行的 shell 命令 |
| AgentCapabilities | version、textStream、toolEvents、nativeResume、stdinInput、supportedPermissionModes、approvalTransport；unknown 与 unsupported 分开 |
| ChatEvent | projectKey、sessionId、turnId、seq、kind、payload；seq 为会话内跨 turn 单调递增，重启不归零；涵盖 started/text_delta/text_snapshot/tool_started/tool_finished/diagnostic/finished |

回合状态：`starting → running → completed | failed | cancelled | timed_out | interrupted`；`starting/running → cancelling → cancelled`。终态只提交一次。UI 的 sending 来自对应会话活动回合，不使用页面级全局布尔值。

拟新增/替换 IPC：沿用并强化 `chat_update_meta`（标题/归档/权限），扩展 `chat_send`（含 clientRequestId）、`chat_abort`（必须含 turnId），新增 `chat_read_events`（afterSeq 分页补流）、`chat_read_run_logs`；保留 list/create/get 的兼容入口。`chat_get` 返回 snapshot 及 lastSeq，`loom://chat-event` 为统一新事件。同步更新 `src/api/contract.ts`、Rust contract fixture 和 TS/Rust 双向序列化测试，尤其 enum variant 内的 camelCase 字段。

**调用信封：** 当前 create/update_meta/send/abort/promote 需要 `{ input: {...} }`，list/get/set_agent/clear_resume 使用平铺字段。由 `src/api/chatClient.ts` 逐命令封装，不能仅用 `Record<string, unknown>` 隐藏形状差异。`contracts/tauri-contract.json` 补请求/响应样本并测试实际 wrapper；现有只检查 command/event 名称的测试不足。切 v2 事件时在同一提交更新 producer、consumer、fixture、Rust emitter 扫描，旧 UI 通过明确的兼容桥接过渡，不能断开数个里程碑。

先建立监听再发送；服务端先持久化再发事件。加载快照期间缓存事件，按 seq 去重/补齐；前端过时请求返回时检查项目和会话 generation。发送重试使用相同 clientRequestId 返回同一 turn，不自动重跑有副作用的调用。

### 5.3 运行时、续聊与权限

- 使用 program + argv + cwd 直接启动进程，不将用户消息拼成 shell 脚本；支持 stdin 输入，限制 argv prompt 长度。GUI PATH 缺失时可配置绝对路径；诊断不打印凭据。
- 所有 Chat 入口使用已注册项目的 canonical path；校验 sessionId/turnId 为合法单段 ID，检查 `.loom` 与数据文件 realpath 不越出授权存储根，拒绝 `../`、绝对路径和符号链接逃逸。磁盘 session.id/projectPath 必须与请求归属一致，cwd 由可信项目记录确定；迁移和日志读取遵循相同规则。
- 按 canonical projectKey/sessionId 持有运行锁；spawn 前登记 starting。启动前/中收到取消也能结束；spawn 失败、解析失败、退出、超时均释放锁。
- create/index 更新使用项目锁；send/update_meta/set_agent/clear_resume/归档/最终保存共用会话写入边界。活动回合不能归档、换 Agent、改权限或清 resume，后端必须校验；标题更新通过 revision 避免被终态保存覆盖。同一项目多 Loom 进程仅允许一个 Chat writer，其他实例明确提示占用。
- stdout/stderr 并发读取，增量处理跨 chunk JSONL、UTF-8、超长行和未知事件；限制内存缓冲及日志体积。未知事件进入脱敏诊断，不把原始 JSON 当 assistant 正文。
- 从 `agents/stream.rs` 提取可复用解析层；用各 CLI 的真实版本样本验证 delta、最终全文快照和工具结果，防止正文重复或混入工具输出。
- 进程组接入 supervisor；取消先 TERM，超过 grace period 再 KILL，等待回收；应用正常退出先停止活动回合并 flush。启动时不按历史 PID 盲杀其他进程，异常残留只能依据可核验的运行归属处理并提示。
- 原生 resume 有效时仅发送新消息；不支持 resume 的 adapter 使用明确标注的有限历史重放，给出字符预算和截断提示，不冒充原生连续上下文。2026-09-20 已由 `chat_context.rs` 落实传输切片；结构化句柄、权限指纹及真实 CLI 全量验收仍待本计划完成。
- Agent、cwd 或权限指纹变化后不能复用不相容句柄；活动回合禁止变更执行配置。失效 resume 保留转录并提示“以历史上下文开启新会话”，由用户操作，不静默重试执行。
- 首轮默认探索。`explore` 只有验证过限制写入/工具执行后才可用；`auto` 的实际能力由 adapter 声明，不能将 acceptEdits 等同于允许全部命令。新建与续聊必须套用同一有效限制。
- 当前 `ask` 保留“每回合发送前授权可写 CLI”的兼容语义；该 UI 授权不等于后端强制审批。后续逐工具审批作为独立能力，仅在存在并验收双向协议后提供；不静默将旧 ask 数据降级或宣称已支持工具级审批。能力驱动权限与后端授权校验属于待实施工作，当前 CLI 限制不足时明确显示限制。
- 沿用旧 PRD 的候选优先级 grok → codex → claude，但仅用于 M0 选择首条切片；“PATH 命中”不代表登录成功/协议正确。M0 以真实探针确定首个可用 Agent 和版本，并记录失败原因；已有用户默认选择优先。

### 5.4 存储与迁移

- 新布局：`.loom/chat/v2/sessions/<id>/snapshot.json`、同目录 `events.jsonl`（会话级唯一权威事件日志）；`runs/<turnId>/` 存脱敏 stdout/stderr 和执行元数据。index 为可重建缓存，不再额外维护另一份权威回合事件日志。
- 复用 `storage::atomic_write_json/atomic_write_text` 的同目录临时文件 + rename，并补所需错误清理。用户输入、turn 开始/结束必须完成文件写入/flush；流式批处理目标 250ms，写入后才发布 UI 事件。该目标是正常 I/O 下的处理延迟，不是断电损失上限；本阶段保证范围是进程崩溃恢复，不能把 rename/flush 当作 sync_all。硬断电持久性需要另行定义文件/父目录同步和验证。
- 提交顺序：追加权威事件并 flush → 更新内存投影及派生快照 → 发布事件；不能在日志提交前推进 lastSeq。2026-09-20 实施澄清：快照属于缓存，写入失败可从已提交 journal 重建，不撤销日志；权威日志失败才停止回合并显示非持久化 storage_error，不能假报已提交。未来版本/身份冲突不作为普通缓存损坏覆盖。
- 仅修复末尾不完整记录；中间损坏隔离提示，不能任意跳过坏行。事件补流按会话 seq 分页、去重，重启从最大已提交序号继续。
- 首次迁移只读解析旧 `.loom/chat/index.json` 和 `sessions/*.json`，生成 v2 副本；不要直接用可能原地升级源文件的 `migrations::read_versioned_json`。用迁移清单/完成标记区分“迁移一半”与完成，重跑续做缺失项，不能因 v2 目录存在就跳过。保留原文件，不移动/改写 Tasks、Review、反馈和验证证据。
- 旧 resumeCommand 仅解析允许的 adapter 格式为句柄；无法验证的保留诊断文本但不执行。未知未来 schema 只读提示，不降级覆盖。
- 重启将无活动 runtime 的 starting/running/cancelling 置 interrupted，恢复已落盘部分输出。仅恢复记录，用户手动继续；不自动重启命令。
- 回滚旧应用仍可读原 v1 数据；切回旧版前将 v2 新对话导出为 Markdown/JSON，不能声称旧版本可直接读 v2。

## 6. UI 方案

```text
┌ 项目导航 ─────┬ 会话列表 ──────────┬ 会话标题 / Agent / 状态 ─────────┐
│ 项目切换     │ 新对话 / 搜索      │ 用户消息                         │
│ 聊天         │ 进行中 / 已归档    │ 助手 Markdown                    │
│              │ 标题、预览、时间   │ ▸ 工具/命令及结果                 │
│              │ 生成/失败标识      │                                 │
│ 高级工作流   │                    │ 输入框                           │
│ 设置         │                    │ Agent · 权限 · 发送 / 停止        │
└──────────────┴────────────────────┴─────────────────────────────────┘
```

- 默认简洁三栏，导航约 180–220px，会话列表约 240–300px，正文限制阅读宽度；窄窗口折叠导航/列表，主消息区优先。右侧执行证据按需打开，不常驻空 Sources 面板。
- 新会话空态展示项目、可用 Agent 和输入区；未选项目/没有可用 Agent 时直接引导配置。Agent 选择器同时呈现路径、版本、可用性；不同 Agent 的能力区别可见。
- 中文为默认语言，沿用 `--loom-*` 令牌、现有字体与图标。用户与助手消息保持清晰层次；工具执行用可折叠行，错误可定位，输入区固定在底部。
- Enter 发送、Shift+Enter 换行；输入法组合期间不发送。Cmd+N 新会话；保留 Shift+Tab 的键盘反向聚焦，不照搬 Craft 的权限切换快捷键。
- 自动滚动仅在用户接近底部时跟随；向上查看历史时显示“有新输出”。每个会话保留草稿；切换会话不停止后台正在运行的回合。
- Markdown 禁用原始 HTML，代码块可复制；外部链接经已有安全打开边界。优先复用现有 Markdown 工具；若需新增前端库，在 M3 说明选择与安全/包体积理由，不引入整套 UI 框架。
- 设计验收覆盖：空态、运行中、工具展开、失败、取消、CLI 缺失、归档、窄窗口、长代码块。不新增无法工作的占位按钮。

## 7. 实施里程碑

任务按 0.5–1 个开发日拆分；较大项按模块/Agent 分别提交。阶段天数为粗估，新增前置修复后需在 M0 重估，不沿用原稿 15–22 日总量作为承诺。

**执行顺序：M0.0 IPC/测试基线 → M0 其余探针/契约 → M1（含状态隔离）→ M2 单 Agent 闭环 → M3 主界面 → M5 单 Agent 交付。M4 次级 Agent 为可选扩展，不是 M5 的前置门禁。**

### M0 — 确定真实接入能力与替代边界（1–2 日）

| 编号 | 具体任务/文件 | 依赖 | 验证与产出 |
|---|---|---|---|
| 0.0 | 新建 `src/api/chatClient.ts`，修正五个 input 信封并接入 `ChatPage.tsx`；更新 `contracts/tauri-contract.json`、`tests/unit/contract.test.cjs`、`tsconfig.test.json`；三项 Chat 镜像测试改为导入实际模块 | — | actual wrapper 逐命令参数断言；生产组件创建失败仍可见错误；桌面 create/update_meta 不需模型即可验 IPC，不以 Mock 替代 |
| 0.1 | 更新 `docs/requirements.md` 的阶段性 MVP；对齐 `docs/guides/craft-local-chat-prd.md`、`craft-local-chat-ia.md`，明确本计划优先及旧决策变化 | — | Chat 验收不再要求四阶段闭环；保留长期方向 |
| 0.2 | 在临时测试项目探测 grok/codex/claude 的 version/help、登录可用性、JSON 流、工具、resume、权限；新增 `docs/guides/local-agent-chat-protocols.md`、`tests/fixtures/chat/<adapter>/` | — | 每个 capability 标明版本、证据、通过/未验证/不支持；脱敏 fixture 入库；确定首选 adapter |
| 0.3 | 冻结 `docs/architecture/chat-contracts.md` 的 v2 模型/IPC/状态机；产出 `designs/local-agent-chat/README.md` 及线框资源 | 0.0、0.1、0.2 | 覆盖调用信封、seq/提交顺序、路径校验、迁移恢复、权限降级和进程 owner |

**退出条件：** 至少一个真实 Agent 在指定 cwd 有可解析输出及有效限制；不满足则记录明确阻塞，不靠 Mock 宣称通过。探针只用测试目录，付费/登录失败单独记录。

### M1 — 替换 Chat 领域、仓储与 IPC 底座（3–4 日）

| 编号 | 具体任务/文件 | 依赖 | 验证与产出 |
|---|---|---|---|
| 1.1 | 拆 `src-tauri/src/chat.rs` 为 `chat/{mod,models,commands,service}.rs`；更新 domain、API、Rust contracts 及 `contracts/tauri-contract.json` | 0.3 | 请求/响应/parts 序列化、旧入口兼容与事件切换点明确；拆文件不破坏扫描 |
| 1.2 | 实现 `chat/repository.rs`，复用 `storage.rs` helper；只读迁移、项目/会话锁、唯一 writer、可重建 index；抽取 `projects.rs` canonical 校验 | 1.1 | ID 穿越/symlink/伪造 cwd、并发 create、多进程写入、中文路径、迁移中断重入、坏尾行、未来版本与写失败通过 |
| 1.3 | 在 `chat/service.rs` 加幂等、会话 seq/revision、活动回合 mutation 限制和日志分页；repository 提供 JSON/Markdown 导出供回滚使用 | 1.2 | 双发送仅一次；归档后先恢复再发；活动回合不能改执行配置；元数据不被终态覆盖；导出可读 |
| 1.4 | 新建 `src/features/chat/state/chatStore.ts`、`src/hooks/useChatBridge.ts`，接入 M0.0 chatClient 与现有 ChatPage，隔离项目/会话/turn、请求 generation 和草稿 | 1.3 | A 结束不跳走 B；切项目清旧选中；listen 就绪前不发送；响应不覆盖已到达事件；移除 120 秒 UI 假超时，兼容旧事件的过渡路径受测试保护 |

**退出条件：** 会话 API 可独立验证，老数据可读，新存储回滚路径可执行。此时不宣称真实 Agent Chat 完成。

### M2 — 首个真实 Agent 的可靠聊天闭环（4–5 日）

| 编号 | 具体任务/文件 | 依赖 | 验证与产出 |
|---|---|---|---|
| 2.1 | 扩展 `agent_adapter.rs` Chat 请求/能力/权限/resume；提取 parser 至 `agent_adapter/stream.rs`；更新 `agent_diagnostics.rs` 和 picker 的首选 Agent 诊断 | 0.2、1.1 | fixture 与实际 CLI 相符；原生 resume 不重复注入历史；权限有效；区分路径存在、版本失败、auth unknown 和调用验证通过 |
| 2.2 | 扩展 `process_supervisor.rs` owner/超时终止；同步 `command_runner.rs`、`pty.rs`、`implementation_review.rs`、`agents/stream.rs`、`agents.rs`、`run_recovery.rs` 的创建/查询/停止调用点 | 1.1 | 保留 Task 构造/query 兼容及原测试；Chat 无假 taskId；精确停止对应 turn，任务恢复不把 Chat 算作 Task 进程 |
| 2.3 | 实现 `chat/runtime.rs` 排流/stdin/取消/超时/脱敏/事件；同时新增 `tests/fixtures/chat/fake-agent.*` 和 runtime 集成测试，提供初版 `scripts/chat-smoke.sh` | 1.3、2.1、2.2 | 测试直接调用生产 runtime；大 stderr 不死锁；非零退出保留正文且失败；启动中取消、孙进程持有管道和超时不挂起；无需凭据 |
| 2.4 | 接入增量持久化/重放与关闭恢复；更新 `src-tauri/src/lib.rs` 注册和退出处理 | 2.3 | 终态持久化先于 finished 事件；中途退出后 partial 保留且状态 interrupted |
| 2.5 | 用 M1.4 bridge/已有组件切换到 v2 IPC；记录 `docs/dogfood/local-agent-chat-<date>.md` | 1.4、2.4 | 真 CLI 三轮、工具、停止、重启，以及 A/B 切换和迟到事件均通过；不得等 M3 才补此阶段所需隔离 |

**退出条件：** 首个 adapter 从桌面端可用，满足成功标准 2–6。故障修复优先于第二个 Agent 或视觉细节。

### M3 — Craft 风格 Chat 主界面与状态隔离（3–4 日）

| 编号 | 具体任务/文件 | 依赖 | 验证与产出 |
|---|---|---|---|
| 3.1 | 新建 `ChatShell.tsx`，复用 Inbox/Header；在 `src/App.tsx` 分离旧 Header/Navigation/Task 加载，抽取共用项目导航；拆 `useAgentBridge.ts` 为 Agent catalog 与 Planning 订阅，更新 Chat/Settings 使用方 | 0.3、2.5 | Chat 不显示任务阶段条、不加载 Tasks、不订阅 Planning；项目管理与高级任务入口可用；仅删 AppContent 一个 hook 不算完成 |
| 3.2 | 完善 M1.4 已有 store/bridge 的每会话草稿、分页和日志读取；保留已有组件，不再重复新建 client/store | 2.5 | 切页卸载/重挂载补齐事件，无重复订阅；部分消息/工具状态与快照一致 |
| 3.3 | 完善已有 `ChatTranscript.tsx`、`ChatComposer.tsx`，新增 `ToolCallCard.tsx`、`RunLogDrawer.tsx`，重做 `ChatPage.css` | 3.1、3.2 | Markdown/代码块、工具/错误、停止、草稿、IME、键盘焦点、滚动全部可用 |
| 3.4 | 复用归档和 update_meta，补 rename/restore/search UI；接 M2.1 诊断与能力权限；完善 `tests/unit/chatInteraction.test.cjs` | 3.3 | 初始无会话时 API 错误也可见；重启元数据保留；无假审批；IME/反向 Tab/窄窗口通过，截图存设计目录 |

**退出条件：** 页面所有主操作走真实 API；网页预览标注模拟，测试与预览才能使用 `mockStore.ts`。

### M4 — 可选扩展：次级 Agent、兼容性与故障处理（2–3 日）

| 编号 | 具体任务/文件 | 依赖 | 验证与产出 |
|---|---|---|---|
| 4.1 | 按 M0 探针结果逐个补齐其余 Codex/Claude/Grok 的 adapter/parser/fixture；更新协议矩阵 | 2.5、3.4 | 每个标记支持的 Agent 均需真实验收；不可用保留明确原因，不伪装支持 |
| 4.2 | 扩展 M2.1 已有诊断到次级 adapter，完善版本能力矩阵和 Settings 展示 | 4.1 | 版本失败不显示“可聊天”；auth unknown 不伪装未登录或已登录；GUI 绝对路径配置可用 |
| 4.3 | 补配置切换、resume 失效、损坏索引、权限变化、未知事件、长输出与重启恢复故障场景 | 4.1、4.2 | UI 提示可操作；旧上下文不跨 Agent 泄漏/误用；失败不自动重放副作用 |

**退出条件：** 第二个 adapter 证明无需修改 Chat 核心；只有一个可验收 Agent 时 M4 保持 deferred，记录具体原因并允许 M5 交付单 Agent MVP。未通过真实验收的 adapter 禁用或明确标实验性；不能将 M4 宣称完成。支持范围扩展需另行复验。

### M5 — 清理替代路径与交付验收（2–4 日）

| 编号 | 具体任务/文件 | 依赖 | 验证与产出 |
|---|---|---|---|
| 5.1 | 删除旧 Chat spawn/parser/临时事件桥接，限制 Mock 为显式预览；整理高级路由并更新 `docs/architecture.md` | 3.4 | 每个真实回合仅一个 runtime；Task/设置回归通过；启用 M4 adapter 时另须完成相应验证 |
| 5.2 | 整合 M1–M3 已有回归与 M2.3 smoke，补迁移/导出回滚和真实 CLI 验收入口 | 5.1 | 无凭据故障矩阵与真实 CLI 分开报告；不在此阶段才首次建立 runtime 测试 |
| 5.3 | 运行完整检查、Tauri 构建、真实桌面验收；归档脱敏证据并更新本计划及 `docs/PLANS.md` | 5.2 | 按下面矩阵逐项填写结果，失败不标 completed；停止预览无残留 |

## 8. 验证策略与交付证据

### 自动化门禁

- 每阶段运行与改动相关的前端测试、Rust 单元/集成测试；使用现有 Node test/jsdom 和 Rust 测试设施，新增 TS 文件需纳入 `tsconfig.test.json`。
- 所有 Chat 测试导入生产模块或挂载生产组件；禁止复制 `MockChatStore`、权限函数后只测试副本。Mock transport 必须断言真实 wrapper 的 input 信封，Rust 对共享 JSON 样本反序列化；至少一次真实 Tauri dispatch 验证 create/update_meta，不能只检查命令名称。
- M5 执行 `pnpm check`：现有脚本包含发布文档检查、前端测试和 build、Rust fmt/clippy/test；再执行 `pnpm tauri build --no-bundle` 验证桌面产物。不重复添加功能相同的检查。
- `scripts/chat-smoke.sh` 在测试临时项目中运行，不操作真实仓库文件；包含 stdout/stderr 并发、大消息、spawn 失败、exit!=0、timeout、cancel、进程组、seq 重放、幂等和存储失败。
- Task 兼容回归至少覆盖现有 `src-tauri/tests/workflow_harness.rs`、契约测试、旧项目任务加载及设置保存。不能为使新测试通过删除旧工作流断言。

### 真实桌面验收矩阵

| 场景 | 可验证结果 |
|---|---|
| 无模型的真实 Tauri create/update_meta/get | input 信封可解码，新建/重命名/归档落盘；错误在空态也可见 |
| 首次使用、无 CLI、未登录 | 引导配置/登录；没有伪成功消息 |
| cwd 含空格/中文，项目 A/B | 命令在所选项目执行，信息不跨项目串流 |
| 连续三轮对话 | 第二、三轮能引用前文；原生 resume 句柄正确，无历史重复注入 |
| 工具读取与测试文件写入 | 工具输入/结果可查；探索拒绝写入；支持的可写模式只在测试目录验证 |
| 生成中停止，再发消息 | 在时间预算内回收进程；partial 保留；新回合不受旧 finished 影响 |
| 生成中切换会话/Agent | 切会话继续后台运行；活动会话禁止换配置，空闲切换重置不兼容 resume |
| 正常退出/强制退出后重开 | 已持久化内容保留；未完成回合 interrupted；可手动继续，无自动重复写入 |
| 服务失败/限流/无结构化输出 | 保留诊断和退出信息，不混入成功回答；重试是明确用户操作 |
| 归档、重命名、搜索、设置 | 刷新及重启后保留；旧任务仍可从高级工作流访问 |
| 非法 ID / `.loom` symlink / 修改磁盘 cwd | 拒绝越界，不在其他项目执行；损坏数据隔离可恢复 |
| 同时创建/改标题/完成回合、多应用实例 | index 不丢会话，标题不被旧快照覆盖；第二 writer 被明确拒绝 |

真实验收记录写 `docs/dogfood/local-agent-chat-<date>.md`：Loom commit、OS、CLI 路径/版本、权限模式、测试 cwd、脱敏命令、退出状态、结果和证据路径。未登录/未安装按 blocked 或 not tested 记录，不能用 fixture 填 passed。

启动前执行 `scripts/debug.sh stop`；桌面优先 `scripts/debug.sh`，仅 UI 预览用 `scripts/debug.sh web` / `scripts/preview.sh`（固定 1420）；结束执行 `scripts/debug.sh stop` 和 `scripts/preview.sh stop`，检查无本仓库残留进程。日志使用既定 `/tmp/loom-preview-vite.log`，不得另开随机端口绕过冲突。

### 回滚

- 按里程碑提交，先保留旧工作流和 v1 数据，再切新 Chat；新旧 runtime 不同时接收发送操作。
- M1–M4 出现阻塞可回退对应提交；迁移副本不覆盖旧数据。先完成 v2 导出再退回只能读取 v1 的应用。
- M5 清理仅删除被替代的实现，不清理用户 `.loom` 历史。数据不可读优先恢复仓储兼容，不执行删除重建。

## 9. 风险

| 风险 | 概率 / 影响 | 缓解与触发条件 |
|---|---|---|
| CLI 协议/登录/权限随版本变化 | 高 / 高 | M0 固定版本和样本，未知能力禁用；版本升级跑 adapter 契约和真实 canary |
| 只读/Ask 只是 UI 文案，resume 继承更宽权限 | 高 / 高 | 去掉 stage 借用，验证每次启动有效权限；无法限制则阻止模式；无双向协议不宣称逐工具审批 |
| stderr 阻塞、取消竞态、子进程残留 | 高 / 高 | 双路并发、starting 注册、精确 owner、TERM/KILL + reap、故障 CLI 验证 |
| 事件早于订阅或完成早于落盘 | 高 / 高 | 先监听、持久化后发布、seq + snapshot 补流、终态幂等 |
| 历史丢失或旧版无法读新数据 | 中 / 高 | v2 独立副本、幂等迁移、日志恢复、导出回滚，不原地破坏 v1 |
| 彻底重构变成无限扩展 | 高 / 高 | M2 真实闭环为首道门禁；Task/Sources/MCP/复杂审批后移；禁止不可用占位功能 |
| 长对话/大工具输出造成卡顿和磁盘膨胀 | 中 / 中 | 分页、批量事件、正文/日志分离、显式截断；禁止无限全量 raw 字符串 |
| 旧 Task runner 因共享设施变化回归 | 中 / 高 | owner 兼容接口、原测试保留；按小提交切换，不先删除旧工作流 |
| 名称契约/镜像测试全绿却没有验证真实 IPC | 已确认 / 高 | M0.0 修调用信封与实际模块测试，M2 真实桌面验收 |
| session 路径逃逸、磁盘 cwd 伪造、多 writer 互相覆盖 | 已有校验缺口 / 高 | M1 校验真实路径及归属，项目/会话锁和单 writer，覆盖迁移与日志接口 |

## 10. 待确认问题与默认决策

下列问题不阻塞先 Review 本计划；实施按默认值推进，只有实测不足时才将对应项列为阻塞。

| 问题 | 默认方案 | 何时确认 |
|---|---|---|
| 首个真实验收 Agent？ | 保留旧 grok → codex → claude 候选顺序，以版本/认证/协议/权限探针决定；用户已有默认优先 | M0 结束 |
| 首批需要几个 Agent？ | 单 Agent 完成 M0–M3/M5 即可交付；M4 可 deferred，只有真实验收通过者列“支持” | M0 确定发布范围 |
| 是否需要第一版就有逐工具审批？ | 不作为 Chat MVP 门禁；当前 Ask 是回合授权，后续逐工具审批需专门的双向 adapter | M0 契约 Review |
| 旧工作流最终是否删除？ | 本阶段移出主流程，保留高级入口及全部历史；后续另案决定删除/迁移 | Phase 2 |
| 是否保留项目内 `.loom` 存储？ | 保留，v2 子目录；不额外引入数据库/云端存储 | M1 前 |

## 11. 完成检查表

- [ ] M0 协议探针、需求顺序和 v2 契约已对齐。
- [ ] M1 数据迁移与会话 API 通过，旧数据无损。
- [ ] M2 首个真实本机 Agent 闭环通过。
- [ ] M3 Chat 主界面和状态隔离通过。
- [ ] M4 次级 Agent 验证通过，或明确 deferred 且首发仅开放已通过的单 Agent。
- [ ] M5 清理、检查、桌面构建、真实验收和回滚样本通过。
- [ ] 完成状态、证据与 `docs/PLANS.md` 同步；未验证能力明确列出。

## 12. Plan Review — 2026-09-17 12:06

- Reviewer：Codex；计划日期 2026-09-17。
- Verdict：原稿 **needs-rework**，上述修订已写入本文件；尚未修复代码，也未将里程碑标为完成。
- 计划概要：保留六个里程碑和 Tauri/Rust 路线，先交付可靠的本机 Chat。调整为 IPC/测试优先、状态隔离前移、UI 复用、次级 Agent 可延后。

### 相对计划的代码变化

- HEAD 仍为 `b8e35c7`，相对上轮最后基线无新增提交。相对初始 `3127ca8`，M1 新增四个 Chat 子组件、权限 helper、`chat_update_meta`、归档过滤及镜像测试。
- 未发生影响计划的文件重命名/删除；`chat.rs` 未提交差异为格式整理。没有 `chat/` v2 模块、Chat store/bridge、事件日志或 runtime 集成测试落地。
- 因此问题不只是计划过时，还包括原审查遗漏的真实 IPC 缺陷、没有对应生产实现的测试、里程碑依赖矛盾。旧 UI 拆分可保留，不能直接认定已具备生产可用性。

### Findings 与已采用修正

| # | 严重度 | 原计划位置 | 代码证据 / 问题 | 已采用修正 |
|---|---|---|---|---|
| R1 | blocker | M0 缺失、M2.5 才接 IPC | `ChatPage.tsx:66/235/258/277/362` 五处传平铺参数；`chat.rs:364/440/527/568/582` 接 `input`；`tauriClient.ts:19` 原样转发。桌面 dispatch 在调用业务函数前即可因缺 input 失败；Mock 分支无法发现 | 新增 M0.0，统一有类型的 chatClient 信封，并提前验证真实 Tauri create/update_meta；原泛型返回值不构成参数类型检查 |
| R2 | major | M5.2 测试过晚、默认信任现有测试 | 三项 `tests/unit/chat*.test.cjs` 自己复制实现；`tsconfig.test.json` 未编译 Chat features；Rust Chat 只有两个权限映射测试。无法发现上述 IPC/状态/runtime 问题 | M0.0 改实际导入；M1.4 加生产组件/bridge 测试；fake CLI/runtime 测试随 M2.3 建立，M5 仅整合 |
| R3 | major | M2 退出条件依赖 M3.2 | `ChatPage.tsx:175–198` 对任意 finished 清 sending 并调用 loadSession(payload.sessionId)；A 结束可抢走当前 B，项目切换也没有 generation 校验；`:268` 旧计时器可解锁新回合 | 状态隔离从 M3.2 提前至 M1.4，并成为 M2.5 显式依赖；M2 验收 A/B、早到/迟到事件，不允许延期 |
| R4 | major | M3.1 改动清单不完整 | `App.tsx::AppContent` 只是一个入口；`Header.tsx` 在 Chat 仍按非 Board 显示阶段条，`Navigation.tsx` 耦合 Task；`useAgentBridge.ts:31` 自动订阅 Planning | 补 Header/Navigation 路由隔离、项目导航复用和 catalog/planning hook 拆分；检查 Chat 无 Task 加载/Planning 订阅 |
| R5 | major | §5.3 / M1.2 缺存储边界 | `chat.rs:214` 直接拼 sessionId；`:238` 不验证读出的 ID/cwd；`:616` 用磁盘 projectPath 执行。改仓库 JSON 或使用路径片段可越过预期项目归属 | 增加单段 ID、canonical/realpath、session 归属、可信 cwd 验证，覆盖 read/update/migrate/logs；复用 projects/task_repository 的现有模式 |
| R6 | major | §5.4 恢复约定不完整 | seq 未定义作用域而日志按 turn 分散；rename/flush 被写成断电损失保证。`storage.rs:213` 已有 atomic helper，但未 sync；通用 migrations 读取旧版本可能原地升级 | 明确会话级 seq/journal、日志→快照→事件顺序；复用 atomic helper，保证范围限定进程崩溃；只读副本迁移及中断重入 |
| R7 | major | M1.3 只覆盖 send、回滚缺交付任务 | `chat_update_meta/chat_set_agent/chat_clear_resume` 无共同写锁/活动状态检查；完成保存也会读改写同一文件；create 的共享 index 可丢并发更新。原计划要求导出回滚却无实现任务 | 项目锁保护 index，所有 mutation 单写入者+revision，活动回合后端拒绝配置变更；M1.3 实现 v2 导出，多应用 writer 明确受限 |
| R8 | major | M4.2 才诊断，M2 默认能力不实 | `agent_diagnostics.rs::diagnose_agent` 找到路径后即使 version 失败仍可 ready，auth 始终 unknown；Chat picker 只过滤 enabled/dummy | 首个 adapter 的诊断与 capability 提前至 M2.1；UI 区分找到命令、兼容版本和真实调用已验证；能力未核实不开放权限 |
| R9 | major | M5.1 依赖 M4.3，与“先一个 Agent”矛盾 | 若第二 Agent 缺登录/版本不兼容，M4 被阻塞，原依赖会连带阻塞单 Agent 发布 | M5 仅依赖 M3；M4 deferred 明示，首发禁用未验证 adapter；启用任何新增 adapter 前再完成其验收 |

### 实际验证结果与限制

- `pnpm test`：115/115 通过；日志 `/tmp/loom-plan-review-frontend.log`。由于包含镜像实现测试，这不是 Chat UI/IPC 可用性证明。
- `cargo test --manifest-path src-tauri/Cargo.toml chat -- --nocapture`：2/2 通过，198 个其他单元测试和 2 个 workflow 集成测试被筛选未运行；日志 `/tmp/loom-plan-review-rust.log`。
- 静态核对范围：App/Shell/Chat 子组件与 hooks、API client/contract、domain、Rust Chat/adapter/parser/session capture、supervisor 调用方、storage/migrations/task repository/projects/diagnostics、测试入口和预览/check 脚本。新建文件已在任务中明确标注，不当作现有代码。
- 本轮没有运行完整 `pnpm check`、Tauri build 或真实 CLI/桌面 E2E；没有更改代码来修上述缺陷，也未触碰已有未提交 Rust 格式变化。

### 推荐执行顺序与待确认

1. 先完成 M0.0，让生产 IPC 和测试基线可信；同时保留旧 UI 拆分成果。
2. M0.2 决定实际可验收的首个 Agent/版本，再完成 v2 契约、仓储和状态隔离。
3. M2 通过真实聊天闭环后完善 M3，完成 M5 即交付单 Agent MVP；按需要继续 M4。

没有新增必须由用户拍板的问题。首个 Agent、可验证权限与平台范围仍按第 10 节默认方案，通过探针决定；不将“可执行文件存在”当作已登录或已验收。

## 13. 实施记录 — 2026-09-20 仓储与活动回合

本轮先处理真实 Chat 的数据一致性缺口，为 v2 拆分提供可用底座；保留 v1 数据格式，未用 v1 加固替代本计划的 v2 交付目标。

- 将 `chat.rs` 迁移至 `chat/mod.rs`，提取 `models.rs`、`repository.rs`、`service.rs`；更新 Tauri emitter 契约扫描，IPC command 名称和信封保持兼容。
- 生产路径统一经过仓储：原子 JSON 写入、项目内 mutation 串行化、OS writer 锁。index 作为缓存，完整会话文件可以在 index 损坏后重新发现。
- 启动进程前预留回合，避免 get/list 将活动回合误恢复；同会话拒绝重复发送，取消核对项目/session/turn，启动前取消写入回合归属并由进程启动边界检查。
- 所有会话改动在共享写边界内完成，活动回合不能归档/换配置/清 resume；完成时读取最新元数据，落盘后才构建终态事件。旧回合不能清除新回合的活动记录。
- 校验已登记/canonical 项目、单段 ID、磁盘 schema/id/cwd，拒绝静态 symlink 目录/文件；不覆盖未来版本或伪造项目的会话。尚未声称抗并发恶意路径替换。
- 修正工具 parts 的 camelCase wire 字段，并兼容读取旧 snake_case；原始内容无破坏性迁移。
- 使用 Rust 标准库文件锁，声明最低 Rust 1.89；本机 1.95.0 验证，不新增依赖。
- 完整回归中补修独立 IdGenerator 同毫秒复用 run ID：计数器改为进程内共享，保持 ID 格式不变，避免共享进程登记互相覆盖；修复前有实际失败的生产函数回归。

验证：`pnpm check` 最终通过（前端 143、Rust 单元 237/忽略 2、workflow 集成 2），最终代码 `pnpm tauri build --no-bundle` 通过。原始失败、修复过程和证据范围见 [2026-09-20 工程验证记录](../../dogfood/2026-09-20-chat-repository.md)。本轮没有真实模型/桌面交互验收。

仍须继续：v2 journal 与迁移、独立 ProcessOwner、排流/强制停止/超时回收、真实 CLI 权限及续聊、桌面验收、同一候选产物的签名/公证/安装发布门禁。历史发布资料仍为 Hold，不能据旧 DMG 或旧 smoke 宣称本次可发布。

## 14. 实施记录 — 2026-09-20 Runtime 与 Grok 真调用

- 提取 `chat/runtime.rs` 并接入生产命令，stdout/stderr 并发排流、可选 stdin 并发写入并关闭；Codex/Claude 接入现有 adapter 的 stdin 能力，Grok/自定义模板保留 argv。
- `ProcessMetadata.owner` 区分 Task 与 Chat，保留 Task 构造/query 兼容；不再使用伪 taskId。监督器拒绝重复 run id，不覆盖现有进程归属。
- 停止和超时采用 TERM → KILL → 有界回收；处理父进程退出而子进程持管道、已关闭管道但仍存活的子进程、异步 future 被取消。删除旧独立 timeout watchdog，终止计时和 I/O 在同一循环内。
- 加入字节/行长/行数/工具片段限额，处理 UTF-8 分片、输出消费错误和非零退出；非零退出保留 partial 但标 error。未知结构化事件不作为原始 JSON 正文；补 Codex agent_message 文本识别。
- `tests/fixtures/chat/fake-agent.sh` 经生产 runtime 执行；11 项进程测试覆盖真实失败路径，其中父进程退出后子进程持管道场景连续 20 次验证。新增 `pnpm smoke:chat` 无账号回归入口。
- 增加显式 opt-in 的 `real_agent_tests.rs`；`LOOM_REAL_CHAT=1` 且 `--ignored` 才请求真实模型。Grok 1.0.30 三轮各有流式文本、原生 resume、磁盘仓储重开，随机标记准确且不重复；当前仍是 headless 验收，不是 Tauri 交互证明。
- 真实 canary 发现 Grok 旧输出格式 flag 不匹配；改为 `streaming-messages-json` 并更新 adapter 测试与探针文档。诊断保留 stderr 有界尾部，避免启动警告淹没真正错误。
- 连续回归发现重复 KILL 的时序问题：已强制终止后的收口路径不再重复发 KILL；问题在普通本机权限下也出现，因此不归因于沙箱或跳过测试。

本轮验证与失败过程见 [Runtime 工程与真实 canary 记录](../../dogfood/2026-09-20-chat-runtime.md)。接下来仍须完成 v2 journal/迁移/事件补流、CLI 权限与工具/停止实测、Tauri 桌面验收及同候选发布门禁。以上不替代原始完整目标。

最终门禁：`pnpm check` 通过（前端 143；Rust 单元 250/忽略 3；集成 2）；`pnpm smoke:chat` 52/忽略 1 通过；`pnpm tauri build --no-bundle` 通过。真实 Grok 三轮 opt-in 测试另行运行通过，不计入默认忽略项的通过数量。

## 15. 实施记录 — 2026-09-20 v2 日志与生产桌面接线

- v1 文件只读复制到 v2；完整换行提交的 events.jsonl 为权威来源，snapshot 为派生缓存。字段/消息 patch 与 stream delta 避免逐 token 重写整个转录。
- 补流 API、统一持久化事件与非持久化错误通知已经同轮接通 Rust/TS/fixture/真实 ChatPage；前端按 project/session/seq 缓冲、去重、补缺，拒绝陈旧响应。
- 迁移中断重入、尾部修复、完整坏行/未来版本拒绝、坏会话可见隔离、writer 进程直接退出后 partial 恢复均有生产路径验证。
- 元数据/状态采用 session_patch 编码，stream 包含 text/tool 增量；创建记录建立基线。完整 ChatTurn 生命周期语义仍待补，不能以当前 idle/streaming 兼容状态替代原目标。
- 追加并 flush 日志是确认持久化的边界；缓存 checkpoint 失败时可以从日志重建，不撤销已经提交的事件。权威日志失败仍停止回合并报告，不伪装成功；没有硬断电保证。
- 真实 QA App 通过 A/B 后台完成隔离、正常退出重启、B 连续三轮真实 Grok 记忆标记、重命名、归档/恢复、搜索；原生句柄在三次完成记录中一致。
- 由真实 UI 验收补修启动误弹框、虚构/错目录检测结果、IME 标题提交和反向 Tab 改权限；迟到确认按原项目/会话清理已提交草稿。

最终检查：前端 160、Rust 单元 258（忽略 3）、workflow 集成 2 通过；QA App 包构建与原生 UI 操作通过。详细证据、配置差异和范围见 [v2 验收记录](../../dogfood/2026-09-20-chat-v2.md)。

下一实施优先级仍为：M1.3 clientRequestId/完整回合记录/导出，M2.1 结构化续聊与实际权限，独立执行日志与运行中退出恢复，再完成 M3 默认 Shell 分离和 M5 同候选发布门禁。当前 UI 验收不替代这些未完成项，也不把 QA 包认定为正式 release。

## 16. M1.3 发送幂等实施切片（2026-09-20）

本切片目标：发送回执丢失、完成后重试和重启后重试均不能再次启动同一请求。当前只有活动回合互斥，没有持久化请求标识。

- `chat_send` 强制携带 `clientRequestId`；前端一次提交分配一个标识，失败重试复用，收到明确回执后新提交使用新标识。
- 仓储在同一写锁内先查持久化回执，再校验活动/归档与准备执行；相同请求返回原 turn 和最新 session，不再次准备或 spawn。复用标识但改变正文或请求权限时报冲突。
- 回执与用户/助手消息在同一 journal 记录落盘，包含请求标识、回合及消息关联；旧会话缺少回执时默认为空。重启只恢复中断状态，不重新执行已接收请求。
- 同步 Rust/TS、IPC 样本和生产 wrapper；验证并发双发送、完成/归档/重开仓储后的重试、冲突、落盘失败、前端回执丢失以及项目/会话隔离。
- 此切片不宣称完整 ChatTurn、运行日志、续聊权限和发布门禁已完成。测试/开发产物名称统一保持 `Loom`。

实施结果：本切片已完成。`pnpm check` 前端 163、Rust 267（3 ignored）、集成 2 通过；默认 Loom.app 构建通过。增强真实 Grok canary 三轮及活动/完成重开后的去重通过，9 次接收调用仅产生 3 个回合；桌面保留用户当前操作，自动重复请求 UI 验收未计通过。详见 [发送幂等证据](../../dogfood/2026-09-20-chat-idempotency.md)。M1.3 的完整回合/日志/导出仍未完成。

## 17. M2.1 续聊绑定与参数边界（2026-09-20）

现状：Codex resume 分支未重设 sandbox/cwd 且绕过 stdin；Claude 会复制历史续聊字符串中的额外参数；空格路径解析失败；Chat 只保存原始 resumeCommand，无法辨认设置内的执行配置变化。

实施步骤与验收：

1. 依据本机 help 和官方 Codex CLI 文档统一首轮/续聊参数；Codex 在 exec 层明确 sandbox/cwd，stdin 保持一致。Claude 首轮/续聊显式重设 plan/acceptEdits。历史字符串仅允许确切程序路径、adapter 对应 resume 操作和单个有界 ID，绝不复制历史额外参数；拒绝缺失、选项式或跨程序 ID。
2. Chat 保存版本化 `resumeHandle`（adapter、sessionId、执行配置 SHA-256），绑定 Loom 的 Agent/命令/参数/目录/权限配置。使用已在锁文件中的 sha2 0.10 作为直接依赖，避免将可能含凭据的配置原文写入指纹。句柄不匹配或旧记录无指纹时明确要求“开新 CLI 会话”，保留转录，再由用户操作后重放有预算的历史；不静默执行旧命令或重试。
3. 完成时用该次 prepared 配置创建句柄，不能读可能已变化的 Agent 配置；换权限/Agent/清 resume 同时清除句柄。UI 区分可续聊与需重建的旧记录。
4. 生产 adapter/context/仓储测试覆盖新建与 resume 权限一致、额外参数拒绝、路径含空格、前导连字符正文、配置变化和旧记录；实际 CLI 在合成目录验证三轮/重开续聊，文件权限探针单独记录，不以参数断言替代安全保证。

全量工具权限矩阵、独立日志及应用退出仍按主计划继续，本切片不宣称完成全部 M2。

实施记录：结构化句柄/指纹、独立 Chat 权限、参数边界已接生产；真实探针发现同帧多工具结果丢失并已修复。最终 `pnpm check` 前端 164/Rust 271（4 ignored）/集成 2、默认 `--no-bundle` 构建通过；Grok 三轮结构化续聊通过。文件探针整体仍 failed/not completed，修复后重跑因读取并可能外发私人 skill 内容被自动审批拒绝，已请求明确授权，未绕过。详见 [续聊与权限证据](../../dogfood/2026-09-20-chat-resume.md)。当前继续推进未受影响的回合/日志/导出等工作，不能据此标记 M2 或发布完成。

## 18. M1.3/M2.3 回合记录与日志（2026-09-20）

本轮不请求外部模型，真实 Grok 权限重跑继续等待明确授权。

1. 在 ChatSession 投影增加 ChatTurn：请求/消息关联、不可修改的脱敏 invocation 配置、starting/running/cancelling 与 completed/failed/cancelled/timed_out/interrupted 状态、接收/启动/结束时间、退出码/原因、stdout/stderr 日志引用。旧会话缺省为空，不伪造旧回合运行证据。
2. CLI spawn 前回合和配置随请求原子提交；runtime 观察启动和分离后的输出，逐行脱敏后写独立有界日志。失败必须可见；启动失败、取消、超时、非零退出及重启中断各自保存正确终态。终态和 invocation 不允许随后被改写。
3. 新增 `chat_read_run_logs` 有界字节游标分页；项目登记、会话/回合归属、ID/路径/symlink 与 UTF-8 校验沿用仓储边界。不接受调用者提供任意日志路径。
4. 转录每个已记录回合显示状态和日志入口；stdout/stderr 切换、分页/刷新、错误提示与项目/会话切换隔离。真实 CLI 失败不能变成“无日志”成功态。
5. 用生产 runtime 的本地子进程 fixture 验证分流、脱敏、退出码、停止/超时/启动失败与磁盘恢复；生产 IPC/组件验证分页和迟到响应。通过后更新证据和索引。日志/回合不替代仍待完成的导出、退出清理与发布门禁。

实施结果：回合状态/不可修改配置、独立脱敏日志与日志面板已接生产。最终 `pnpm check` 前端 166/Rust 277（4 ignored）/集成 2 及默认 `--no-bundle` 构建通过；本地子进程覆盖退出7、超时/取消/启动失败/恢复/日志写入失败，浏览器生产组件完成分页/切流与窄内容区检查。详见 [回合与日志证据](../../dogfood/2026-09-20-chat-turn-logs.md)。真实外部验收仍待先前请求的授权，本轮未调用模型；M1.3 导出及 M2 应用退出/真实矩阵、M3/M5 尚未完成。

## 19. M1.3 导出与回滚（2026-09-20）

1. 新增 `chat_export`，由 Rust 在项目 `.loom/chat/exports/<exportId>/` 生成独立目录，含完整 JSON 快照、可读 Markdown、manifest 及运行日志副本。不接收任意目标路径，不覆盖已有文件；先写 staging，全部成功后 rename 发布。
2. 会话快照固定在已提交 seq；日志按捕获的完整换行前缀复制并记录字节界限。运行中导出标记 inProgress：快照与各日志边界分别标明，不伪装为最终结果；不持锁等待整个日志复制过程。缺失/损坏日志在 manifest 明示，不能默认为完整成功，转录仍可备份。
3. Markdown 中用户/Agent/工具内容按字面代码块保留，动态围栏防止内容闭合；不把原文 HTML 当可执行内容。JSON 保留回合、回执、句柄和游标；正文可能包含用户输入的敏感信息，导出仅写本机，不自动外发。
4. 会话菜单提供“导出会话（JSON + Markdown）”，显示目录和运行中/缺失证据说明，支持系统文件夹定位；失败/跨会话迟到结果不污染当前页。切换会话不会撤销已开始的本地导出。不宣称旧版本能导入 v2 JSON。
5. 验证旧 v1 文件不变、已归档与活动回合快照、日志复制/缺失、Unicode/围栏、路径/symlink、重复导出不覆盖及 UI/IPC。补可操作的回滚指南：停止回合、导出并备份整个目录及 v2 存储，再使用旧版；新旧分支历史不自动合并。

实施结果：JSON/Markdown/manifest/日志副本的目录导出、UI 入口及离线回滚样本已完成。`pnpm check` 前端 169/Rust 281（5 ignored）/集成 2、默认 `--no-bundle` 构建通过；离线样本生成器显式运行通过，已校验并保留输出。另补 staging 清理断言后导出回归 4 passed。见 [导出验收](../../dogfood/2026-09-20-chat-export.md) 与 [回滚指南](../../guides/chat-export-rollback.md)。浏览器组件检查不替代最终原生桌面导出/实际降级再升级门禁；继续应用退出、M3/M5 和待授权真实矩阵。

## 20. M2.4 正常退出与活动操作收口（2026-09-20）

1. 在 Tauri 2.11.1 的 ExitRequested 与主窗口关闭入口建立一次性退出流程：暂缓退出，停止接收新的执行操作，等待已有操作完成停止/持久化，再允许真正退出。退出失败保留应用并显示原因，可再次请求退出；不悄悄强退。
2. ProcessSupervisor 增加跨启动/注册/收口的 operation guard，覆盖 Chat、Task CLI/command/review/PTY，以及 Chat 导出。退出期间已获准但尚未注册 PID 的操作仍计入等待；新操作拒绝。guard 在异常返回/取消时清理自己仍注册的进程组，不按历史 PID 操作。
3. Chat 起始 lease 即可记录 app_shutdown 取消原因；runtime 沿既有 TERM→KILL/排流路径收口。退出驱动保留相关仓储与会话引用，确认回合不再活动且磁盘投影可读；终态保存失败时保留已提交 partial 并尝试明确恢复为 interrupted，无法保存则拒绝退出。
4. 统一停止当前应用注册的进程，宽限期后按最新注册状态升级 KILL，有界等待进程/操作和 Chat 持久化。主窗口保留到收口结束，关闭独立计划查看窗口不触发全应用退出。无阻塞任务时快速退出。
5. 使用隔离测试子进程验证全局关闭门闩，包含无 PID 起步、真实拒绝 TERM 的子进程、多会话/项目、退出期间新请求、导出/操作持有、保存失败重试、正常终态与 Task 兼容。不得停止用户当前 Loom 实例或调用待授权外部模型。SIGKILL/系统崩溃无法运行退出回调，异常孤儿与最终原生 UI 退出仍需单独验收，不以本轮测试冒充完成。
