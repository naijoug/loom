# Agent 诊断 P2 修复（2026-11-05）

基线：`main` `7d679fa`。范围：当前进展审查中的三个 P2，以及已核实的文档差异；不扩展 Research、不修改用户配置、不运行收费模型验收。

## 修改

1. 模型来源检查只验证模型列表：HTTP 404/405/501 返回 `model_list_unavailable`，不再归为地址/模型不存在；`ok` 文案只说明列表可访问，成功和未知都注明未测试对话。Settings 按钮明确为检查模型列表，CLI 人读输出同样说明范围。测试不改变 provider 的 `configured` 或 Agent 可选状态，不自动请求对话；API 的 `outcome` 从 `not_found` 改为 `model_list_unavailable`，前后端同步。CLI JSON 的退出语义保持原样，消费方需检查 outcome，详见 [CLI 指南](../agent-cli.md)。
2. `agent_cli` 日志 sink 改用现有 JSON 感知脱敏，保留 usage 和错误字段，嵌套凭据继续脱敏，非 JSON 继续文本脱敏。真实桩进程回归覆盖人读/NDJSON、stdout/stderr、执行记录中的 `stderrTail` 与敏感值清除。
3. 通用 runner 非零退出时优先显示已解析且非空的协议错误，保留 stderr 尾部作为折叠详情、真实退出码和终止原因；没有协议错误则回退退出摘要，零退出但协议报错仍失败。真实本地子进程 fixture 验证上述三种分支。

## 文档差异与处理

| 文档 | 对应实现 | 原差异与本次处理 |
|---|---|---|
| `requirements.md` Discuss 范围 | `src/features/discuss/NewDiscussForm.tsx`、`src-tauri/src/discuss/jobs.rs` | 遗漏无项目直连 API；补充该选项和有项目时禁用的限制 |
| Learn Discover 计划 §2 | `src/features/learn/learn.css` | 写 1100px，CSS 使用 `max-width:1179px`；统一为 <1180px，并同步 PLANS 索引 |
| A7 发现 3 | `src-tauri/src/agent/runner.rs` | 记录称取 stderr 首行；先更正历史实现为通用摘要 + 最后 2000 字符，再记录本轮协议错误优先的修复，不改写历史真实结果 |
| `agent-cli.md` 入口 | `src-tauri/Cargo.toml` | 称 loom-agent 为第二个 bin，实际另有 loom 与 loom-skills；改为独立 bin 并列出另外两个 |
| `release/README.md` 与 `release/app-updates.md` | 文档之间的状态差异，无代码变更 | README 仍称缺少公证凭据；按 10-16 更新记录标为已配置，将旧探针标为历史，保持分发 Hold；未重新访问账号/验证凭据 |

Agent 层契约与 CLI 指南同步本轮模型列表、日志和错误语义。A7 记录链接到本页，历史真实运行结果保留。

## 验证

- Agent 定向 Rust 回归：87 passed / 16 ignored；本地 HTTP fixture 使用获批的回环访问。
- 前端 Agent/provider 定向回归：8 passed。
- 分层检查、发布文档检查通过。
- `pnpm check` 全部通过：前端 160 passed；Rust 441 passed / 39 ignored；含分层、Prettier、发布文档及 checker fixture、提示词一致性、TypeScript/Vite build、Rust fmt 和 Clippy `-D warnings`。共享 runner 改动扩大到完整回归，覆盖 Chat/Learn/Research/Discuss 调用方。
- 浏览器合成验证通过：en / zh-CN × 1280px 浅色 / 700px 深色，共 4 个场景；实际 ProvidersPanel/Hook 使用合成 `ok`、`model_list_unavailable(404)`、`auth_failed(401)` 返回值，检查提示、HTTP 状态与无横向溢出。目视查看英文宽屏与中文窄屏截图；截图及机器结果在本机 `.designs/exports/agent-p2/`（不提交）。启动前后均执行 `scripts/debug.sh stop`。
- 浏览器准备失败如实保留：首次预览进程随工具会话结束退出，改为同会话启动与验证；随后测试选择器误点 Skills，按页签文本更正后通过。均未修改产品代码来绕过验证。
- 修改文档的相对文件链接与 `git diff --check` 通过。完整检查日志：本机 `/private/tmp/loom-p2-check.log`；定向日志 `/private/tmp/loom-p2-agent-tests.log`、`/private/tmp/loom-p2-frontend-tests.log`。

以上为离线 fixture 验证；未重新执行真实 provider、桌面原生 IPC 或签名分发验收。
