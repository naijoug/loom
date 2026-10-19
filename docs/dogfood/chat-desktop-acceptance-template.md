# Chat 真实桌面验收记录模板

## 用途

用于记录单 Agent Chat 的真实桌面验收。它补足 `tests/unit/*`、`pnpm smoke:chat` 和 fixture 进程不能证明的部分：真实 CLI 登录、权限、流式输出、停止、重启、resume、日志与导出证据。

> 规则：没有实际运行的项目必须写 `not tested`；失败写 `failed` 并保留错误摘要。不要用单测、提示词推断或旧 dogfood 记录替代本轮真实结果。

## 验收元信息

- 日期：YYYY-MM-DD
- Loom commit：
- 启动命令：`scripts/debug.sh desktop`
- 验收后清理命令：`scripts/debug.sh stop`
- 操作系统：
- 临时项目：`path/to/safe-temp-project`（必须是可安全读写的测试目录；记录相对或脱敏路径）
- Agent CLI：
- CLI 版本：
- 登录/能力检查：passed / failed / not tested
- 权限模式：Ask / Auto / 只读 / 可写（按实际 UI 与 CLI 组合填写）
- 会话 id / 导出目录：

## 启动与环境检查

| 项目 | 结果 | 证据 |
| --- | --- | --- |
| `pnpm smoke:chat` | passed / failed / not tested | 命令输出摘要；该项不请求模型 |
| 桌面应用启动 | passed / failed / not tested | 可看到 Chat 页面、项目列表和 Agent 设置 |
| 目标 CLI 可用 | passed / failed / not tested | 版本、登录状态、失败原因 |
| 临时项目安全 | passed / failed / not tested | 项目用途、是否允许写入、初始 git 状态 |

## 真实对话路径

| 步骤 | 结果 | 证据 |
| --- | --- | --- |
| 创建新会话并发送首轮 | passed / failed / not tested | 真实流式输出、错误或退出状态 |
| 连续三轮追问 | passed / failed / not tested | turn id、是否重复注入历史、resume 行为 |
| 切换会话/项目后旧回合不污染当前会话 | passed / failed / not tested | 观察到的会话标题、消息、状态 |
| 重命名、归档/恢复、搜索 | passed / failed / not tested | 操作结果与重启前后状态 |
| 重启后读取转录 | passed / failed / not tested | `.loom/chat/v2/sessions/...` 或 UI 转录可读性 |

## 停止、退出与恢复

| 场景 | 结果 | 证据 |
| --- | --- | --- |
| 生成中点击停止 | passed / failed / not tested | UI 是否显示停止中、子进程退出状态、日志尾部 |
| 停止后再发 | passed / failed / not tested | 新 turn 是否正常创建且不复用坏状态 |
| 应用退出时有活动回合 | passed / failed / not tested | 退出方式、进程残留检查、重启后的会话状态 |
| 无原生 resume fallback | passed / failed / not tested | 是否提示预算/截断，最新请求是否完整 |

## 权限语义

| 场景 | 结果 | 证据 |
| --- | --- | --- |
| Ask 取消确认不发送 | passed / failed / not tested | UI 行为和无新增 turn 证据 |
| Ask 确认后才发送 | passed / failed / not tested | 确认动作、turn id、日志 |
| Auto 不增加每回合确认 | passed / failed / not tested | UI 行为和权限提示 |
| 只读限制 | passed / failed / not tested | 真实 CLI / OS 层限制结果；不能只记录 flag |
| 可写限制 | passed / failed / not tested | 只在临时项目中验证；记录改动和清理方式 |

## 导出与日志证据

- Markdown 导出：passed / failed / not tested；路径：
- JSON 导出：passed / failed / not tested；路径：
- Manifest：passed / failed / not tested；是否有 warnings：
- stdout/stderr 日志：passed / failed / not tested；缺失或损坏必须写入 warnings：
- 脱敏检查：passed / failed / not tested；不得包含密钥、未脱敏 `.env` 或私人路径。

## 结论

- 总结：passed / failed / partial
- 阻塞问题：
- 不得复用为发布结论的范围：签名、公证、安装、卸载、真实回滚、其他 CLI、其他 OS。
- 下一步：

## 可复制命令清单

```bash
pnpm smoke:chat
scripts/debug.sh desktop
scripts/debug.sh stop
```
