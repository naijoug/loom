# Chat 进展 Review 修复与验证（2026-09-22）

基线：`0fe5f79` 加已有用户工作区和本轮改动；没有把未提交工作冒充独立发布 commit。当前修复入口为 [实施计划](../plans/2026-09-22/14:29-chat-progress-review-fixes.md)。

## 已完成修复

- Chat 使用独立 `ChatShell`，保留项目导航和高级入口，隐藏任务阶段条与任务列表。`WorkflowShell` 只在高级路由挂载，负责 Task 加载与唯一 Planning 订阅。`useAgentCatalog` 供 Chat、Settings 和仅需要配置/诊断的组件使用，不附带 Planning 订阅。
- 活动回合补流在首次校验后建立内存字节索引，后续空轮询不重放历史，新增输出只读取所需记录。元数据变更清索引；文件/快照元数据变化触发完整校验，恢复与非活动会话仍按权威日志重建。磁盘格式和 IPC 不变。
- 修正 `docs/testing.md` 的“独立运行日志尚未完成”，修正架构文档中旧 v1 与假 taskId 描述，并给历史计划的状态/证据增加现行入口。
- 原生验证发现并修复调试脚本两个问题：macOS Bash 3.2 在 `set -u` 下展开空数组失败；宽泛 `tauri` + `dev` 子串匹配会误停带有 `cfg(dev)` 的 Rust 编译进程。现在匹配可执行文件/参数边界，已退出 PID 的清理也返回成功。

## 工程结果与可复核证据

- 最终 `CARGO_NET_OFFLINE=true pnpm check`：退出码 0；前端 **172/172**、Rust 单元 **289 passed / 5 ignored**、工作流集成 **2/2**，包括 build/fmt/Clippy/发布文档检查。[完整输出](samples/chat-review-2026-09-22/check.txt)
- `CARGO_NET_OFFLINE=true pnpm tauri build --no-bundle` 通过；随后默认配置 `--bundles app` 通过，产物为 `src-tauri/target/release/bundle/macos/Loom.app`。[应用构建输出](samples/chat-review-2026-09-22/app-build.txt)
- 长历史生产仓储回归：10,002 条事件，索引建立后连续 100 次空补流解析历史记录 **0** 条；追加一条后读取 **1** 条，历史分页读取请求的 **3** 条。该次读取阶段耗时 2ms，仅为本机测试观察，不作为硬性能 SLA；覆盖的是已建立索引的活动回合，不代表冷启动/大量会话列表性能。[输出](samples/chat-review-2026-09-22/long-history.txt)
- 新回归还覆盖同长度文件替换后的坏记录、截断、未来快照版本、元数据更新后分页与活动 lease 释放后的中断恢复；保留已有大记录分页预算、越界游标和旧数据恢复测试。
- 实际 App 路由组件测试覆盖 Chat→高级工作流→Chat/Settings→高级工作流：Chat/Settings 不调用 list_tasks、不订阅 Planning；高级工作流只有一组订阅，离开清理、返回重新读取持久化任务。[测试代码](../../tests/unit/chatShell.test.cjs)
- macOS `/bin/bash` 下清理脚本的空进程、消失 PID、真实 dev 命令和 rustc/build 负例通过；`bash -n scripts/debug.sh` 与最终 `scripts/debug.sh stop` 成功。[输出](samples/chat-review-2026-09-22/debug-script.txt)

[源码及证据 SHA-256 清单](samples/chat-review-2026-09-22/source-sha256.json) 绑定 244 个源码、配置、依赖锁、脚本和测试文件；后续代码变化必须重新验证，不沿用本次结果。清单自身 SHA-256：`8adfbf6b33f3c91efaca9cbacee9cafd8cc5838ee6ab00dc14d49366e9206b6c`。

原生验收使用的 app 主二进制 SHA-256：`0aa1437dd5005c550f1d05b3efc86cf5c5ef04d931c939ed7e0dde858751df56`。该值仅标识本机构建，不代表签名、公证或分发通过。

## 原生桌面观察

通过 CUA 操作上述新构建的默认 `Loom.app`，页面为 `tauri://localhost`；不是浏览器 Mock。已有项目和旧 Task 可读。本轮没有发送任何模型请求。

1. 默认 Chat 无旧任务阶段条/任务操作；进入高级工作流后显示真实任务，返回 Chat 恢复对话导航。
2. 原生目录选择器登记空项目 `/private/tmp/loom-review-desktop.XDDFhd`，出现高级入口，无需存在 Task。
3. 新建空会话 `chat-1790059338655-2`，通过原生会话菜单导出；UI 显示快照序号 1 与实际目录。磁盘 JSON/manifest 均可读，`inProgress=false`、无 warnings、无虚构消息/运行日志。
4. 导出目录：`.loom/chat/exports/export-1790059352157-3/`。本轮只证明空会话的原生导出入口与 IPC；运行日志副本的工程验证见既有测试，不扩写为真实模型验收。
5. 正常退出并重启当前 app，测试会话仍可选择和读取。此时无活动回合，不能代替运行中退出或强制退出验收。
6. 验收后从侧栏移除临时项目（文件保留），恢复原项目列表，关闭本轮启动的 app，并完成调试进程清理。截图观察留在本任务工具输出中，未虚构独立截图文件。

第一次按 debug bundle 路径定位时打开了旧包，确认旧 UI 后立即退出，未计为本轮通过。第一次新 app 打包被调试清理脚本误停，明确记为失败；修复脚本后最终重建成功并使用其产物完成上述验收。中间前端测试还补齐了测试环境的 ThemeProvider/localStorage，最终生产路由断言通过。

## 未关闭项

- 真实 Grok 首轮/resume 文件权限、工具、停止后再发，仍未重跑。已向用户明确请求本轮合成目录测试及可能向 Grok 服务端发送主目录私人 skills 的授权，尚未收到答复；此前自动审批拒绝来源见 [09-20 记录](2026-09-20-chat-resume.md#真实-grok-结果与限制)。本轮只运行 CLI help，不以提示词、改 HOME 或权限绕过处理此边界。
- 原生桌面运行中退出、强制退出/孤儿进程、长历史冷启动/保留策略，以及同候选签名、公证、安装/卸载/真实回滚门禁仍未完成。
- 用户已有整批未提交改动保留原状，没有自动替用户提交。源码校验清单使本次结果可核对，但不能替代后续整理提交和发布候选。
