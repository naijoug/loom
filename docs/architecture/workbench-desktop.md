# Workbench 桌面接线（M1）

当前实现把独立 `loom-core` 接到 Tauri 主窗口和 React 新 Shell，包含旧项目历史的只读预检、导入和阅读。它是 M1 数据/交互骨架，不代表 Craft 完整界面、真实模型、全部迁移边界或 M2–M8 完成。

## 入口与数据隔离

- `src-tauri/src/workbench/`：惰性创建新 core，默认写入应用数据目录的 `workbench/v1`，不使用旧项目 `.loom/`。IO/请求处理放在后台线程。
- `src/workbench/WorkbenchApp.tsx`：独立工作区、项目登记、会话列表/搜索、状态/旗标/归档、笔记、Composer 和转录。
- `src/api/workbenchClient.ts`：请求归属/类型检查与 Channel；底层 Tauri API 仍只从 `tauriClient.ts` 进入。
- `src/workbench/projection.ts` / `sessionController.ts`：按 Session 的只读投影、乱序缓冲、seq 去重、补流和生命周期。

新入口由 `VITE_LOOM_WORKBENCH=1` 选择。默认构建仍使用旧入口，M8 完整验收后才切换。新 UI 不挂载旧 AppState/WorkflowShell/Planning 订阅。

```sh
scripts/debug.sh stop
LOOM_PREVIEW_TARGET=workbench scripts/debug.sh desktop
scripts/debug.sh stop
```

## 桌面契约

| Command | 作用 |
| --- | --- |
| workbench_capabilities | 返回协议版本和 runtime 可用性 |
| workbench_invoke | 执行生成契约中的用户意图，返回带 requestId 的 CoreReply |
| workbench_save_connection | 可信主窗口保存连接；秘密先写系统 vault，响应仅返回普通配置和引用 |
| workbench_watch | Session snapshot + 专属 Channel，监听持久化后事件 |
| workbench_watch_workspace | Workspace 的轻量变更 Channel，用于列表失效刷新 |
| workbench_unwatch | 按调用窗口和 watchId 清理订阅 |
| workbench_resume_queue | 显式请求继续队列；当前只有 debug fixture 可执行 |

所有入口限制为可信 `main` WebviewWindow，计划查看器等窗口不能调用。订阅 ID 在窗口内去重，总量有界；窗口销毁、显式 unwatch、Channel 失效和退出会清理。core 的订阅采用 RAII/Weak owner，已释放的订阅不会保留 writer。

Session 事件携带完整 stream/seq；Workspace 事件只携带工作区/会话身份与序号，不携带对话内容。工作区通知覆盖创建、接收、开始、结束、元数据、已读与项目登记，不逐字推送流式列表更新。列表刷新合并并行请求，防止迟到结果覆盖；定时读取只作兜底。

前端先建立 Channel 再接收 snapshot，早到事件缓冲；迟到旧 snapshot 不覆盖新状态。缓冲超限或服务端断开要求补流/重新订阅，初始 snapshot 返回前发生断开也不会留下假活跃订阅。React StrictMode 的停止/重挂载和旧回调都有代次保护。

## fixture 与真实执行的界线

仅 debug 构建且显式 `LOOM_WORKBENCH_FIXTURE=1` 时启用本地 fixture。它通过实际 core Lease、事件、存储和终态运行，回复固定测试文本；界面顶栏与 Composer 明示“测试回复”。普通构建不允许环境变量启用该模式。

没有真实 runtime 时发送控件不可用，不伪造模型回复。M2 仍须接入 SDK、vault、权限、真实工具、进程监督和审计。当前 UI 的权限选择只保存新模型的请求配置，不能证明真实工具权限已经执行。

工具审批面板通过生产 core 的事件投影展示脱敏参数与单次允许/拒绝。request ID 与 binding digest 来自展示的审批记录，切换会话后的迟到回应不会修改新会话。取消/过期/消费状态通过同一 journal 补流恢复。

