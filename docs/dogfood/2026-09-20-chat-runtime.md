# Chat Runtime 与真实 Grok Canary — 2026-09-20

- 基线：`0fe5f79` + 本轮工作区；承接 [仓储切片](2026-09-20-chat-repository.md)，尚未形成独立发布 commit。
- 平台：当前 macOS，Rust 1.95.0，Grok 1.0.30（04b7ffed98c6）。没有 Windows/Linux 或其他真实 CLI 验收。
- 范围：生产 `chat/runtime.rs`、`ProcessOwner`、输入传输、输出/取消/超时/回收，及 Grok 三轮真实无工具文本对话。
- 发布结论：**尚不可宣称 Release 就绪**；仍缺 v2 journal/迁移、真实权限与工具/停止、Tauri 桌面和同一候选发布门禁。

## 生产行为与 Fixture 验证

`tests/fixtures/chat/fake-agent.sh` 不调用模型或网络；测试通过生产 runtime 启动真实 `/bin/sh` 子进程，并检查退出、实际输出和进程消失。

| 场景 | 可复核结果 |
|---|---|
| stderr 写满后才输出 stdout | 并发排流，最终回答完整，无死锁 |
| 已输出正文后 exit 7 | status=error，partial 保留，诊断可读，不接纳新 resume |
| 忽略 TERM 的进程：取消/超时 | 升级 KILL、回收并释放监督器记录；取消与超时诊断不同 |
| 父进程退出、子进程继续持有管道 | 连续 20 次通过；有限等待后回收同组子进程，验证 PID 最终不存在 |
| 子进程先关闭输出管道但仍存活 | 成功回合收口也清理同组进程，不遗留后台进程 |
| 大 stdin、写完关闭 stdin | 输入写入与两路读取并发，无等待 EOF 死锁 |
| UTF-8 跨 read 分片 | 中文字符正确还原 |
| 未知 JSON 与 Codex agent_message | 未知事件不作为原始 JSON 正文；agent_message 文本可见 |
| 字节、单行、行数限额 | 超限失败并终止进程，不伪装成成功截断 |
| 流式/结束事件消费失败 | 返回 error 并保留已解析 partial；不泄漏进程 |
| spawn 失败、启动前取消、future 被取消 | 监督器清理、同组进程终止，无误用其他回合 |
| Chat owner 和重复注册 | Task 查询不含 Chat；重复 run id 不覆盖已有 metadata |

默认运行预算：10 分钟；TERM 宽限 1 秒，KILL 后回收等待 2 秒；管道尾部 drain 300ms。输出上限 8 MiB、单行 512 KiB、20,000 行、1,000 个工具片段；限额失败可读。测试缩短时间预算以验证相同生产逻辑，不代表真实 Agent 会在这些测试时限内响应。

## 真实 Grok 三轮 Canary

执行命令：

```bash
LOOM_REAL_CHAT=1 cargo test --offline --manifest-path src-tauri/Cargo.toml \
  chat::real_agent_tests::grok_three_turns_resume_after_repository_reopen \
  -- --exact --ignored --nocapture
```

显式 opt-in 的测试使用现有账号额度，只在空临时项目要求模型记住并返回随机标记，不请求工具、文件读取或修改。通过生产 adapter/context/runtime/repository，不复制核心实现。常规 `pnpm check` 与 `pnpm smoke:chat` 默认忽略该付费测试。

修正 Grok 输出格式参数后的实测：总用时 44.93 秒，退出码 0。

| 回合 | 结果 | 流式正文 | 原生 resume | 标记次数 |
|---|---|---|---|---|
| 1 | complete | 27 bytes | 已捕获并落盘 | 1，正文完全一致 |
| 2 | complete | 27 bytes | 从磁盘重开仓储后使用 | 1，正文完全一致 |
| 3 | complete | 27 bytes | 再次从磁盘重开仓储后使用 | 1，正文完全一致 |

后两轮断言 adapter 的 `resumed=true`，因此不是重放历史绕过原生续聊。每轮仓储实例释放后重开，但没有退出和重启 Tauri 应用；此证据不覆盖桌面 lifecycle。测试日志 `/tmp/loom-chat-grok-canary.log` 仅汇报状态/字节/句柄是否存在/标记次数，不将账号凭据或原始 CLI 配置写入本文。

真实 canary 通过后还修正了重复 KILL 和终态消费失败时保留 partial 的边界，随后使用 fixture 和完整检查复验；没有额外重复付费三轮。最终候选仍需按发布矩阵执行真实桌面验收。

## 发现与修复过程

1. 旧 runner 串行读取 stdout→wait→stderr，无法处理 stderr 背压；现为统一循环并发处理两路读取、stdin、退出与超时。
2. 旧 Grok adapter 传 `streaming-json`，实际 CLI 提示 `--include-partial-messages` 仅对 `streaming-messages-json` 生效；按 1.0.30 help 修正，并同步 adapter 参数回归。首次 canary 失败时 stderr 前部还被大量启动警告占据，现诊断保留有界尾部。
3. 子进程管道用例曾报进程组 `Operation not permitted`；最初在普通本机权限下复验通过，但完整并行运行又出现，不能仅归因于沙箱。删除已 KILL/回收后重复 KILL 的收口路径，增加同一场景 20 次重复；最后完整门禁在默认执行环境通过，没有忽略该测试。
4. 进程归属由伪 taskId 改为 `ProcessOwner::Chat { projectKey, sessionId, turnId }`，Task 调用使用兼容构造函数，原高级工作流回归保留。

## 工程门禁

- 最终 `CARGO_NET_OFFLINE=true pnpm check`：退出码 0；前端 143/143，Rust 单元 250 通过/3 忽略，workflow integration 2/2；包含 fmt、Clippy、前端 build 和 release 文档检查。
- 3 个默认忽略项包含本次显式运行通过的真实 Grok canary，另两个为原有忽略项；不把 ignored 项算入普通通过数量。
- 最终 `CARGO_NET_OFFLINE=true pnpm smoke:chat`：退出码 0，52 通过/1 忽略（默认不调用真实模型）。
- 最终 `CARGO_NET_OFFLINE=true pnpm tauri build --no-bundle`：退出码 0，生成 `src-tauri/target/release/loom`；未生成/签名/公证新 DMG。
- Shell 语法、文档链接、计划索引及 `git diff --check` 通过。
- 原始日志：`/tmp/loom-chat-runtime-check.log`、`/tmp/loom-chat-runtime-tests.log`、`/tmp/loom-chat-transport-tests.log`、`/tmp/loom-chat-smoke.log`、`/tmp/loom-chat-runtime-build.log`；本机日志可能被后续验证替换。

## 后续门禁

继续 v2 事件日志与迁移/补流，CLI 的实际文件/命令权限、工具事件、停止后再发、真实 Tauri 交互与退出恢复；签名、公证、Gatekeeper、安装/卸载必须针对同一新候选复核。本轮没有启动预览，也没有生成或发布新 DMG。
