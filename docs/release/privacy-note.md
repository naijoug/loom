# Loom Beta 隐私说明

- **状态**: draft
- **适用阶段**: M3 隐私、安全与诊断包披露
- **关联文档**: `docs/release/beta-safety-notes.md`、`docs/release/local-data-and-uninstall.md`

## 一句话边界

Loom Beta 是本地优先桌面应用：默认数据留在你的 Mac 和你选择的项目目录中；但当你配置真实 Agent CLI、运行命令、导出诊断包或主动发送反馈时，相关上下文可能离开本机。首轮 Beta 不承诺企业级合规、云端同步或远程沙箱隔离。

## 默认留在本机的数据

Loom 会在本机保存用于恢复任务和排查问题的记录，包括：

| 数据 | 默认位置 | 用途 |
|---|---|---|
| 项目元数据 | 项目内 `.loom/loom.json` | 识别项目、schema 版本和最近任务 |
| 任务历史 | 项目内 `.loom/tasks/` | 保存 Planning / Review / Testing 状态和人工确认记录 |
| 命令与 Agent 日志 | 项目内 `.loom/logs/` | 复核 stdout / stderr、退出码、阻塞原因和验证证据 |
| 计划证据 | 项目内 `.loom/planning/` | 保存多 Agent 计划、互评和综合计划 |
| 最近项目列表 | app data 下 `loom/recent-projects.json` | 让启动页显示最近打开过的项目 |

卸载 App bundle 不会自动删除这些数据；清理步骤见 `docs/release/local-data-and-uninstall.md`。

## 默认不主动收集的内容

首轮 Beta 不包含 Loom 自有云服务，因此默认不主动上传：

- 完整项目源码。
- 完整环境变量。
- 完整系统磁盘索引。
- 后台遥测、产品分析事件或崩溃自动上报。
- 真实 Agent 账号 token、password、API key 或 cookie。

注意：如果你把这些信息写进任务描述、文件名、命令参数、日志或反馈附件，它们仍可能进入本机 `.loom/` 记录或被你主动发出。

## 可能离开本机的路径

以下场景可能让项目上下文、命令输出或人工反馈离开本机：

1. 你配置并运行 Codex、Claude Code、OpenClaw、Hermes 或自定义 CLI；这些工具可能按各自策略把上下文发送给服务商。
2. 你在反馈中主动附上截图、日志片段、诊断包、任务摘要或项目文件。
3. 你在 Loom 中运行会访问网络的命令，例如包管理器、部署脚本或自定义调试脚本。
4. 你把 `.loom/` 目录或导出的总结文件复制到外部协作工具。

如果你不确定某个 Agent 或命令会发送什么内容，请先使用 `docs/release/beta-smoke.md` 的 dummy / fixture 流程，不要配置真实账号。

## 脱敏与诊断包边界

Loom 的安全策略会在写盘前对命令、stdout/stderr、Planning/Review 输出、context 和 summary 做统一脱敏，常见 Authorization/Bearer、password、token、secret 和环境变量赋值会替换为 `[REDACTED]`。已有 dogfood 记录显示：诊断包导出会脱敏项目根路径和敏感字段；日志默认不导出，用户选择导出日志时限制为最近 20 份、每份 200 行、最多读取 64 KiB 尾部。

这不是发送前免审承诺。发送任何材料前仍必须人工复核：

- 路径、仓库名、用户名、客户名是否需要打码。
- stdout/stderr 是否包含 `.env`、token、cookie、私钥、Authorization header 或内部 URL。
- 代码片段是否暴露商业逻辑或未公开实现。
- 截图是否包含聊天记录、浏览器标签页、终端历史或其他项目名称。

发现敏感信息时，不要发送诊断包；在 `docs/release/beta-feedback-template.md` 中把类别标为 `security`，说明“诊断包因疑似敏感信息未附”。

## 试用者操作建议

- 只用临时项目、示例项目或低风险个人项目试用。
- 试用前提交或备份项目，确保可回滚。
- 不在任务描述、反馈、附件、文件名或命令参数中粘贴长期凭据。
- 真实 Agent 账号只在你理解其服务商数据策略、额度和费用后启用。
- 外发反馈前优先发最小复现步骤；只有必要时再附脱敏日志或诊断包。

## Beta 阶段不承诺

首轮 Beta 不承诺：

- Apple Developer ID 签名、公证或企业 MDM 分发。
- 团队权限隔离、集中审计或合规报告。
- 对第三方 Agent 服务商的数据保留、训练使用或计费策略负责。
- 操作系统级沙箱隔离。经用户确认的本地命令仍以当前系统用户身份运行。

## 隐私停止线

出现以下情况请停止试用并反馈：

- 诊断材料中出现未脱敏 token、cookie、私钥、客户信息或生产数据。
- Agent 或命令要求上传密钥、关闭安全软件、访问生产环境或扩大系统权限。
- 你无法判断下一步是否会把私有上下文发送给外部服务。
- App 行为与 `docs/release/beta-safety-notes.md` 描述明显不一致。
