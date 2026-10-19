# Provider workers

当前实现是 M0 的真实 SDK 边界实验，**尚不是生产适配器**。只接受明确的 `http://127.0.0.1:<port>` 模型端点，只暴露 SDK Write/write 工具，使用独立 fixture 目录，不读取用户模型凭据或项目技能。

- Claude SDK 通过 `PreToolUse` 将完整工具调用发给 Rust；未获匹配允许前不执行。
- Pi SDK 通过实际 `createWriteToolDefinition` 执行器的前置包装发出相同审批请求。
- 请求带 workspace/session/turn/tool-call、cwd、参数 SHA-256；回应必须匹配 request ID、scope、digest。
- 取消、stdin 断开或审批超时使 pending 请求拒绝。真实写文件结果由宿主 smoke 核验。
- 依赖固定为 Craft v0.14.0 对应 SDK 版本；本包自己的锁文件支持独立安装，不依赖 `.ref` 作为运行时。

运行：

```sh
bun install --frozen-lockfile --ignore-scripts --cwd packages/provider-workers
bun run --cwd packages/provider-workers typecheck
cargo test --manifest-path experiments/runtime-bridge/Cargo.toml
cargo build --manifest-path experiments/runtime-bridge/Cargo.toml
python3 scripts/runtime-bridge-smoke.py
```

当前十个集成场景覆盖两 SDK 的 allow/deny/cancel/stale/disconnect。模拟的是模型 HTTP 响应，SDK 和工具执行均为真实实现；这不证明真实模型、OAuth、所有工具、MCP、sandbox、多平台或恢复已经完成。M2 必须增加完整能力与生产配置，移除实验专用政策后仍保持 Rust 权限控制。
