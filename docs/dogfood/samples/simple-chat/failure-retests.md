# 开发期失败与复验说明

本文件补充原摘要省略的失败背景。原失败摘要继续保留；下表的通过只对应具体复验场景，不表示所有模型请求或原生桌面操作均已通过。

| 场景 | 失败原因与处理 | 最终证据 |
|---|---|---|
| Codex 读取/权限 | 首个测试要求“文件读取工具”并禁止 shell，但该版本 Codex 通过命令工具读取文件，因此没有返回 sentinel 的内容。改为允许只读 cat，并限定编辑使用 apply_patch。属于测试提示词修正；不把首次运行记为通过。 | `codex-permissions.txt`；Code 差异另见 `codex-diff.txt` |
| Claude 模式切换 | 模糊 ToolSearch 未定位内建 Read，转向 Monitor 后超过测试的 120 秒预算。最终场景明确发现并使用内建 Read，再切换 Code 编辑；产品默认回合预算仍为 10 分钟。 | `claude-mode-switch.txt` |
| Codex / OpenCode 停止 | 持久化失败字段记录进程组信号收到 `Operation not permitted`。收口增加“内核明确确认进程组已消失”检查；成功发送 TERM 后不重复发送，发送失败仍允许重试。两种情况都有离线回归，不会将仍存在的权限拒绝吞掉。 | `codex-stop.txt`、`opencode-stop.txt`；失败字段见 `failure-details.json` |
| Grok | 返回 402，使用额度耗尽，同时曾报告登录刷新异常。未购买额度、未修改账户，也未把其它 CLI 的通过替代 Grok 验收。 | `cli-resume.txt`；状态保持 blocked |

`failure-details.json` 仅从合成临时项目的 Chat v2 快照摘取状态、原因和退出码，测试 PID 已替换；没有复制模型正文、凭据或原始 stderr。`sourceAvailable` 标明收尾时是否仍可读取对应临时证据。

停止通过记录覆盖生产仓储/runtime 的取消与再发；Esc 按键本身只有前端/合成网页验证。仓储重开也不等于应用重启。这两项原生交互由用户验收。
