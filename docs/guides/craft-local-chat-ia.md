# Craft 启发的本机 Agent Chat — 信息架构（Phase 1 / M0）

- **Related**: [craft-local-chat-prd.md](./craft-local-chat-prd.md) · [chat-first-ia.md](./chat-first-ia.md) · [chat-contracts.md](../architecture/chat-contracts.md)
- **Plan**: [09:03-craft-inspired-local-agent-chat.md](../plans/2026-09-17/09:03-craft-inspired-local-agent-chat.md)

## 路由表

| View id | 入口 | 默认？ | 说明 |
|---|---|---|---|
| `chat` | 侧栏主项「对话」 | **是**（冷启动） | Inbox + Transcript + Composer + Agent / 权限 |
| `board` | 侧栏 advanced「任务看板」 | 否 | 现有 Board；**保留不删** |
| `workspace` / task detail | 从 Board 打开任务 | 否 | Planning / Session / Testing 等 |
| `settings` | 侧栏 / 齿轮 | 否 | Agent 管理与诊断（Chat 内可链入摘要） |

> 相对 chat-first IA：主表面区域拆分更细；Board 仍为 advanced，不是第二首页。

## Chat 页布局（线框）

```
┌──────────────┬────────────────────────────────────────────┐
│ Inbox        │  SessionHeader                             │
│ · 新建会话    │  标题 · Agent picker · 权限三档 · 菜单      │
│ · 过滤：      │────────────────────────────────────────────│
│   进行中/归档 │  Transcript                                │
│ · 会话项：    │  · user / assistant 气泡                   │
│   标题/预览/  │  · 可选 parts：text / tool / error 卡片     │
│   Agent/时间  │  · streaming / aborted / error 态          │
│ · 旗标（可后）│                                            │
│──────────────│────────────────────────────────────────────│
│ Advanced     │  Composer                                  │
│ · 任务看板    │  输入 · 发送 · 停止 · 权限提示              │
│ · 设置        │  （升格为任务 / 开新 CLI 会话 → Session 菜单）│
└──────────────┴────────────────────────────────────────────┘
```

可选右侧「上下文」空态（M5）：项目路径、当前权限、Agent 诊断链接；**不**做 MCP 连接。

## 区域职责

| 区域 | 职责 | Phase 1 要点 |
|---|---|---|
| **Inbox** | 会话列表、新建、选中、按 `active` / `archived` 过滤 | status 仅两态；归档 ≠ Board 列 |
| **Transcript** | 渲染 `messages`（及可选 `parts`）；流式尾气泡；错误摘要 | M1 可先纯文本；M2+ 工具卡 |
| **Composer** | `chat_send` / `chat_abort`；无 Agent / 流式中禁用发送 | Enter 发送，Shift+Enter 换行 |
| **Agent picker** | `list_agents` + diagnostics；切换 `agentId` | 不可用时显示诊断摘要（M3 打磨） |
| **Permission** | 三档：`explore` / `ask` / `auto`（文案：探索 / 询问编辑 / 自动） | 写入 session；经映射表进 adapter；Ask **无**审批弹窗 |
| **SessionHeader / 菜单** | 标题、续聊状态、开新 CLI 会话、升格 stub、归档 | 密度对齐 Craft SessionMenu，不抄样式 |
| **AdvancedNav** | 链到 Board / Settings | 文案标明「高级工作流」 |

## 权限控件（产品）

| 档位 | 中文 | UI 行为 Phase 1 |
|---|---|---|
| `explore` | 探索 | 默认；只读 CLI |
| `ask` | 询问编辑 | 与 explore 同级保守 CLI；**无** per-tool modal |
| `auto` | 自动 | 可写 CLI（acceptEdits / workspace-write） |

快捷键循环（若框架允许）：Shift+Tab 在三档间切换（M1）。

旧 checkbox「允许写入」在 M1 壳层过渡期可映射为：关 → `explore`，开 → `ask`（偏安全）；完整三段控件在 Header 落地后移除 checkbox。

## 导航文案（中文）

- 主：对话
- Inbox 过滤：进行中、已归档
- 权限：探索、询问编辑、自动
- Advanced：任务看板；（任务内）计划 / 实施 / 调试
- 设置：Agent 与权限

## 明确不做（IA 层）

- Inbox 不做 Craft 五态工作流列。
- 不做 Sources / MCP 连接面板（仅空态说明）。
- 不做后台任务条产品化。
- 不把 Board 列语义复用到会话收件箱命名上。
