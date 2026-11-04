# 模型来源（Providers）与执行器选择 — 布局稿（A5）

来源：[Agent 调用层：无窗口 CLI 与 API Key 模式计划](../../docs/plans/2026-11-02/21:54-loom-agent-cli.md) A5。
状态：2026-11-03 用户确认（“Approve, build it”，第 7 节取本稿默认），已实现（A5，2026-11-03 15:15）。截图将由 `pnpm smoke:visual` 生成到 `designs/exports/agent-providers/`（不入库）。

后端已就绪（A1–A4）：`provider_list` / `provider_save` / `provider_delete` / `provider_test`，`AgentConfig.providerId`，`api` 执行器，`AgentCapabilities.providerKinds`。本稿只定 UI。

## 1. 两个维度

每个 Agent = **执行器**（Codex、Claude Code、OpenCode、Grok、自定义 CLI，或"直连 API"）× **模型来源**（CLI 自带登录，或一个 provider）。

| 执行器 | 可选模型来源 |
|---|---|
| Claude Code | 自带登录 · Anthropic 类 provider |
| Codex | 自带登录 · OpenAI Responses 类 provider |
| OpenCode | 自带登录 · 三类都可 |
| Grok、自定义 CLI | 只有自带登录（下拉禁用并说明） |
| 直连 API | 必须选一个 provider；没有工具，只能纯文本回答 |

## 2. Settings → 新页签"模型来源"

位置：Agents 与 Appearance 之间，图标 `KeyRound`。

```text
┌ 模型来源 ──────────────────────────────────────────── [+ 添加] ┐
│ 给 CLI 换用你自己的 API Key，或不经 CLI 直接调用模型。              │
│ Key 只保存在本机私有文件（0600），不会写进 CLI 的全局配置。          │
├──────────────┬──────────────────┬───────────┬──────────┬────────┤
│ 名称          │ 类型 · 地址        │ 默认模型   │ Key      │        │
├──────────────┼──────────────────┼───────────┼──────────┼────────┤
│ DeepSeek     │ Anthropic 兼容     │ deepseek- │ ● 已保存  │ 编辑    │
│              │ api.deepseek.com/… │ chat      │          │ 测试 删除│
│ 本机网关      │ OpenAI Chat        │ —         │ ○ 本机无需│ 编辑 …  │
│              │ 127.0.0.1:<端口>/v1│           │          │        │
│ OpenAI       │ OpenAI Responses   │ gpt-5.1   │ ● $OPENAI_│ 编辑 …  │
│              │ api.openai.com/v1  │           │   API_KEY │        │
└──────────────┴──────────────────┴───────────┴──────────┴────────┘
  被引用：DeepSeek ← Claude Code（DeepSeek）、API · DeepSeek
```

- Key 列只显示来源：`● 已保存`、`● $变量名`（环境变量优先于已保存的 key）、`○ 本机无需`（本机/内网地址）、`⚠ 缺少 key`。永远不显示 key 本身或其片段。
- "测试"发一次最小请求，结果就地显示在该行下方：`可用` / `Key 无效（HTTP 401）` / `地址或模型不存在` / `限流` / `超时` / `连不上`；不展示上游返回内容。
- "删除"在 provider 被任何 Agent 引用时禁用，悬停说明"先在 Agents 里改掉：Claude Code（DeepSeek）"。后端同样会拒绝。
- 列表为空：一句说明 + 预设按钮组（见下）。

## 3. 添加 / 编辑 provider

点"+ 添加"先选预设，预设只预填字段，用户仍可改。

```text
┌ 添加模型来源 ─────────────────────────────────────────────────┐
│ 预设  [Anthropic] [OpenAI] [OpenRouter] [OpenAI 兼容]          │
│       [Anthropic 兼容] [本机网关]                               │
│                                                                │
│ 名称        [DeepSeek                          ]               │
│ 协议        (•) Anthropic Messages                             │
│             ( ) OpenAI Responses   ( ) OpenAI Chat Completions │
│ Base URL    [https://api.deepseek.com/anthropic ]              │
│             Anthropic 地址不含 /v1；OpenAI 地址包含 /v1。         │
│ 默认模型    [deepseek-chat                      ]（可留空）       │
│ API Key     [••••••••••••••••          ] 👁                    │
│             编辑时：已保存 key  [替换] [清除]                      │
│ 或环境变量  [                                    ]（如 DEEPSEEK_KEY，优先）│
│                                                                │
│                              [取消]  [保存]  [保存并测试]          │
└────────────────────────────────────────────────────────────────┘
```

| 预设 | 协议 | Base URL | Key |
|---|---|---|---|
| Anthropic | Anthropic | `https://api.anthropic.com` | 必填 |
| OpenAI | OpenAI Responses | `https://api.openai.com/v1` | 必填 |
| OpenRouter | OpenAI Chat | `https://openrouter.ai/api/v1` | 必填 |
| OpenAI 兼容 | OpenAI Chat | 空，用户填写 | 必填 |
| Anthropic 兼容 | Anthropic | 空，用户填写 | 必填 |
| 本机网关 | OpenAI Chat | `http://127.0.0.1:` + 用户填写端口 + `/v1` | 可空 |

