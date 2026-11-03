# 原生窗口启动与恢复

实现：`src-tauri/src/webview_startup.rs`、`src/bootstrap.ts`、`src/appStartup.ts`、`src/shell/AppShell.tsx`。这是启动可用性保护，不是会话/Agent 的重试机制。

## 启动顺序

1. `index.html` 提供静态启动状态，入口脚本尚未执行时也有内容。
2. 轻量 `bootstrap.ts` 先取得当前文档 generation，再动态加载 `main.tsx`。模块加载/初始化失败显示重试按钮，并报告固定的失败状态。
3. React 主外壳实际提交后，`AppShell` 回报 `interface_ready`。页面 load finished 不能替代这一回执；回执不等待项目、Agent 诊断或模型请求结束。
4. Rust 从 native ready / 主文档导航开始计时：15 秒没有就绪时，仅在本进程从未成功进入界面的前提下自动重载一次。第二次仍失败，显示绑定主窗口的原生 Retry / Close 提示；不再自动循环。

每次导航递增 generation；旧文档回执、旧计时器被忽略。generation 是时序标记，不是权限凭据。两个新增命令 `webview_startup_context` / `report_webview_startup` 都只接受 main 窗口，前端经 typed client 调用。

## 已进入应用后的边界

- 一旦收到有效界面就绪回执，本进程后续不再自动重载，包括用户后来主动刷新后失败的情形。不会为启动保护打断已经正常使用的界面。
- macOS 内容进程终止事件写入诊断；只有首次启动未就绪时走有界恢复。运行中的界面若异常，可使用 Loom 原生菜单 **Reload Interface… / 重新加载界面…**（⌘⌥R）。
- 手动恢复必须显式确认。它只 reload WebView，不退出 Rust、不取消/重复发送 Agent 工作；未保存的瞬时输入可能丢失，已保存草稿、会话和功能数据不清除。
- 进入应用退出流程后，计时器和提示回调均不恢复窗口。现有 graceful shutdown 逻辑不改。
- 原生提示绑定 main 窗口，避免无父窗口的系统级通知不易观察/操作；不依赖网页 `window.confirm` 或 React 渲染。

## 诊断

每次状态转换记录阶段、时间、PID、generation、自动重试计数；输出 `[loom-startup]` 并保存到 `global_config_dir()/webview-startup.json`，最多保留 128 条。临时 profile 下写 profile 目录；默认写应用配置目录。

不记录用户消息、网页 URL、提示词、命令参数、异常原文或完整环境。日志写入失败只输出固定错误代码，不阻止界面进入。诊断用于区分 native ready、navigation started/finished、interface ready、启动超时、内容进程终止与恢复耗尽。

本机制不证明 OS 绘制层永不出错：如果应用已报告 ready 但原生合成/可访问性异常，自动启动保护不会再次刷新。原始 P8 偶发事件的底层触发仍未独立重现，完整证据与适用范围见 [本轮验收](../dogfood/2026-10-28-webview-restart.md)。

## 验证入口

- Rust 状态机回归覆盖重试上限、旧回执/计时器、正常界面、手动重试与重载未开始。
- 前端回归覆盖文档 token、StrictMode 去重、失败时可见 fallback。
- `scripts/webview-startup-fixture.mjs once|always` 从已构建 dist 产生仅验收使用的资源：首次不运行入口/持续不运行入口。无故障开关进入正常生产代码；测试后必须重新构建正常 app。
- `pnpm smoke:desktop` 现在必须观察到当前启动日志中的 `interface_ready`；只活着四秒不再计通过。其结果是原生 IPC 就绪证据，像素/可访问性外观仍由原生窗口观察确认。
