# M1 独立内核首个切片

- 状态：内核切片已实现并验证，**M1 尚未完成，整体重建仍在实施**。
- 基线：`6b37769` 后续工作区；精确源码 hash、平台和命令见 [验证记录](m1-core-evidence.json)。
- [当前内核契约](../../architecture/workbench-core.md)。

## 已落地

- 根 Rust workspace 和独立 `loom-core`；旧 `src-tauri`/实验项目保持独立，未切换默认桌面入口。
- Workspace/可选 Project、Session、Message、Turn、ToolCall、权限/模型快照和运行 Lease。
- Catalog/Session journal、校验链、提交 watermark、单 writer、缓存/快照、尾部保留修复、崩溃中断恢复。
- 发送和运行帧幂等、FIFO 排队、单活动回合、取消/完成约束、会话隔离、读标记、元数据并发和补流。
- `loom-core-host` 本地 JSONL 入口，协议错误关联 requestId；运行帧无用户可伪造的 wire command。
- Rust/serde 生成 TS 协议，编译示例、漂移检查与 `pnpm check:core`；新检查接入完整 `pnpm check`。

## 验证

完整 `pnpm check` 已通过：前端 184、旧 Rust 290（另有 5 项 ignored）、旧集成 2、新内核当时 24。随后只调整新内核的长笔记/缓存处理并增加相应回归，最终 `check:core` **26 passed**；fmt、Clippy `-D warnings`、类型生成漂移、TS 编译均通过。最终变更范围和两个日志的覆盖区别保存在 JSON 记录中。

关键场景经过真实文件/进程/传输入口：

- 12 线程同时发送同一请求，只创建一个 receipt；重开仍幂等，内容冲突被拒。
- 队列保留接受时的权限/模型；另一个回合、会话或 Core 实例不能借用 Lease 写入。
- 真实子进程持锁运行后 `process::exit`，重开可获得 writer 并记录 Interrupted，不重新执行。
- 日志写入失败不发布事件；journal 已写但 watermark 写失败的未知提交可恢复原 receipt。
- 不完整尾行先保留再修复；完整损坏和已提交日志被截短会拒绝载入。
- 派生缓存丢失可恢复；symlink 不会重定向数据；项目目录被换成 symlink 后不能以旧路径领取执行。
- 慢订阅者断开后可以从 seq 补流；跨会话不串事件。
- 流式事件不与安全的标题编辑冲突；旧元数据写入被拒；已读游标不后退。
- 64 KiB notes 不进入列表行；超过缓存容量的会话仍可读，活动 Lease 保持有效。

## 接下来

1. 接上新的 Tauri command/event pump 和按会话前端投影，形成实际可操作的新骨架。
2. 实现旧 Chat/Task 的只读导入器、dry-run/幂等/附件缺失报告与回退验证。
3. 将 M0 SDK/Browser 技术证据推进为生产适配器与资源服务，继续完整功能矩阵。

当前没有真实模型执行、新桌面交互或完整迁移完成证明；不能把内核绿色测试当作最终验收。
