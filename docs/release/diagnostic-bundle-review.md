# Loom Beta 诊断包安全复核

- **状态**: draft
- **适用阶段**: M3 隐私、安全与诊断包披露
- **关联文档**: `docs/release/privacy-note.md`、`docs/release/beta-feedback-template.md`、`docs/security-policy.md`、`docs/dogfood/project-optimization-2026-08-05.md`

## 一句话边界

诊断包是邀请制 Beta 的排障辅助材料，不是自动可外发的安全材料。Loom 已有脱敏、路径替换和日志限额机制；试用者和维护者在发送、接收、归档前仍必须人工复核，发现疑似敏感信息时按 `security` 反馈处理，不附原始包。

## 当前导出内容

`export_diagnostic_bundle` 生成 JSON，当前 schema 包含：

| 区域 | 内容 | 隐私处理 |
|---|---|---|
| `loom` | Loom 版本、后端标识 | 不含用户数据 |
| `system` | OS 与 CPU 架构 | 只用于排查平台问题 |
| `project` | 项目名、项目路径占位 | 路径写为 `[PROJECT_ROOT]` |
| `task` | 任务状态、暂停/阻塞/取消原因、主 Agent、最近 run 摘要 | 文本字段经脱敏；run 最多 100 条 |
| `policyDecisions` | 命令策略判定、风险等级、类别、说明 | detail 经路径与 secret 脱敏 |
| `logs` | 用户选择后导出的 stdout/stderr 尾部 | 默认不导出；最多 20 份、每份 200 行、每份尾部最多 64 KiB |
| `omittedLogCount` | 因限额省略的日志数量 | 帮助判断证据是否不完整 |

不应把诊断包理解为完整审计日志：它偏向排障，且会刻意省略或截断日志。

## 已有工程证据

来自 `docs/dogfood/project-optimization-2026-08-05.md` 和 `docs/security-policy.md` 的可复核证据：

- `export_diagnostic_bundle` 已纳入 Tauri command contract。
- 诊断包导出版本、系统、任务/run 元数据和策略判定。
- 项目根路径替换为 `[PROJECT_ROOT]`，HOME 路径替换为 `~`。
- 命令、阻塞原因、日志尾部等文本会走统一敏感信息脱敏，常见 Authorization/Bearer、password、token、secret 和环境变量赋值替换为 `[REDACTED]`。
- 日志默认不导出；用户主动选择后只读取项目内 `.loom/logs/` 下的最近日志尾部。
- secret fixture 已验证命令、日志、阻塞原因中的 token 不出现在诊断 JSON。
- 2026-08-09 复跑 `cargo test diagnostic_bundle --lib` 通过，记录见 `docs/release/diagnostic-bundle-engineering-proof.md`；该记录证明后端 fixture 仍有效，但不替代 UI 手工 smoke。
- 真实付费 Agent canary 尚未执行；该路径仍是显式手工触发，不属于当前自动诊断证据。

## 导出前自查清单

试用者导出前先执行：

1. 确认该问题确实需要诊断包；能用复现步骤、截图或最小日志片段说明时，不优先导出完整包。
2. 默认关闭日志尾部；只有排查 stdout/stderr、退出码或命令行为时才勾选日志。
3. 如果任务描述、命令参数、文件名或日志中曾出现 token、cookie、客户名、内部 URL、私钥或生产数据，不外发诊断包。
4. 导出到临时位置后，用文本编辑器打开 JSON，搜索：
   - `token`
   - `secret`
   - `password`
   - `Authorization`
   - `Bearer`
   - `OPENAI_API_KEY`
   - `ANTHROPIC_API_KEY`
   - 公司、客户、仓库或用户名关键词
5. 若搜索结果只是字段名或 `[REDACTED]`，继续人工浏览相关上下文；若出现真实值，删除诊断包并提交 `security` 反馈。
6. 如需发送，优先附最小诊断包；不要连同 `.loom/` 目录、完整日志目录或项目源码一起打包。

## 接收方处理规则

维护者收到诊断包后：

1. 先按 `docs/release/beta-feedback-template.md` 标记分类和严重度。
2. 看到未脱敏凭据、客户信息、生产路径或完整源码片段时，立即停止继续传播，反馈分类升级为 `security` / `S0 停止分发`。
3. 只把必要结论写入 issue、计划或修复记录；不要复制原始敏感片段。
4. 如果问题与真实 Agent 有关，记录 Agent 类型、是否真实账号、是否可能计费；不要要求试用者上传服务商 token 或完整会话。
5. 修复后补一条可复跑验证：至少包含触发方式、预期脱敏字段、实际 JSON 中不存在的敏感样例。

## 不可外发条件

出现任一条件时，不发送诊断包：

- JSON 中出现真实 token、cookie、私钥、完整 Authorization header 或完整环境变量。
- 日志尾部包含客户代码、商业秘密、生产数据、内部域名或未公开接口。
- 任务要求访问生产环境、关闭安全策略、扩大文件系统权限或上传密钥。
- 试用者无法判断诊断包是否包含私有上下文。
- 诊断包来自高价值仓库、客户项目或工作机生产目录。

## Beta 发布门禁

进入更大范围 Beta 前至少补齐：

- 一次从 UI 导出诊断包的手工 smoke 记录，覆盖“默认不导出日志”和“选择日志尾部”两条路径。
- 一条故意包含 fake token / fake path 的 fixture 记录，证明 JSON 中只出现 `[REDACTED]` 与 `[PROJECT_ROOT]`。
- 一条真实反馈处理演练：用 `security` 模板记录“因疑似敏感信息不附诊断包”的流程。
- 如要公开下载，再把诊断包说明合并进 release notes 或下载页，不只放在仓库文档里。

## 后续改进

- 在导出 UI 增加“发送前搜索关键词”提示。
- 在 JSON 顶层增加 `reviewRequired: true` 或同等字段，提醒它不是自动可外发材料。
- 为诊断包增加 schema 文档和样例 fixture，便于试用者知道哪些字段会出现。
- 增加单独命令或 UI 按钮，生成“无日志、无任务详情”的最小支持包。
