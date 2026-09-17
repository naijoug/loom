# 本机 Agent Chat 协议探针（2026-09-17）

探针环境：macOS（Asia/Shanghai），Loom 工作区外临时目录。仅记录 CLI 能力，不把登录态或密钥写入仓库。

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
