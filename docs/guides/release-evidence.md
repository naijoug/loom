# 发布状态与证据

发布前查阅；普通 Chat 开发不需要完成分发流程。`scripts/check-release-docs.mjs` 检查结构与引用，不替代签名、公证或人工验收，也不执行发布。

## 状态与当前结论

Review 表允许 `Pass`、`Wait`、`Hold`、`Fail`、`Review`，不锁定某一天的状态。保留阻断规则和所需证据说明；历史记录注明日期，不用历史 Hold 阻止有新证据的推进。

Checklist 中只保留一条独立的粗体当前结论：`**Distribution decision: Hold**` 或 `**Distribution decision: Invite-only**`。正文条件说明、代码块模板与历史叙述不代表当前结论。该检查器不处理 Public 发布，公开发布另需下载、撤回、更新等正式审查。

普通文档/环境 gate 可依据已有来源标注状态；Artifact identity、Credential preflight、Notarized DMG、首次启动、诊断包和安装/卸载改为 Pass 时，必须有下面的候选证据。Invite-only 要求表中每一项 Pass，且所有 gate（包括环境和文档复核）都绑定同一候选物。

## 候选证据格式

仅在有真实复核结果时创建 `docs/release/release-evidence.json`。以下为占位模板，不能用作通过证据：

```json
{
  "schemaVersion": 1,
  "candidate": {
    "commit": "<实际 commit>",
    "sha256": "<64 位 SHA-256>",
    "sizeBytes": 0,
    "buildCommand": "<实际构建命令>",
    "artifact": "<候选文件名>"
  },
  "gates": {
    "Credential preflight": {
      "status": "Pass",
      "candidateSha256": "<与 candidate.sha256 相同>",
      "reviewer": "<复核人>",
      "checkedAt": "<ISO 日期时间>",
      "record": "records/<复核记录>.md"
    }
  }
}
```

- 候选物 commit 为 7–40 位十六进制，SHA-256 为 64 位，sizeBytes 为正整数；记录真实 build command 和 artifact 文件名。
- `gates` 的 key 与 checklist Gate 名称完全一致。record 相对 `docs/release/`，必须是真实存在的 Markdown 文件，realpath 不能越出该目录。
- 记录正文包含 gate 名称、相同 commit/SHA-256 和 `Result: Pass`，并附实际命令、结果、日期、平台和人工验收说明。检查器只能核对必要标识；复核人负责确认正文证据足以支持 Pass。
- 重新构建候选物后更新 manifest 和对应证据；不得把旧 checksum 的 Pass 复制成新候选结果。
- 保留历史记录，新增复核结果；模板中的 `pass / hold` 不作为真实 pass。变更 smoke 记录的当前 Pass 摘要时也需要 manifest。
- 不记录密钥、账户凭据或本机绝对路径；命令和证据脱敏后纳入版本控制。

运行 `pnpm docs:release:check` 和 `pnpm docs:release:test`。本次规则优化不改变仓库当前 Hold 结论，不生成任何真实 Pass 证据。
