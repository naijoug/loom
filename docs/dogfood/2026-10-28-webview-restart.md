# 原生重启空白窗口：排查、恢复修复与验证

基线：`1ff6e6b` + 本会话未提交 P7/P8 工作区。对应 [修复计划](../plans/2026-10-28/02:48-native-webview-restart.md)，实现契约见 [窗口启动](../architecture/webview-startup.md)。

## 结论与证据边界

已修复“启动未完成时只能留在空白窗口、缺少就绪判定和恢复入口”的应用层缺口：静态启动内容、真正的界面就绪握手、首次启动最多一次自动重载，以及独立于 WebView 的原生提示/手动恢复入口。

**原始 P8 那一次的底层 OS/绘制触发仍未完全确认。**本轮同数据未修改版本两次启动及隐藏/唤醒没有重现。不能用故障注入成功或后续不再出现，宣称已根治所有 WebKit/显示层异常。

## 历史现场定位

限定读取原故障 Loom PID 86638 与 WebContent PID 86666 的系统日志，不读取其他应用内容：

- 00:00:10.751 主文档 commit，10.775 load finished，10.901 记录 firstMeaningfulPaint=0.053；资源加载与 JavaScript 执行有成功记录。
- 后续因窗口被遮挡进入后台挂起，00:00:47 恢复可见。没有找到该内容进程崩溃/退出证据，因此没有把处理方向武断改为进程崩溃或清缓存。
- AppKit 的 restoreWindowWithIdentifier 告警及部分 WebKit 沙箱告警在正常启动 PID 31501 也出现；不能据此关闭安全保护或删除窗口状态。
- 旧代码的 HTML 为空 root；异步入口初始化没有外围失败兜底，React ErrorBoundary 不能捕获它挂载之前的模块失败。Rust 未收取界面就绪回执。

证据：[故障时间线](samples/webview-2026-10-28/historical-timeline.txt)、[限定历史日志](samples/webview-2026-10-28/historical-webkit.txt)。正常启动对照日志与完整限定日志同目录保存。

## 修复

- `bootstrap.ts` 动态加载 React 图；外壳提交后回报当前 generation，旧页面的延迟消息不能把新页面误判 ready。
- Rust 15 秒无 ready 自动重载一次；持续失败显示主窗口原生 sheet。正常 ready 之后不自动刷新，退出时不恢复。macOS 内容进程终止仅在首次未就绪时参与自动恢复。
- 原生菜单 Reload Interface…（⌘⌥R）经确认后只重载界面。保留 Rust 状态和磁盘历史，不重新发送 Agent 请求；取消确认不重载。
- 启动状态日志有界 128 条，不记录用户内容、URL、原始错误或环境。原生启动全过程可区分 page finished 与 interface ready。
- `desktop-smoke.sh` 改为通过 debug.sh 启停准确包，并以当前进程 `interface_ready` 判成功；旧版只检查进程存活四秒，会漏掉空白界面。

## 验证矩阵

| 项 | 结果 / 边界 |
|---|---|
| 定向 Rust 启动回归 | 6/6 passed |
| 前端启动 + 现有功能栏 + 契约 | 9/9 passed（新增启动 3 项） |
| 首次失败注入 | passed：首次页面 load finished 但不运行前端；15 秒后仅 1 次 automatic_reload，第二文档 interface_ready；原生可见四功能与历史 |
| 持续失败注入 | passed：仅 1 次自动重载，第二次超时后原生 Retry/Close sheet 可见，recovery_exhausted 后不循环；关闭按钮可退出 |
| 正常最终构建原生重启 | 5/5 界面可见，每次 automaticReloads=0；Chat / Research / Discuss 历史可读 |
| 手动恢复 | 原生 sheet 可见；取消保留 Discuss 视图、日志无 manual_reload；确认后新文档再次 ready，已有记录仍在 |
| 桌面 smoke 正例 | 2/2 次界面就绪，脚本正确完成 |
| 桌面 smoke 反例 | passed：sleep fixture 持续存活超过旧四秒门槛但不发 ready，新脚本按预期失败（exit 1）并清理 |
| 完整门禁 | passed：`RUST_TEST_THREADS=1 pnpm check` 前端 148、Rust 328（30 ignored）全过；此前失败另列 |
| 正常 app / no-bundle | 正常 app 与 no-bundle 均 passed；最终产物不含注入入口，5 次正常 GUI 观察均自动重试 0 次 |
| 完整合成矩阵 | passed：全部 Chat / 功能栏 / Learn / Research / Discuss / Skills / 卡片矩阵 |

故障注入只修改 `.tmp/webview-fixtures/` 中的打包资源，未给生产代码增加“跳过界面”环境变量/开关。测试后已恢复正常构建。全程未调用真实模型，不操作或删除旧 Task、研究和讨论数据。

## 全量检查的非启动失败

- 第一轮并行全量：327/328 Rust 通过；既有 Chat shutdown 子进程 fixture 出现 killpg EPERM（P1/P4/P7/P8 已有同类记录）。不把它归因于本次就绪保护。
- 第二轮并行全量：327/328 通过；既有 Skills 四 provider fixture 返回“Translation has not completed successfully”，未取得足以确定其底层根因的记录，不能直接归为上项。
- 为诊断采用 Rust 串行模式后，发现既有 Skills 子进程 ready 标记解析只接受独占一行；libtest 串行模式会把测试名称放在同一行，导致父进程错过 ready 并等待子进程自行退出。只调整测试端识别两种精确标记形式，不改锁/生产行为，不放宽锁断言；该用例串行定向复验 1/1 passed。
- 保留全部失败输出；最终门禁结果和执行模式单独记录，不用定向通过冒充完整检查。

## 文档/代码差异

1. 验收口径补充：原 `scripts/desktop-smoke.sh` 只检查存活四秒，没有界面判据；本次代码改为强制 ready 回执，`docs/testing.md` 与 `scripts/README.md` 同步明确该层证据仍不能替代视觉观察。不把旧的“进程 smoke 通过”回填成历史 UI 已就绪。
2. P8 的“重启空白原因未定位”是当时有效记录，保留为历史。当前补充已证实的页面加载时间线及应用层恢复机制，不把未知底层诱因改写为已定因。

实现、故障注入、正常重启与历史现场属于不同证据层级。修改仍在工作区，未提交/推送/发布。

## 最终证据

[完整门禁](samples/webview-2026-10-28/check.txt)、[正常 app 构建](samples/webview-2026-10-28/bundle.txt)、[no-bundle](samples/webview-2026-10-28/build.txt)、[合成矩阵](samples/webview-2026-10-28/visual.txt)、[一次故障恢复](samples/webview-2026-10-28/once.txt)、[持续故障上限](samples/webview-2026-10-28/always.txt)、[桌面 smoke 正例](samples/webview-2026-10-28/desktop-positive.txt)、[反例](samples/webview-2026-10-28/desktop-negative.txt)、[汇总与源码哈希](samples/webview-2026-10-28/summary.json)。

五次最终正常构建观察均有原生 AX 证据和 native ready → interface ready 时间线，未触发自动重载。取消原生提示时 Discuss 当前视图保持；确认时重新就绪。既有 P8 Chat messages/turns、Research turns、Discuss tasks/attempts 与验收前保存的内容一致。未新增模型调用；最终预览已清理。
