# Chat 可靠性测试地图（2026-09-24）

本记录把近期围绕单 Agent Chat 的桥接、投影、发送与 UI 交互测试串成一张可接力地图。它不是发布结论：自动化单测只能证明本地契约边界，真实 CLI、权限、停止、重启和分发仍需按 `docs/testing.md` 的验收路径执行。

## 当前自动化覆盖

| 契约边界 | 已有证据 | 说明 |
| --- | --- | --- |
| 监听先建好再开放发送；加载快照期间缓冲事件 | `tests/unit/chatBridge.test.cjs`：`production bridge waits for listeners and buffers events before get resolves` | 锁定 early event 不被 stale snapshot 擦掉，保护 `useChatBridge` 启动顺序。 |
| 切换会话后旧加载、旧错误和旧补流不得污染当前会话 | `tests/unit/chatBridge.test.cjs`：`production bridge ignores late A loads/errors after selecting B and replays sequence gaps`、`production bridge suppresses recovery failures after switching sessions` | 覆盖 session token / active selection 边界。 |
| 当前会话的恢复失败必须可见 | `tests/unit/chatBridge.test.cjs`：`production bridge reports recovery failures for the current session` | 与旧会话失败抑制成对，避免把真实当前失败吞掉。 |
| 监听初始化失败不得误标 ready | `tests/unit/chatBridge.test.cjs`：`production bridge reports listener initialization failures without marking the listener ready` | 失败时 `listenReady=false`、`session=null`，Composer 不应提前可发送。 |
| 补流分页使用更新后的 cursor，并在页数 cap 后回退 fresh snapshot | `tests/unit/chatBridge.test.cjs`：`production bridge drains paged recovery using the updated cursor`、`production bridge falls back to a fresh snapshot after the recovery page cap` | 覆盖漏事件恢复与长尾分页保护。 |
| 投影乱序、重复、缺口和超大 pending 队列 | `tests/unit/chatJournalProjection.test.cjs`：`out-of-order and duplicate durable events are applied exactly once`、`large sequence gaps are capped and wait for an authoritative snapshot` | 防止在非权威状态上无限缓存或错误推进 `lastSeq`。 |
| patch 应原子应用；失败时保留最后安全态并等待权威快照 | `tests/unit/chatJournalProjection.test.cjs`：`failed patches keep the last safe session until a fresh snapshot arrives` | 覆盖字段校验、消息引用缺失和恢复路径。 |
| 草稿隔离与迟到回执清理 | `tests/unit/chatJournalProjection.test.cjs`：`drafts remain isolated and survive switching away from a project`、`a late send acknowledgment clears only the submitted draft in its own project` | 保护项目/会话草稿不被旧发送回执误删。 |
| 发送幂等与不确定 IPC 失败重试 | `tests/unit/chatSendController.test.cjs` | 确保同内容 pending 合并、不确定失败复用 `clientRequestId`、成功后新请求换 id。 |
| Composer 与 Header 的人工交互护栏 | `tests/unit/chatBridge.test.cjs`：Composer Ask / IME / listener readiness、Header rename / resume label | 覆盖发送、授权、重命名和结构化 resume 的前端交互边界。 |

## 仍需真实验收的缺口

- 真实 CLI：Codex、Claude、Grok 在本机登录、权限、stdin/argv、原生 resume 与无 resume fallback 的行为差异；不能用 fixture 单测替代。
- 停止与退出：运行中停止、应用退出、子进程持管道、TERM 到 KILL 升级和日志落盘，需要桌面或进程级 smoke。
- 权限语义：Ask/Auto 在 UI 和后端的边界仍需真实只读/可写项目验证；Ask 目前是每回合授权，不是逐工具审批。
- 长历史与保留策略：长会话冷启动、日志保留/清理、超预算提示仍是契约里明确的未完项。
- 发布候选：签名、公证、安装、卸载和回退必须针对同一个候选 artifact 复核，历史 dogfood 不能复用为发布通过。

## 下一段建议

1. 近期建议中的三个自动化小边界已分别落地：导出 warning UI、停止按钮 pending 状态、日志分页 UTF-8 cursor。后续若继续写单测，优先从未覆盖的产品层面选题，例如长历史预算提示、保留/清理策略或真实 CLI 失败态，不要继续在 `ChatProjection` 上堆近重复用例。
2. 若有可安全修改的临时项目和已登录 CLI，按 `docs/testing.md` 的“当前 Chat 验收路径”跑一条真实桌面记录；记录骨架使用 `docs/dogfood/chat-desktop-acceptance-template.md`，证据写到新的 `docs/dogfood/YYYY-MM-DD-chat-desktop-acceptance.md`。
3. 若准备发布候选，先冻结 commit，再按 `docs/release/` 的 release 证据路径验证，不要把本文件或真实桌面模板当成 release gate。