- Key 输入框是 `type=password`，不回填；编辑已有 provider 时默认"保留"，只有点"替换"才出现输入框，点"清除"需确认。保存后清空前端状态里的 key。
- 校验就地提示（后端为准）：明文 `http` 只允许本机、内网或 `.local`；URL 不能带用户名、查询串；模型名不能有空白或以 `-` 开头。
- 保存冲突（`loom-agent` 或另一窗口刚改过）：横幅"模型来源已在别处修改，已重新加载"，表单保留用户输入但不自动重试。

## 4. Agents 页的改动

### 4.1 列表

"状态"后加一列"模型来源"：`自带登录` 或 provider 名；直连 API 执行器的"路径"列显示 provider 的地址，"能力"列显示"仅文本回答"。

页头按钮：`[重新检测] [+ 自定义 CLI] [+ 直连 API]`。

### 4.2 编辑 CLI Agent

```text
┌ 编辑 Claude Code ─────────────────────────────────────────────┐
│ 名称        [Claude Code                      ]               │
│ 可执行文件  [claude                           ]               │
│ 模型来源    [自带登录                    ▾]                     │
│             ├ 自带登录（使用 claude 自己的账号）                   │
│             ├ DeepSeek · Anthropic 兼容                         │
│             ├ Anthropic · Anthropic                             │
│             ├ ── 不兼容 ──                                       │
│             ├ OpenAI · OpenAI Responses（Claude Code 只接 Anthropic 协议）│
│             └ 管理模型来源…                                      │
│ 说明：选了模型来源后，每次运行临时注入地址、key 和模型，               │
│       不改 ~/.claude 等全局配置；没有默认模型的来源需要在运行时填写模型。 │
└────────────────────────────────────────────────────────────────┘
```

- 不兼容的 provider 列出但禁用，并写明原因；Grok 与自定义 CLI 整个下拉禁用，说明"此执行器不支持注入模型来源"。
- "管理模型来源…"跳到第 2 节的页签。

### 4.3 新建直连 API 执行器

```text
┌ 直连 API ────────────────────────────────────────────────────┐
│ 名称        [API · DeepSeek                   ]               │
│ 模型来源    [DeepSeek · Anthropic 兼容    ▾]（必选）              │
│ 说明：不启动任何 CLI，直接把问题发给模型；没有读文件、改代码、运行命令    │
│       的能力。目前可用于不选项目的 Discuss 和 loom-agent run。         │
└────────────────────────────────────────────────────────────────┘
```

没有可执行文件、参数、写权限开关。名称默认 `API · <provider 名>`。

## 5. 共享 AgentPicker

按钮显示"执行器 · 模型来源"，自带登录时只显示执行器名，保持按钮短：

```text
[● Claude Code · DeepSeek ▾]      [● Codex ▾]      [● API · Anthropic ▾]
```

菜单项第二行小字写模型来源，并按功能是否需要工具置灰：

```text
┌──────────────────────────────────────────────────┐
│ ✓ Claude Code                        2.1.317      │
│   DeepSeek · 结构化流, 工具事件, 原生续聊          │
│   Codex                              0.166.0      │
│   自带登录 · 结构化流, 工具事件, 原生续聊           │
│   API · Anthropic                    直连 API     │  ← 置灰
│   此功能需要读项目或运行工具，直连 API 不支持         │
└──────────────────────────────────────────────────┘
```

- 置灰规则由功能声明（与后端 `needs_tools` 相同）：Chat、Learn、Research 始终置灰 `api` 项；Discuss 只在选了项目时置灰，并在表单上提示"不选项目可使用直连 API"。后端仍会拒绝，前端只是提前说明。
- 模型输入框仍只在 `modelSelection` 为真时显示；选了带默认模型的来源时占位符写"默认：deepseek-chat"。
- provider 缺 key 时该 Agent 的健康点为 `down`，小字"模型来源 DeepSeek 缺少 key"，不可选。

## 6. 窄窗口与主题

- 700px：Providers 表格改为卡片列表（名称+类型一行、地址一行、Key 来源与操作一行），编辑表单单列；预设按钮换行。
- 只用现有 `--chat-*` / settings 令牌；`smoke:visual` 补 Providers 页：en / zh-CN × light / dark × 700px，检查无横向滚动、key 不出现在 DOM。

## 7. 待确认

1. 模型来源放独立页签（本稿），还是作为 Agents 页底部的一节？
2. AgentPicker 按钮在自带登录时只显示执行器名（本稿），还是总显示"· 自带登录"？
3. 预设列表是否够用（第 3 节表格），要不要加别的服务商？
