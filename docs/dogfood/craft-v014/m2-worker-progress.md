# M2 正式 worker 与进程监督切片

日期：2026-10-08；基线 `1bf174b` 加本轮修改。新 Rust supervisor 和 Claude/Pi worker 已实现，Pi 的可执行证据如下；完整桌面 SDK 与账户接入仍未完成。

## 已验证

- `pnpm check:core`：80 个测试入口通过（原 core 68 + runtime 12），含严格 Clippy、fmt、两套生成协议漂移和协议 TS 编译。
- worker `bun run typecheck` 通过；Python 脚本语法检查通过。
- Runtime 11 个真实进程案例覆盖精确请求/决定、拒绝无写入、未批准工具结果、错作用域/序号、坏帧/残尾/超预算、无终态、错误退出码、取消忽略 SIGTERM 的进程、超时、残留子进程回收、stderr 持续排空、秘密不在 argv/env 和阻塞 Handler 时 watchdog 阻止延迟写入。另 1 个流式过滤测试覆盖重叠前缀/Unicode/转义/编码。
- 真实 Pi SDK + 新 worker + Rust supervisor/core，使用进程内注入 fetch 的模型响应：允许、拒绝、取消、迟到决定、无认证与命令形状字面值凭据，共 6/6 通过。允许前文件不存在，批准后只有指定合成文件；拒绝/取消没有写入；core 保存审批和回合终态。见 [Pi 机器证据](m2-worker-fetch-results.json)。
- 测试发现并修复了 SDK 在无认证模式自动附带 Authorization 的行为；最终请求中 Authorization/x-api-key 均不存在。修复 Pi 配置 apiKey 表达式执行风险，合成 `!printf …` 凭据按字面值传递且无命令副作用。

所有结果使用本机合成凭据、临时目录和真正 SDK 的工具执行器，没有真实模型账户调用。主测试文件、协议和 worker SHA-256 记录在 [总证据](m2-worker-evidence.json)。

## 尚未验证及原因

- 正式 worker 的真实 HTTP 组合（Claude/Pi × 5 场景）：两次 require_escalated 自动审批均超时，命令未启动；普通沙箱 bind 127.0.0.1 返回 Operation not permitted。已请求一次明确许可，尚无回复。没有把以前 M0 worker 的 HTTP 通过结果套用到新 worker。
- Claude 新 adapter 目前只有 SDK 类型检查，没有本轮运行证据；Pi 注入 fetch 不验证 TLS、实际网络或服务认证。
- 桌面连接表单原生验收仍受先前锁屏限制；本轮没有启动原生预览。
- Tauri 正式调度、生产工具分类/自动策略、vault 到 worker 的发布接线、完整工具/认证类型、resume/compact/steer、完整 usage/日志和跨平台/打包仍需完成。

可复现的命令与精确边界见 [worker 契约](../../architecture/sdk-worker.md)。当前不是 M2 或整体 M0–M8 完成证明。
