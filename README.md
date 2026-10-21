# Loom

Loom 是 Tauri + React + Rust 构建的本机 CLI Chat。会话按项目分组，选择 Read-only / Ask / Auto 权限，在输入框中选择本机 Claude Code、Codex、OpenCode、Grok 或自定义 CLI，查看流式回答和工具调用，停止并继续对话。

界面默认英文，可在 Settings → Appearance 切换简体中文和主题。Agent 登录与模型选择仍由各 CLI 管理；本机启动 CLI 不代表离线推理。

## 启动

```bash
pnpm install
scripts/debug.sh stop
scripts/debug.sh desktop
# 使用结束后
scripts/debug.sh stop
```

网页预览：`scripts/debug.sh web`，固定端口 1420。开发态访问 `http://127.0.0.1:1420/?fixture=simple-chat` 可查看合成会话；该模式不调用模型，不能作为真实 CLI 验收证据。

## 使用边界

- Read-only 对应 CLI 只读策略；Ask 每回合确认后允许修改，Auto 使用适配器声明的自动执行策略。Loom 不提供逐工具审批。
- 已有 `.loom/chat/v2` 会话继续读取。Task/Board/Planning 工作流已退出应用；旧任务文件保留，不迁移、不删除。
- Workbench、Rust 新内核和 SDK worker 源码冻结，不进入默认应用与主门禁。
- 各 CLI 的已测能力与环境阻塞见 [本轮验收记录](docs/dogfood/2026-10-20-simple-chat-acceptance.md)。

## 验证与文档

完整工程检查为 `pnpm check`，桌面构建为 `pnpm tauri build --no-bundle`。`pnpm smoke:visual` 检查合成网页交互与截图；`pnpm smoke:chat` 验证真实本地 fixture 进程，不请求模型。

- [需求](docs/requirements.md)、[架构](docs/architecture.md)、[Chat 契约](docs/architecture/chat-contracts.md)
- [Agent 适配与实测](docs/agent-adapter.md)、[测试验收](docs/testing.md)
- [实施计划](docs/plans/2026-10-20/01:01-simple-chat-implementation.md)、[计划索引](docs/PLANS.md)
- [开发指南](docs/development.md)、[脚本](scripts/README.md)、[发布资料](docs/release/README.md)
