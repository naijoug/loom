# Chat 发送幂等验收（2026-09-20）

基线：工作树基于 `0fe5f79`，包含本轮尚未提交的 repository/runtime/v2 journal 与发送回执修改。沿用 [当前计划](../plans/2026-09-17/10:15-local-agent-chat-rebuild.md) M1.3；本记录不表示整个 Chat 或发布验收完成。

## 行为与实现

- `chat_send` 必须携带 `clientRequestId`，请求正文保留原始空白。项目/会话内先查不可修改的 `sendReceipts`，再检查活动回合、归档、准备 CLI。
- 用户消息、助手占位与回执在同一 journal patch 提交。只有新接收请求拿到运行 lease；相同请求返回旧 turnId 和最新会话，不再准备/启动 Agent。正文或请求权限改变而标识不变则拒绝。
- 完成、停止、中断、归档、改 Agent/权限后，重复请求仍返回旧回合。启动通知失败不能把已接收请求丢在落盘与 spawn 之间；日志可补流。
- 前端合并同内容的并发发送；IPC 失败保留标识供手动重试，成功后下一次提交使用新标识。缓存跨 Chat 页面挂载，暂不跨 renderer 重载。发送已确认后即清理对应草稿，不因随后停止/刷新失败留下可重复发送的原文。
- 回执不是完整 ChatTurn 生命周期或 CLI 执行日志；这些仍属后续工作。

## 自动验证

`CARGO_NET_OFFLINE=true pnpm check` **通过**：前端 **163**，Rust **267**（另 3 个显式忽略的测试），工作流集成 **2**；包括 fmt、Clippy、前端 build 和发布文档检查。

新增 Rust 行为/契约覆盖：8 线程并发仅一次接收；完成/归档/配置变化/重开仓储后的去重；中断去重；同标识冲突；不同请求不能绕过活动锁；准备与权威日志写入失败无回执/lease；回执和原请求不可改写；子进程不运行析构直接退出后恢复回执；项目/会话隔离；标识长度/字符和必填约束。

前端测试调用生产 controller 与实际 IPC wrapper，覆盖回执丢失后标识不变、并发提交合并、成功后新提交分配新标识、项目/会话/内容/权限隔离。共享 JSON 样本由 Rust 和 TS 双向校验，包含发送输入与回执。

日志：`/tmp/loom-chat-idempotency-check.log`、`/tmp/loom-chat-idempotency-frontend.log`、`/tmp/loom-chat-idempotency-rust.log`。临时日志可被清理或覆盖，以本次命令结果为证据，不作为未来版本的通过证明。

## 真实 CLI

增强后的生产仓储/context/runtime canary：

```bash
CARGO_NET_OFFLINE=true LOOM_REAL_CHAT=1 cargo test --manifest-path src-tauri/Cargo.toml \
  chat::real_agent_tests::grok_three_turns_resume_after_repository_reopen \
  -- --exact --ignored --nocapture
```

受限环境首跑因 Grok 自有会话目录访问失败（`FS_PERMISSION_DENIED`）结束，未当作通过。经运行环境批准后访问 CLI 正常会话目录和网络，独立合成项目的三个无工具文本回合均 **complete**；每轮流式输出 27 bytes，随机标记恰好出现一次；第二、三轮恢复原生上下文。

每轮在运行前的活动状态重复请求，以及完成并释放仓储后重新打开再重复请求，都返回原 turn，未进入新 invocation 准备分支；完成重试前后 seq 相同。最终只有 3 条接收回执、6 条消息。此结论针对生产仓储/runtime 的 headless 验收，不冒充 Tauri 自动重试或真实工具权限验收。

命令退出 0，测试用时 44.09 秒。日志：`/tmp/loom-chat-idempotency-real-approved.log`；首跑失败日志单独保留于 `/tmp/loom-chat-idempotency-real.log`。

## Loom 桌面构建与观察

`CARGO_NET_OFFLINE=true pnpm tauri build --bundles app` **通过**，产物 `src-tauri/target/release/bundle/macos/Loom.app`。Info.plist 名称/显示名称均为 **Loom**，标识 `com.naijoug.loom`；没有使用 QA 名称覆盖。主二进制 SHA-256：`b40149ada3d92e48c20233a24b73c9c13f2ee401dbcb91b2616cafd404553e63`。构建日志：`/tmp/loom-chat-idempotency-build.log`。

CUA 按完整产物路径打开后，窗口名为 Loom，页面为 `tauri://localhost`。尝试切换至合成项目时检测到用户正在操作，停止自动切换并保留窗口。随后只读观察到新的 Codex 会话 `chat-1789884727385-1` 已有两条 complete 消息、idle 状态、seq=6，以及唯一的请求/turn/两条消息关联回执。**这是用户操作后的桌面与存储观察，不是本轮自动桌面重复请求验收，也不表示 Codex 权限已验收。**

## 剩余范围

仍需完整 ChatTurn/调用配置与日志、结构化 resume 和有效权限、导出/长历史、真实停止后再发、运行中应用退出，以及默认 Chat Shell 与发布候选门禁。用户正在使用的 Loom 窗口保持打开，未结束其会话。
