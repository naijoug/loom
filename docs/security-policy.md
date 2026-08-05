# Loom 安全策略

## 原则

- 本地优先、最小权限、默认拒绝路径逃逸。
- UI 禁用不是安全边界；所有命令和阶段门禁由 Rust 再校验。
- 每次执行保留脱敏后的命令、cwd、时间、状态、退出码和终止原因。

## 执行策略

`execution_policy.rs` 对 command runner、PTY 与 Agent invocation 使用同一套检查：

1. canonicalize 项目根和 cwd，cwd 必须位于项目内。
2. 校验 Agent 启用状态、阶段能力、可执行命令、`can_write_files` 和 `can_run_commands`。
3. 对安全命令直接放行；安装依赖等中风险行为遵守用户确认设置；删除、破坏性 Git 和生产外部目标始终要求显式确认。
4. 审批凭证只使用一次，并绑定 program、args、cwd、项目和 Agent，不能复用到另一条命令。
5. 子进程使用独立进程组；停止、超时、暂停、阻塞和取消会终止关联进程树。

危险类别及示例：

| 类别 | 示例 | 策略 |
|---|---|---|
| dependency_install | `npm install`, `cargo add` | 依设置确认 |
| destructive_filesystem | 递归删除、覆盖 | 强制确认 |
| destructive_git | reset/clean/强推 | 强制确认 |
| production_external | 明确生产域名或部署命令 | 强制确认 |
| path_escape | cwd 或证据路径越出项目 | 拒绝 |

## 敏感信息

- 命令、stdout/stderr、Planning/Review 输出、context 与 summary 在写盘前经过统一脱敏。
- 常见 Authorization/Bearer、password、token、secret 和环境变量赋值会替换为 `[REDACTED]`。
- Loom 不把完整环境变量写入 Task 或日志。summary 只引用验证日志路径，不复制原始日志正文。
- 脱敏是最后一道防线；用户仍不应把长期凭据直接写入需求或文件名。

## 文件与附件

- Task/run id 只接受受限 ASCII 标识符，防止目录穿越。
- 反馈最多 8 个附件，单文件 10 MiB、总计 30 MiB；仅允许常见图片、文本、JSON、Markdown、日志和 PDF。
- 附件先 canonicalize 并验证为普通文件，再复制到任务专属目录；Agent 获得的是受控副本路径。
- 历史日志只从 `.loom/logs/<task-id>/` 读取，每次最多读取尾部 2 MiB / 5000 行。
- 总结导出由系统文件保存对话框选择目标，格式仅限 Markdown 或 JSON，并使用原子替换。

## Tauri 边界

- CSP 禁止任意远程脚本；开发连接只开放本地 Tauri/Vite 所需来源。
- Capability 仅包含窗口拖动、附件选择、总结保存/打开/定位。
- 自定义 CLI adapter 不接受 shell wrapper 作为可执行命令，避免把未经解析的脚本字符串绕过策略。

## 已知边界

Loom 不是操作系统级沙箱。经用户确认的本地 Agent 仍以当前用户身份运行，并可能读取该用户可访问的文件。应只配置可信 Agent 命令，并在高价值仓库中结合 Git、备份和最小权限账户使用。
