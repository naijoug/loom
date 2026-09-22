# Chat 仓储与活动回合工程验证 — 2026-09-20

- 验证完成时间：2026-09-20 10:56，Asia/Shanghai。
- 基线：`0fe5f79` + 本轮未提交工作区，非独立发布 commit。
- 平台：当前 macOS；Rust 1.95.0；不包含其他操作系统或 Rust 1.89 最低版本实测。
- 范围：[重构计划](../plans/2026-09-17/10:15-local-agent-chat-rebuild.md) M1 仓储基础切片；v1 格式保留，v2 journal 与迁移未完成。
- 发布判断：**未达到 Release 条件**。本记录为工程回归，不是实际 Agent、桌面交互、DMG 安装或公证通过证明。

## 修改与证据

生产调用经 `chat/repository.rs` 统一读写，`chat/service.rs` 在完成保存后才返回终态事件；`chat/models.rs` 保持现有 IPC 模型。关键测试直接调用这些生产模块，没有复制实现。

| 场景 | 结果 |
|---|---|
| get/list 读取活动回合，包括进程尚未启动阶段 | 保持 streaming，不被误标记为重启中断；重复 begin 拒绝 |
| 遗留回合恢复 | 保留已保存 partial，转 aborted，恢复幂等，能够再发送 |
| 同 sessionId 不同项目、旧 turn 的取消 | 只有相符项目/session/turn 被取消；启动前取消可保留到启动边界 |
| 改标题与最终保存并发、旧回合结束 | 标题不被覆盖；旧 finish/lease 释放不影响新 turn |
| 活动配置修改、归档与发送 | 生产 guard 拒绝活动配置修改；归档后必须恢复才能发送 |
| 并发 12 次创建与 12 次 mutation | 会话和修改全部保留，index 不丢项 |
| index 损坏 | 完整 session 文件仍可发现，后续 create 重建缓存 |
| 非法 ID、静态 symlink、伪造 cwd/id、未来 schema | 拒绝读取/修改，原始文件及 symlink 目标未被覆盖 |
| OS writer 锁及继承描述符 | 有持有者时排他，最后仓储持有者显式解锁；不删除锁文件规避占用 |
| 完成通知与保存 | 事件返回时快照已可读；保存失败产生 storage_error，不产生成功完成 |
| 错误与续聊句柄 | 已保存 partial 和旧 resume 保留，错误可读 |
| 工具 parts 序列化 | 输出前端 camelCase，兼容读取旧 snake_case 字段 |
| 不同 IdGenerator 实例 | 同一进程共享计数器，实际生产函数 1024 次生成无重复 ID |

## 执行结果

1. `CARGO_NET_OFFLINE=true pnpm check`：最终退出码 0。
   - 前端 143/143 通过；build 通过。
   - Rust fmt、所有 target/feature 的 Clippy 通过。
   - Rust 单元 237 通过、2 忽略；workflow integration 2/2 通过。
   - release 文档检查与检查器回归通过；不等同于发布 gate 已通过。
2. 最终代码的 `CARGO_NET_OFFLINE=true pnpm tauri build --no-bundle`：退出码 0，生成 `src-tauri/target/release/loom`，未生成/签名/公证新 DMG。
3. 文档相对链接、计划索引顺序及 `git diff --check` 通过。

本机原始日志：`/tmp/loom-chat-repository-check.log`、`/tmp/loom-chat-repository-build.log`。日志可能由后续运行替换，持久证据以本记录的基线、命令和结果为限。

## 本轮发现并处理的失败

- 初次并行门禁中，writer 文件仅关闭描述符，继承的描述符可短暂保留锁；改为最后一个仓储持有者显式 unlock，并以保留复制描述符的生产锁测试覆盖。
- 后续一次并行测试中，旧 Task 取消测试报进程组停止 `Operation not permitted`，单独运行通过。进一步回归独立 IdGenerator 实例，**修复前实际复现重复 run ID**；共享监督器可被相同 key 覆盖，因此将计数器改为进程内共享。重复 ID 测试及最终并行完整门禁通过。不把该问题与所有 OS 权限失败等同。
- 重复 ID 的首次失败日志：`/tmp/loom-id-collision-regression.log`；保留失败来源，不仅记录最后一次通过。

## 仍未验证或未完成

- 没有开启预览或调用真实模型；没有本轮真实 Tauri dispatch/交互验收。
- 当前仍是 v1 结束时持久化，未保证中途未落盘流可恢复；v2 journal、迁移和序号补流继续实施。
- Chat runtime 的 stdout/stderr 排流、非零退出保留正文后的状态、强制停止回收、独立 ProcessOwner 仍需完成。
- 静态路径校验不等同于内核级抗并发目录替换；硬断电持久性不在本轮证据内。
- 新候选 artifact 的签名、公证、Gatekeeper、安装/卸载及真实 CLI 权限矩阵没有本轮通过证据，继续保留发布门禁。
