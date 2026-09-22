# Chat 回合与执行日志验收（2026-09-20）

基线：`0fe5f79` 上当前未提交工作树，接续发送幂等和结构化续聊。对应 [主计划 §18](../plans/2026-09-17/10:15-local-agent-chat-rebuild.md#18-m13m23-回合记录与日志2026-09-20)。本轮没有请求外部模型，没有重启用户正在使用的 Loom.app。

## 交付内容

- ChatTurn 随接收回执/两条消息原子保存：请求/消息关联、starting 状态、不可修改的脱敏调用配置。真实 spawn 后记录 running、时间/PID；取消记录 cancelling；完成区分 completed/failed/cancelled/timed_out，重启遗留为 interrupted。
- 保存退出码、结束时间、原因和错误，无法取得退出码时为 null；终态/配置不可覆盖。旧会话不伪造运行历史，PID 不作为重启杀进程依据。
- 生产 `service::run_chat_turn` 同时驱动 runtime、journal 和独立 stdout/stderr。日志创建/写入失败会停止回合；单回合两份日志总量 8 MiB，脱敏后写盘，Unix 日志权限 0600。argv 中正文显示 `[PROMPT]`，不重复保存完整 prompt/历史；secret/token/API-key 等已知凭据形式遮蔽。
- `chat_read_run_logs` 仅按已登记项目/会话/回合读取派生日志路径；不接受任意路径。4–65536 bytes 分页，默认 32768；保留 UTF-8 边界，拒绝越界游标、错回合和符号链接。
- 转录中新增执行记录面板，展示状态/耗时、配置/时间/退出原因及 stdout/stderr；支持分页、重新加载和读取错误。迟到响应不会覆盖新的流或会话，HTML 日志按文本显示。`chat_abort` 现在必须指定 turnId。

## 最终验证

`CARGO_NET_OFFLINE=true pnpm check` **通过**：前端 **166**、Rust **277**（另 4 ignored）、工作流集成 **2**，包括 fmt/Clippy/build/发布文档检查。日志 `/tmp/loom-turns-complete-check.log`。

`CARGO_NET_OFFLINE=true pnpm tauri build --no-bundle` **通过**，仍用默认 Loom 名称配置。release 二进制 SHA-256：`fd19690d2a80864b8e43732166246eb70c91278efc8d2c9a77b673434aefd533`。日志 `/tmp/loom-turns-build.log`。没有替换用户正在使用的桌面 app 包，不把旧 app 包称为本轮候选。

生产 service + 真实本地 shell fixture 覆盖：

| 场景 | 结果 |
|---|---|
| stdout/stderr、中文、凭据标记、exit 7 | 分流正确，秘密值未写日志，记录 failed/exitCode=7，partial 保留 |
| 正常 exit 0、不存在的程序 | 分别 completed/0、failed/null；未启动的回合 startedAtMs 为 null |
| 启动前取消、运行中取消、超时 | cancelling/取消状态与时间一致；timeout 保存 timed_out 及超时原因；子进程被回收 |
| 权威结果未提交即释放仓储，再重开 | interrupted，日志/partial 可读，重复恢复不新增事件，不虚构退出码 |
| 日志观察器返回磁盘写入错误 | runtime 停止进程、释放 supervisor 所有权并报告 error |
| 元数据改名、配置/终态改写 | 改名保留；改写历史调用配置或退出码被拒绝 |
| 分页、错回合、游标、symlink、日志预算 | Unicode 多页拼接不损坏；错误边界拒绝；超预算不追加、不覆盖已有日志 |

首次全量并行检查发现新测试夹具使用固定 `turn` ID，触发 supervisor 跨测试碰撞。夹具已改用生产 IdGenerator，最终全量通过；未修改生产唯一 ID 或放松 supervisor 检查。

前端生产组件与 wrapper 覆盖实际日志 IPC 参数、分页游标、stdout/stderr 切换、迟到请求隔离、错误可见、日志 HTML 不执行。共享 ChatTurn/ChatLogPage JSON 样本经 TS/Rust 校验。

## 视觉与交互

通过 `scripts/debug.sh web` 在固定端口 1420 查看 [视觉样例](../../designs/chat-run-log-preview.html)，使用生产 ChatRunDetails 与明确标注的合成数据，**不是桌面 IPC 或真实 CLI 验收**。CUA 实际展开面板、加载更多、切换 stderr，核对正常和 360px 内容区的换行/按钮与日志。截图观察保留在本任务工具输出，没有宣称已生成独立截图文件。

预览首次因沙箱禁止监听端口而启动失败；获准启动本地服务后，将预览绑定到活动工具进程完成检查。结束时已关闭预览页并执行 `scripts/debug.sh stop`；未操作用户的 Loom 窗口。

## 边界与待办

本轮真实 Grok 重跑仍未获明确授权：此前自动审批因私人 skill 可能外发而拒绝，未绕过。已把 opt-in canary 接到同一生产回合/日志 service，编译检查通过，**没有执行该外部调用**。

仍需真实权限/停止矩阵、运行中完整应用退出、导出/回滚、长历史/保留策略、Chat Shell 解耦及发布签名/公证/安装验收。日志脱敏覆盖已知形式，不等于任意秘密检测；flush 不等于硬断电保障，静态路径检查也不等于系统级竞态防护。
