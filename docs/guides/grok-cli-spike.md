# Grok CLI spike（M4）

- **Date**: 2026-09-14
- **Binary**: `~/.grok/bin/grok`（`grok 1.0.30`）
- **Decision**: **一等适配** `grok_cli`（不是 custom CLI 模板）

## 契约

| 用途 | 参数 |
|---|---|
| 单轮 headless | `grok -p <prompt> --cwd <project> --output-format streaming-messages-json --include-partial-messages`（2026-09-20 按 1.0.30 help/真实调用修正旧 flag） |
| 只读 | `--permission-mode plan` |
| 可写 | `--permission-mode acceptEdits` |
| 续聊 | `grok --resume <sessionId> …`（Loom 存为 resumeCommand） |
| 长提示 | `--prompt-file <path>`（chat 当前 embed） |

## 注意

- Loom 进程 PATH 需能 `command -v grok`；若桌面端找不到，把 `~/.grok/bin` 加进登录 shell PATH，或在 Settings 把 Agent command 改成绝对路径。
- 未锁定最低版本；以本机已安装二进制为准。
