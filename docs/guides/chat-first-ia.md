# Chat-first 信息架构（M0）

- **Related**: [chat-first-prd.md](./chat-first-prd.md) · [chat-contracts.md](../architecture/chat-contracts.md)

## 路由表

| View id | 入口 | 默认？ | 说明 |
|---|---|---|---|
| `chat` | 侧栏主项「对话」 | **是**（M5 起冷启动） | 会话列表 + 消息 + composer |
| `board` | 侧栏 advanced「任务看板」 | 否 | 现有 Board |
| `workspace` / task detail | 从 Board 打开任务 | 否 | Planning / Session / Testing 等 |
| `settings` | 侧栏/齿轮 | 否 | 含 Agent 管理与诊断 |

> M1 可先并存路由；M5 再把默认 `currentView` 改为 `chat`。

## Chat 页布局

```
┌────────────┬──────────────────────────────┐
│ 会话列表    │  当前会话标题 · Agent picker   │
│ · 新建      │──────────────────────────────│
│ · session… │  消息流（user / assistant）    │
│            │                              │
│────────────│──────────────────────────────│
│ Advanced   │  composer（输入 + 发送 + 停止） │
│ · 任务看板  │  [允许写入] 开关 · 升格为任务   │
│ · 设置      │                              │
└────────────┴──────────────────────────────┘
```

## 组件职责（草案）

| 区域 | 职责 |
|---|---|
| SessionList | `chat_list_sessions`；新建 / 选中 / 空态 |
| MessageList | 渲染 messages；streaming 尾气泡；错误摘要 |
| AgentPicker | 读 `list_agents` + diagnostics；切换更新 session.agentId |
| Composer | `chat_send` / `chat_abort`；无 Agent / 流式中禁用发送 |
| AdvancedNav | 链到 Board / Settings，文案标明「高级工作流」 |

## 导航文案（中文）

- 主：对话
- Advanced：任务看板、（任务内）计划 / 实施 / 调试
- 设置：Agent 与权限
