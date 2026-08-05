# Agent 适配器协议

## 目标

Loom 的编排层只描述“在哪个项目、哪个阶段、用哪个 Agent、发送什么上下文”，不直接拼接特定 CLI 参数。所有本地 Agent 调用都必须先经过 Rust 后端的适配器层，得到统一的进程启动描述，再交给命令运行器执行和留痕。

## 阶段

适配器接受以下阶段，并在启动前校验 Agent 的 capability 与权限：

- `planning`：只读计划生成。
- `implementation`：允许写文件和运行命令。
- `review`：只读审查。
- `debugging`：允许写文件和运行命令；实施 Agent 可作为兼容后备。
- `testing`：只读代码，但允许运行验证命令。
- `documentation`：只读文档生成。

## 输入契约

统一输入包含：

- `projectPath`：已打开项目的根目录。
- `taskId`：用于生命周期门禁、运行记录和证据关联。
- `agentId`：已保存的 Agent 配置 ID。
- `stage`：当前调用阶段。
- `prompt`：已由上下文构建器裁剪、脱敏并汇总的提示词。
- `resumeCommand`：可选；仅内建且可安全解析的适配器允许恢复会话。

规划和 Review 的内部调用还可以提供 `promptFile`，让自定义 CLI 通过 `{promptFile}` 占位符读取长提示词。

## 输出契约

适配器返回 `PreparedAgentInvocation`：

- `program`：配置中登记的可执行程序。
- `args`：适配器生成的参数列表，不经过 shell。
- `cwd`：项目根目录。
- `stdinPrompt`：是否需要通过 stdin 发送提示词。
- `outputMode`：`plain`、`codex_json` 或 `claude_stream_json`。
- `resumed`：是否恢复了既有会话。

调用者不得修改 `program` 或重新拼接特定 Agent 的参数。实施与调试入口还必须把 `agentId` 传给命令运行器，由后端再次核对命令、能力与权限。

## 内建映射

### Codex CLI

- 只读阶段使用 `read-only` sandbox。
- 实施和调试使用 `workspace-write` sandbox。
- 输出使用 JSON 事件流。
- 恢复命令只接受与当前配置程序一致的 `codex resume <session-id>` 形态。

### Claude Code

- 非交互调用使用 print + stream-json。
- 实施和调试阶段使用 `acceptEdits` 权限模式。
- 恢复命令只接受与当前配置程序一致的 `claude --resume <session-id>` 形态。

### 自定义 CLI

支持以下参数占位符：

- `{projectPath}`
- `{promptFile}`
- `{prompt}`
- `{stage}`

自定义 CLI 默认不支持会话恢复；如需恢复能力，应新增显式适配器并对恢复命令进行结构化解析，不能把任意字符串交给 shell。

## 诊断与失败语义

Agent 设置页通过后端诊断获取命令解析路径、版本探测结果、启用状态和详细说明。命令存在不等同于已经登录；认证状态在下一次真实调用中验证。调用失败需要记录退出码、超时、stderr 摘要、证据路径和可恢复会话信息。

## 扩展新 Agent

新增 Agent 时：

1. 实现 `AgentAdapter::prepare`。
2. 为阶段权限、参数映射、输出模式和恢复命令增加单元测试。
3. 在适配器选择函数中注册新类型。
4. 如输出不是现有三种模式之一，再扩展统一日志解析层。
5. 不得在 React 组件中添加该 Agent 的 CLI 参数分支。
