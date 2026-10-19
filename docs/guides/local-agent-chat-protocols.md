# 本机 Agent Chat 协议探针（2026-09-17）

探针环境：macOS（Asia/Shanghai），Loom 工作区外临时目录。仅记录 CLI 能力，不把登录态或密钥写入仓库。

## 2026-09-20 实际调用补充

后续续聊绑定切片重新探测到 Codex 0.155.0、Claude 2.1.273、Grok 1.0.30。新建/resume 已统一显式权限参数；Chat 使用版本化句柄与配置指纹。真实 Grok 三轮结构化续聊通过，文件探针发现并修复多工具结果丢失；修复后真实重跑因私人 skill 数据传输的自动审批拒绝而待授权，**文件权限矩阵尚未通过**。完整来源、失败和限制见 [续聊验收](../dogfood/2026-09-20-chat-resume.md)。下述旧探针记录保持历史含义。

Grok 1.0.30（04b7ffed98c6）已通过生产 adapter → runtime → 仓储的三轮真实对话：各轮有流式文本，原生 resume 句柄从磁盘恢复，随机标记准确且只出现一次。后两轮重新打开仓储，并确认 adapter 使用原生 resume；不是通过重放历史文本绕过续聊。见 [进程层与真实 canary 记录](../dogfood/2026-09-20-chat-runtime.md)。

首次真实调用暴露旧 adapter 的 `streaming-json` 与 `--include-partial-messages` 不匹配提示，已按当前 `--help` 修正为 `streaming-messages-json` 后验收通过。下表原有 Codex/Claude 项仍只代表 09-17 的 help/version 探针，本轮没有其真实模型调用证据。

本次仅验证无工具的文本/续聊链路：尚未证明 Grok 的文件只读限制、真实工具事件、真实回合停止后再发、应用退出及 Tauri UI；不把 `plan` 参数等同于安全验收通过。

## 2026-09-17 探针基线

| Adapter | 二进制 | 版本 | 流式/JSON | Resume | 权限相关 | 探针结论 |
|---|---|---|---|---|---|---|
| grok | `~/.grok/bin/grok` | 1.0.30 | `--output-format streaming-messages-json` / `json`；`-p/--single` | `--continue` / `--resume` | `--permission-mode`、`--always-approve`、`--allow` | **首选候选**：本机已安装，流式与 resume 旗标齐全 |
| codex | `codex` (npm) | 0.143.0 | `codex exec` 非交互 | `resume` 子命令 | sandbox / approval 配置 | 可用次级；需 `exec` 路径对接 |
| claude | `claude` | 2.1.273 | `-p/--print` + `--output-format=stream-json` | `--resume` / `--continue` | `--dangerously-skip-permissions` 等 | 可用次级；print 模式适合 Chat runtime |

## 能力明细

### grok
- 非交互：`grok -p "…" --output-format streaming-messages-json`
- 权限：`--permission-mode`；探索模式应对齐只读 allow 规则
- Resume：`--continue`（按 cwd）或 `--resume`
- 证据：`--help` / `--version`（未在 CI 跑付费对话）

### codex
- 非交互：`codex exec "…"`
- Resume：`codex resume`
- 证据：`--help` / `--version`

### claude
- 非交互：`claude -p "…" --output-format stream-json`
- Resume：`--resume <id>` / `--continue`
- 证据：`--help` / `--version`

## 首发决定
- **首发 Adapter：grok**（本机路径稳定、流式 JSON 旗标明确）。
- codex / claude：M4 可选；未真实验收前 UI 可展示但应标注能力矩阵结果，不得伪装“已支持 Chat”。
- 付费/登录失败：记 blocked，不在本文件用 fixture 冒充 passed。

脱敏 fixture 见 `tests/fixtures/chat/<adapter>/`。
