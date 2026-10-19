# M1 桌面接线与原生验收记录

- 日期：2026-09-30，Asia/Shanghai。
- 状态：本文件只记录 2026-09-30 的桌面数据链路切片；当时只读迁移尚未完成。此后的 macOS 导入见 [导入记录](m1-legacy-import-progress.md)。全局状态以 [计划第 24 节](../../plans/2026-09-30/13:17-craft-parity-rebuild.md) 为准，完整复刻仍未完成。
- 基线：`d0b8337` 后续工作区；来源与产物 hash 见 [证据清单](m1-desktop-evidence.json)。
- 实现边界：[桌面契约](../../architecture/workbench-desktop.md)。

## 自动检查

- 完整 `pnpm check`：前端 194、Tauri Rust 295（5 ignored）、原集成 2、新 core 27 通过。
- 随后增加初始订阅握手断开恢复案例：相关前端 14 项通过，完整前端最终 195 项通过。
- 修复 macOS Quit 菜单后，Tauri Clippy `-D warnings`、契约/退出相关测试以及当前 debug app 包构建通过。
- 前端投影的预期结果由真实 Rust core 例程生成，覆盖文本/thinking、工具、笔记、状态、取消、已读和特殊键名；不是另一份手写 reducer 作为答案。

## 原生桌面实查

使用 `scripts/debug.sh` 启动已打包 Loom，数据在独立 `.tmp/workbench-desktop-2026-09-30`，未接入模型账户。

1. 自动创建默认工作区；创建会话后实际 core 写入磁盘。
2. 中文输入后看到实际 Channel 流式增量，界面明确标记 fixture。
3. 在生成中停止，保留部分文本并显示“已停止”。[AX 与截图](../../../designs/workbench-m1/01-stream-and-stop.ax.txt)。
4. 重命名、标记、保存笔记、归档，转录不丢失。[归档记录](../../../designs/workbench-m1/02-archive-notes.ax.txt)。
5. 创建第二个工作区，其列表独立；重启后切回原工作区，名称、旗标、笔记、归档和停止状态正确恢复。[重启记录](../../../designs/workbench-m1/03-restart-restored.ax.txt)。
6. 在后台回合运行时切到新会话，没有串入输出；结束后列表更新并出现未读，当前草稿保留。[后台记录](../../../designs/workbench-m1/04-background-unread.ax.txt)。
7. 切回后台会话时未读消失，再回到新会话草稿仍在；重命名对话框初始焦点在名称输入框。
8. 生成中 Cmd+Q 后退出，最终通过生产 core 回读确认新测试回合为 Cancelled 且没有 active owner。[回读记录](m1-desktop-readback.json)。

## 发现并修复

- 只有 Session 订阅时，后台完成虽已落盘，列表未及时刷新。补上 Workspace 级持久化后通知与合并刷新，原生复验出现正确未读。
- React StrictMode 清理后复用 controller 会使订阅永久失效；改为可重启、带代次的生命周期。补齐初始握手未完成就收到 resync 的处理。
- 迟到的发送确认可能清掉其他会话草稿；改为按 Session 的草稿/发送状态，并有组件级回归。
- 原生默认 Cmd+Q 未走退出钩子，旧测试回合在回读时变成 Interrupted。这项首次失败已保留；加入自定义菜单路由后新回合确认为 Cancelled。不会把 Interrupted 当作优雅退出通过。

## 验收范围限制

回复是显式 fixture，不证明真实模型、原生 SDK resume、工具权限或账户集成。截图是原生观察证据，未作完整 Craft 像素对齐。Dock/系统终止、Windows/Linux、完整草稿持久化和迁移仍未通过。测试应用与预览服务已清理。
