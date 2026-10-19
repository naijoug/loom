# 重建运行时决策：M0 阶段证据

- 状态：阶段性决策，2026-09-30。不是 M1–M8 完成证明。
- 产品基线：Craft v0.14.0；主应用继续 Tauri + React + Rust。
- 证据：[M0 记录](../dogfood/craft-v014/m0-progress.md)、[SDK 集成](../dogfood/craft-v014/sdk-bridge-results.json)、[Browser 集成](../dogfood/craft-v014/browser-probe-results.json)。

## ADR-001：Rust 拥有领域控制，SDK 作为受监督进程

采用 Rust session/runtime/policy/store 为事实源，Claude/Pi 由最小 provider worker 承载。不存在 TypeScript 的第二套 SessionManager、Source 仓库或任务调度器。前端只连接 typed HostAPI。

依据：真实 Claude SDK PreToolUse 与 Pi SDK Write 执行器包装已通过 Rust 往返，允许/拒绝/取消/断线/旧回合回应都有实际副作用检查。SDK 和其自有工具运行在外部进程，由 Rust 绑定调用、批准和监督；这与让 UI 直接执行文件/命令不同。

生产化约束：

- 统一帧携带 protocolVersion、workspace/session/turn/tool-call/request ID。授权还需绑定 effective policy version 与执行配置，不仅绑定文本。
- native SDK 工具在执行前经 Rust；不能用 canUseTool 回调假设所有工具都会经过它。Claude 使用 hook，Pi 使用显式受控工具清单/执行器。
- 未声明的 capability 在 UI 不可冒充已支持；权限不足不能通过 prompt 或 CLI flag 绕过。
- stdin EOF、回合取消、审批过期不继续等待或执行；进程组清理必须由宿主负责。真实 OS 沙箱能力与产品权限模式分开声明。
- 后续 Read/Bash/MCP、resume/steering、认证、多个并发会话与资源激活仍需完整生产路径测试。

当前 `packages/provider-workers/src/worker.ts` 是 loopback-only 单写工具实验。其固定 prompt、模型 fixture 凭据和单 turn 限制不进入生产 UI；M2 扩展时需要真正的 adapter 和 vault，而非简单删除校验。

M2 已在独立 core 实现持久化单次审批，并通过 `crates/loom-core/examples/sdk_core_approval_probe.rs` 把真实 SDK 的 Write/write 审批接到该生产 journal/租约/消费路径。`scripts/runtime-bridge-smoke.py --core` 验证允许、拒绝、取消、迟到回应、断线。示例中的工具分类仍是单个合成文件的限定规则，不是生产 policy；真实模型账户、完整工具和桌面 SDK 调度继续实施。

## ADR-002：浏览器使用独立 Rust/CEF 宿主

采用独立 Rust CEF helper 作为候选浏览器宿主，保持 Tauri 主应用及领域服务不变。通过受控 stdio/IPC 暴露浏览器能力；不在 loopback 开放通用 CDP 调试端口。

依据：官方 cef-rs 在本机成功打包；适配后的实验通过 CEF native window、CDP AX、实际页面表单、隐藏/恢复、原生关闭转隐藏、持久化 Cookie/localStorage 与重启。原生 AX/剪贴板缺口通过显式 accessibility 开启与 macOS Edit responder 转发修复，已用 CUA 验证中文粘贴和提交。

候选依赖固定 `cef = 154.3.0+154.0.32` / Chromium 154.0.8037.58，原型 debug app 约 326 MiB。这是本机实验包大小，不代表最终 release 体积或性能。需要继续核验授权声明、平台资源、helper 签名、公证与更新；不会把 CEF 原型打包成功等同于 Loom 发布完成。

生产化约束：

- Rust BrowserHost 保存窗口/页面 ID、workspace/session 归属、有效权限和调用者；远程客户端必须显式协商 host capability。
- 可信浏览器工具栏与不可信网页分离；网页不接触 Tauri commands、文件系统、vault 或 stdio 桥。Prototype 没有该完整工具栏。
- 原生关闭 browser window 隐藏保留；应用 Quit/服务停止真正结束并 flush profile。不能混淆两者。
- Cookie partition 的默认共享行为按上游核验，不因技术上支持多个 profile 就擅自改变产品语义。窗口/tab 列表按 workspace 隔离。
- 完整导航/AX 引用、下载、网络检查、弹窗、认证、崩溃恢复、多窗口、跨平台和打包仍是 M0/M5/M8 的待验收项。

## ADR-003：同一产品规格，隔离新旧领域

新领域使用 Workspace、可选 Project、Session、Turn、ToolCall、Source、Skill、Page、TaskRun、Automation。旧项目目录和旧 Task 工作流只通过导入器读取。

新实现不能依赖旧 AppState reducer 或固定 Planning→Implementation→Review→Testing 状态机。可复用原子写、脱敏、进程控制、seq/幂等思路，但需在新契约下验证。日志与元数据不会因 UI 路由切换而消失。

共享契约由新 Rust core 和 TS 客户端共同验证。M1 先建立可测试的事实源、journal、快照和恢复，不先生成全部空目录。

## 后续必须保留的验证缺口

- SDK 目前仅验证真实 Write/write 及其审批控制；其他工具类别和账户未通过。
- 原生中文粘贴不等于真实输入法 composition 的全面验收。
- 单 browser 的 CEF 实验不等于多 workspace、远程 browser 或完整 chrome 完成。
- Linux/Windows 没有真实运行证据。
- 第一方分享/OAuth relay/更新端点仍需 [服务清单](rebuild-services.md) 的独立实现和验收。
- 参考截图是初始观测，完整固定尺寸/主题/异常态的黄金基线仍需补齐。