仅在上述 debug fixture 同时设置 `LOOM_WORKBENCH_APPROVAL_FIXTURE=1` 时，Ask/Guarded 测试回合会生成一个明确标为“FixtureApproval · 无副作用测试”的请求。该工具不执行文件或命令，仅验证真实 core 决策、UI 与退出路径；普通发布构建无法启用。真实 SDK 的单写工具验收由独立 `sdk_core_approval_probe` 运行，不能混称桌面已接入生产模型。

## 退出

工作区先关闭执行入口并停止订阅，等待当前回合保存终态；未领取的 queued 回合不会因退出而执行。其后才由旧退出门处理旧运行时。中断恢复仍依赖日志，不能通过历史 PID 误杀进程。

本机发现默认 macOS Quit 绕过 ExitRequested，复现与 [Tauri 上游问题](https://github.com/tauri-apps/tauri/issues/9198) 一致。`desktop_menu.rs` 保留默认菜单并用相同文本的自定义 Cmd+Q 项调用 AppHandle::exit；原生复验与 core 回读证明活动 fixture 回合保存为 Cancelled。此前失败的 Interrupted 记录保留，没有改写成成功。

Dock Quit、系统登出/终止、强杀、真实 SDK 和所有平台的退出矩阵仍是后续验收，不能用这次 Cmd+Q 结果代替。

## 原生验证方式

```sh
VITE_LOOM_WORKBENCH=1 pnpm tauri build --debug --bundles app
LOOM_PREVIEW_TARGET=workbench-bundle \
LOOM_WORKBENCH_FIXTURE=1 \
LOOM_WORKBENCH_DATA_DIR="$PWD/.tmp/workbench-desktop-2026-09-30" \
scripts/debug.sh desktop
scripts/debug.sh stop
```

`LOOM_WORKBENCH_DATA_DIR` 只在 debug 构建允许覆盖，必须为绝对路径。`workbench-bundle` 是 macOS 本地验收入口，要求已构建 app；它不启动另一个端口。开发服务器入口仍固定 1420。

旧数据操作位于选中项目后的“检查项目旧数据”。预检不写源目录，执行导入才创建新 Session；结果区可打开历史记录。Chat 的历史消息和工具摘要独立读取，Task 进入归档，文件通过校验后的资源 API 提供文本预览。当前未实现草稿跨应用重启持久化、完整 Markdown/附件/模型连接 UI、来源/技能/Pages/自动化导航、多窗口、完整主题/快捷键及跨平台完整迁移。这些继续按 M1–M8 完成，不将本骨架当作最终产品。

## 连接与凭据配置切片

侧栏“模型连接”可新增/编辑配置，Composer 可选择连接和模型；发送后连接身份锁定。表单明确显示“已保存凭据/未验证”，正常 SDK 宿主尚未接入时发送仍不可用，debug fixture 仍只生成测试回复。中途行为、图像能力等是保存的配置，执行效果需要后续 SDK/内容管线接线。

macOS 使用 `security-framework` 调用系统 Keychain；service namespace 绑定新数据目录，account 使用连接/保存操作生成的不透明 ID。秘密不进入普通 catalog、journal、返回 DTO 或命令行参数，宿主处理缓冲使用 zeroize。列表读取不触发密钥读取，也不把“有引用”称为认证成功。当前其它平台明确返回 vault 不可用，需要认证的保存不退回明文文件。

每次替换凭据使用新的不可变 slot，已接受的回合保留旧引用；修改提供方/端点/认证方式时必须重新输入凭据，不能直接复用隐藏的旧值。vault 失败不发布新配置；catalog 不确定提交时保留 slot 供相同操作重试。旧 slot 和孤立 slot 的安全回收、删除连接、跨平台 vault 与 OAuth/云凭据仍待完成，不能据此宣称完整凭据生命周期已交付。
