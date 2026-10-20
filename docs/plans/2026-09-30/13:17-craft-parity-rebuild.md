# Loom 按 Craft Agents v0.14.0 完整重建计划

- 创建：2026-09-30 13:17，Asia/Shanghai。
- 状态：**已暂停，被 2026-10-20 简化 CLI Chat 计划取代**。以下为冻结时的历史状态，不再作为当前实施入口。唯一当前状态是第 24 节：M0/M1 未收口，M2 未到出口，M3–M8 未开始；没有里程碑或 F 行是 passed。本文件是重建方案入口，不是完整交付证明。
- Loom 代码基线：`5e90f550065ed00dc09304dc2692b580ebd61a7d`。
- 参考基线：Craft Agents `v0.14.0` / `73bd9c2a3573158bea880984eb8d5fdb41e0cac2`。
- 用户决策：**保留 Tauri + React + Rust，完整复刻功能与交互**。
- [参考包与源码阅读入口](../../ref.md)；[开发规则](../../development.md)；[验证矩阵](../../testing.md#开发变更验证矩阵)。

## 1. 结论与目标

建议进行新架构重建：以 Craft 的产品行为为规格，移植适合复用的 React 表现层，用 Rust 重建领域服务、权限、持久化和执行控制。旧业务运行时不作为新架构的基础，新旧短期并存仅用于开发与数据迁移，验收后一次切换。

这不是继续给旧 Chat 增补按钮，也不意味着立即清空仓库。现有代码中仍有活跃的 Task 路由和 commands，不能未经依赖分析就称为死代码；“不再属于新产品”与“完全无引用”要分别处理。

目标是固定版本的**可观察产品行为等价**：页面、导航、输入、快捷键、流式响应、工具、权限、状态、持久化、恢复以及外部集成都能对照验收。实现语言、内部文件格式和框架不要求相同。模型的自然语言答案具有随机性，验收事件、工具调用与产物语义，不逐字比较回答。

已评估但不采用 Electron fork：它能更多直接复用上游运行时，但用户明确选择 Tauri/Rust。保留 Tauri 会增加 SDK 桥接、浏览器与多平台验证成本，必须先验证，再扩大实施。

## 2. 当前状态与证据边界

### 本轮已完成

- 使用 ng-tool-ref 的初始化脚本检查 `.ref/` 和忽略规则。
- 更新已有 Craft clone，固定 v0.14.0；旧 v0.9.3 保存到参考仓库 `ref-snapshot-2026-05-13` 分支。
- 阅读启动、RPC、SessionManager、AgentBackend、权限、存储、Sources、Skills、浏览器、Pages、DAG、Automation、Messaging 与设置入口。
- 更新 `docs/ref.md`，保留原参考包；创建本计划并同步计划索引。

### 当前 Loom（源码/契约盘点，不是本轮运行验收）

| 已有代码 | 证据入口 | 对重建的意义 |
| --- | --- | --- |
| Chat/Workflow 双路由 | `src/App.tsx`、`src/features/tasks/WorkflowShell.tsx` | 已拆 Shell，但旧产品和新 Chat 并存，不能以此作为新领域模型。 |
| 项目关联的 Chat、简单 text/tool/error parts | `src-tauri/src/chat/models.rs` | 缺 Workspace、丰富工具身份与其他资源模型，需要新 schema。 |
| v2 journal、幂等回执、seq、导出、进程停止 | `src-tauri/src/chat/`、`process_supervisor.rs` | 有复用价值的实现与行为测试候选，需脱离旧模型审查。 |
| CLI adapter + stdout 解析 | `agent_adapter.rs`、`agents/stream.rs`、`chat/runtime.rs` | 不是 Craft SDK 后端的等价实现。 |
| Ask 为回合授权 | `docs/architecture/chat-contracts.md` | 必须重建逐工具审批，不能复用名称冒充功能。 |
| 全局 reducer/context、旧 Board/Planning/Review/Testing | `src/state/`、`src-tauri/src/lib.rs` | 新应用不再引入这些业务依赖。 |

本轮开始前已有用户改动：`docs/dogfood/2026-09-24-chat-reliability-test-map.md`、`docs/dogfood/chat-desktop-acceptance-template.md`。保持原样，不加入迁移或清理。

### 未完成且不能宣称完成

规划时上述工作均未运行。实施更新：参考桌面已构建/启动，完成首批截图；真实 Claude/Pi SDK + 本地模型 fixture 的单写文件审批 10/10 通过。BrowserHost、全部交互基线、外部账户与分享服务仍未验收，新 Loom 尚未交付。详见 [M0 实施记录](../../dogfood/craft-v014/m0-progress.md)，不能把局部 PoC 当作完整复刻完成。当前残差只以第 24 节为准，本节不另作进度来源。

需求、架构、Chat 契约与 AGENTS 已加入新重建范围，并将旧实现标为迁移/回归背景。后续详细技术决策仍需由 M0 的验证结果固化。

## 3. 范围与成功标准

### 一比一的定义

- 基线只认固定 SHA 的可用功能；M0 启动参考应用后补齐菜单、设置、空态、错误态、禁用态和隐藏入口。官网动态文档是解释材料。
- 保留 Loom 名称、图标、bundle ID 和独立服务端点；这些是明确的品牌差异，布局与交互仍对齐。
- macOS 首先实现与验收；最终逐项验证 Windows/Linux，未测平台不得写通过。
- 对上游存在但依赖凭据的功能，标记 blocked，保留交付工作；对仅有 schema、尚未执行的控制流，标记 upstream-unimplemented，不虚构需求完成。
- 开发阶段可分批启用；最后交付必须逐行核销完整范围，不能以“Chat MVP 完成”代表完整复刻。

### 非目标

- 不继续扩张旧 Loom 固定四阶段开发编排，不将其强塞进 Craft 的 Task DAG。
- 不同时维护 Electron 与 Tauri 两套产品，不追逐基线之后所有上游更新。
- 不复刻上游内部 bug、未实现的预留节点、品牌身份或不可取得的第三方服务所有权。
- 不要求旧运行中的 CLI 会话跨新架构无缝接管；旧记录可读，无法可靠迁移的续聊句柄明确重新建立。

### 最终成功标准

1. 下列 F01–F23 及 M0 补充项都有源码入口、可执行案例、实际结果和证据路径；所有 in-scope 项为 passed，或由用户明确批准差异。blocked 不能计为 passed。
2. 不再依赖旧 WorkflowShell、旧 Task reducer、旧规划/Review/Testing commands；旧数据导入器是唯一允许的兼容边界。
3. UI 关键路径与参考截图/交互一致；主题、窗口尺寸、键盘、中文 IME、焦点、滚动和状态恢复可验证。
4. 真正完成 SDK 工具审批、Sources 认证和调用、浏览器操作、自动化、Task、Page、远程、渠道和分享闭环。
5. 在同一候选产物上通过安装、重启、升级迁移、回退、平台相关检查与发布门禁。

## 4. 功能对齐矩阵

状态只允许 `not-tested`、`partial`、`blocked`、`passed`。下表是 2026-10-10 的验收账，不是最初的 planned 清单。没有任何行是 `passed`。`partial` 只表示已有代码和局部证据，不能抵扣该行出口。正常/异常/恢复子案例仍未拆齐，这本身记在第 24 节，不在本表虚构。

表中 `renderer` 指上游 `apps/electron/src/renderer/`；`shared` 指 `packages/shared/src/`；`server` 指 `packages/server-core/src/`。

| ID | 功能与关键行为 | 上游定位 | 主要里程碑 / 最低验收样例 | status | evidence |
| --- | --- | --- | --- | --- | --- |
| F01 | 首次启动、连接选择、登录/重认证、引导 | `renderer/components/onboarding/` | M2–3：空配置启动→连接测试→Workspace→首次会话；取消/失败可恢复。 | partial | [连接切片](../../dogfood/craft-v014/m2-connections-progress.md)与[原生补验](../../dogfood/craft-v014/m2-http-native-progress.md)：目录、Keychain、保存/留空保留/重启已有证据。登录、重认证和引导未做。 |
| F02 | Workspace 与 Project 管理，本地/远程切换、默认值 | `shared/workspaces/`、`shared/projects/` | M1/M3/M7：可无 Project 聊天；切换不串配置、草稿和资源。 | partial | [内核](../../dogfood/craft-v014/m1-core-progress.md)、[桌面](../../dogfood/craft-v014/m1-desktop-progress.md)：可无 Project。远程切换未做。 |
| F03 | 多列 Shell、导航历史、窗口/分栏、搜索与命令入口 | `renderer/components/app-shell/`、`contexts/NavigationContext.tsx` | M3：分栏尺寸、前进后退、多窗口、空态、持久化与快捷键。 | not-tested | 无多列 Shell、导航历史或分栏证据。调试开关下的基础 Shell 不计入。 |
| F04 | Inbox、会话状态、归档、旗标、标签/值、筛选、未读 | `SessionList.tsx`、`shared/statuses/`、`shared/labels/` | M3：多窗口状态一致；后台结束出现未读；重启与过滤结果一致。 | partial | [桌面接线](../../dogfood/craft-v014/m1-desktop-progress.md)：后台完成未读、标记、归档及旗标/归档重启恢复已验证。[归档证据](../../../designs/workbench-m1/02-archive-notes.ax.txt)、[重启证据](../../../designs/workbench-m1/03-restart-restored.ax.txt)。标签/值、完整筛选行为和多窗口状态一致性仍未验收。 |
| F05 | Composer、草稿、附件、来源/技能引用、模型/thinking | `input/FreeFormInput.tsx` | M3–4：拖放/粘贴/引用真实生效；IME、发送键、模式循环按参考测试。 | partial | [桌面接线](../../dogfood/craft-v014/m1-desktop-progress.md)：按会话草稿。`tests/unit/workbenchApp.test.cjs` 覆盖 IME Enter 不发送，不是原生 IME 验收。附件、引用和 thinking 未做。 |
| F06 | 流式文本/thinking/tool、停止、排队/steering、重试 | `server/sessions/SessionManager.ts`、`shared/agent/backend/types.ts` | M2–3：三轮追问、工具卡片、生成中输入、停止后再发、会话切换。 | partial | [运行桥](../../dogfood/craft-v014/m2-core-bridge-progress.md)、[宿主调度](../../dogfood/craft-v014/m2-desktop-sdk-progress.md)：队列、停止、取消和跨会话已测。[原生 Claude](../../dogfood/craft-v014/m2-http-native-progress.md)拒绝/停止/再发/允许/重启已测；Claude/Pi 本机 HTTP 10/10。steer、重试和真实模型账户未测。 |
| F07 | 历史、搜索、分支/压缩、笔记、导入导出、跨工作区转移 | `shared/sessions/`、`server/handlers/rpc/{sessions,transfer}.ts` | M1/M3/M7：长历史冷启动、断线补流、分支隔离、导入幂等、损坏记录可诊断。 | partial | [导入](../../dogfood/craft-v014/m1-legacy-import-progress.md)：macOS 导入、幂等和源离线可读。搜索、分支、压缩、导出包和跨工作区转移未做。 |
| F08 | 多 LLM connections、模型能力与账户、API/OAuth/自定义端点 | `shared/config/llm-connections.ts`、`agent/backend/`、`shared/auth/` | M2/M7：Claude 与 Pi 家族均通过；每种基线认证/端点能力独立核验。 | partial | [连接](../../dogfood/craft-v014/m2-connections-progress.md)、[worker](../../dogfood/craft-v014/m2-worker-progress.md)：api key/bearer/none 与 Pi 注入 fetch 为 partial。[新 worker 本机 HTTP 与原生 Claude](../../dogfood/craft-v014/m2-http-native-progress.md)已有运行证据，先前环境阻塞解除；真实账户/TLS/OAuth 仍未测。 |
| F09 | Explore/Ask/Execute、可选 Guarded、分层权限、记忆审批 | `shared/agent/{mode-types,mode-manager,permissions-config}.ts` | M0/M2/M6：真实工具执行前允许/拒绝；过期/跨回合响应不能放行。 | partial | [审批](../../dogfood/craft-v014/m2-approval-progress.md)、[Shell](../../dogfood/craft-v014/m2-shell-policy-progress.md)、[策略](../../dogfood/craft-v014/m2-tool-policy-progress.md)、[配置](../../dogfood/craft-v014/m2-permissions-store-progress.md)、[记住](../../dogfood/craft-v014/m2-remember-progress.md)。真实 Guarded、管理员审批和热更新未做，本行不是 passed。 |
| F10 | Sources：MCP stdio/HTTP/SSE、REST、本地文件、OAuth/续期 | `shared/sources/`、`server/handlers/rpc/sources.ts` | M4：三种传输、凭据过期恢复、Source 级权限、启停与热更新。 | not-tested | 无 MCP、REST 或本地 Source 的连接和调用证据。权限文档编辑不计入。 |
| F11 | Skills：多层发现、导入/创建、详情、requiredSources、显式调用 | `shared/skills/`、`renderer/pages/SkillInfoPage.tsx` | M4：只配置必需上下文；同名覆盖按参考优先级；引用激活依赖 Source。 | not-tested | 无产品实现或运行证据。 |
| F12 | 内置 session 工具、Agent 修改配置、认证/计划交互 | `packages/session-tools-core/src/tool-defs.ts` | M2/M4–6：catalog 逐项映射；UI 和工具变更共用校验与持久化。 | not-tested | 无 session tool catalog 逐项映射证据。工具分类不能代替可调用工具。 |
| F13 | 富内容与查看器：Markdown/代码/Mermaid/表格/图像/PDF/Office/HTML/diff | `packages/ui/src/components/overlay/`、`apps/electron/resources/` | M5：转换、缩放、复制、定位、差异展示；加载失败和不可信 HTML 隔离。 | not-tested | 无产品实现或运行证据。 |
| F14 | 内置 Browser：窗口/标签、导航、登录态、AX/CDP、下载/网络 | `apps/electron/src/main/browser-pane-manager.ts`、`browser-cdp.ts` | M0/M5：相同窗口操作、后台保留、切 Workspace 隔离、真实页面点击/填表。 | not-tested | [M0 探针](../../dogfood/craft-v014/browser-probe-results.json) 只证明 CEF 原型。产品窗口、多 Workspace、下载和签名未交付，不把本行改为 partial。 |
| F15 | Pages：创建/编辑、静态/交互/live、数据、定时刷新、授权 | `shared/pages/`、`server/pages/`、`renderer/components/pages/` | M5–6：数据更新可见；内容变化使旧授权失效；刷新失败有记录。 | not-tested | 无产品实现或运行证据。 |
| F16 | 后台任务、子会话、消息、父子关系、进度和输出 | `server/sessions/`、`shared/agent/spawn-session-tool.ts` | M6：创建/结束/失败/孤儿恢复；父子权限不提升，跨会话消息精确送达。 | not-tested | 无产品实现或运行证据。 |
| F17 | Task 看板、task.yaml DAG、并发/依赖、暂停恢复、验证/修复 | `server/tasks/TaskRunner.ts`、`shared/tasks/`、`app-shell/kanban/` | M6：有依赖的 DAG、失败节点重试、停止/重启；不执行上游仅预留节点。 | not-tested | 无产品实现或运行证据。 |
| F18 | Automations：时间/事件、prompt/script/webhook、历史/重试 | `shared/automations/` | M6：时区/重复事件/超时/重启测试；触发来源与执行证据可追溯。 | not-tested | 无产品实现或运行证据。 |
| F19 | 可选 Decision layer 与相关 AI 设置 | `shared/decisions/`、`server/decisions/` | M6：开/关、不可用、低置信度回退；每项功能核对，不允许模型提升权限。 | not-tested | 无产品实现或运行证据。 |
| F20 | Remote/headless、Web UI、CLI、混合工作区和本地能力桥 | `packages/server/`、`server/transport/`、`apps/{webui,cli}/` | M7：认证、断线重连、跨客户端事件、远程请求本机浏览器、能力缺失。 | not-tested | 无产品实现或运行证据。 |
| F21 | Messaging：Telegram/WhatsApp/Lark、pairing、绑定/准入/回传 | `packages/messaging-gateway/`、`messaging-whatsapp-worker/` | M7：专用测试账户往返；未授权发送者被拒；去重/解绑/重启可靠。 | not-tested | 无产品实现或运行证据。 |
| F22 | 会话/Page 分享、更新/撤销、Viewer、密码保护 | `server/sessions/SessionManager.ts`、`shared/pages/publisher.ts`、`apps/viewer/` | M7：自己的服务端点上创建/读取/更新/撤销，离线导出不能替代。 | not-tested | 无产品实现或运行证据。 |
| F23 | 所有设置域、主题/语言、快捷键、通知、深链、更新与打包 | `renderer/pages/settings/`、`apps/electron/src/main/` | M3/M8：逐页功能矩阵；真实持久化、系统集成及三平台产物。 | not-tested | 权限页和连接表单只作为 F08/F09 的证据。主题、语言、快捷键、通知、深链、更新和三平台打包未验收。 |

## 5. 架构方案

### 5.1 模块与部署

建议最终布局如下（是目标布局，M1 前不存在）：

```text
apps/desktop/                  Tauri 壳、窗口、系统集成、React 入口
apps/web/                      共用 UI 的远程客户端
apps/cli/                      共用协议的 CLI
crates/loom-domain/            DTO、ID、状态、能力与领域约束
crates/loom-core/              会话/Workspace/资源/Task/Automation 服务
crates/loom-storage/           journal、索引、快照、迁移、附件
crates/loom-runtime/           Adapter、Supervisor、流、SDK worker 协议
crates/loom-policy/            工具前校验、审批、路径/网络边界、审计
crates/loom-integrations/      MCP/API/Auth/Browser/文档转换/消息渠道
crates/loom-server/            headless、认证、WebSocket、HTTP/分享适配
packages/protocol/             生成的 TS 契约、校验 schema、公共 fixture
packages/ui/                   tokens、纯 React、富内容、HostAPI 接口
packages/provider-workers/     仅必要的 Claude/Pi SDK 适配进程
```

阶段内可先保持少量 crate、按模块组织，再按真正部署边界拆包；不能为目录整齐先生成大量空架构。新源码以独立入口构建，不从旧 UI/store/task service 导入。

```text
React → typed HostAPI → Tauri IPC / authenticated WS
                         ↓
                  Rust Application Services
                  ↙         ↓           ↘
           SessionStore   Runtime     Resource Services
                            ↓
                   Policy / ApprovalBroker
                            ↓
                 Provider worker / tool executor
```

Tauri 壳调用与 headless 调用共用服务；core 不依赖 Tauri。保留 Rust 对进程、文件、权限、日志和持久化的控制，不把 TypeScript server-core 整体藏进 sidecar。

### 5.2 Provider worker 的边界与可行性门

Claude/Pi 的 SDK 是 TypeScript 生态；保留 Rust 不等于可以假设 SDK 原生提供 Rust API。建议最小 worker 仅承载 SDK 会话和事件适配，由 Rust 启动、停止、隔离环境并监督。

- 版本化双向 NDJSON/RPC：`initialize/capabilities/startTurn/interrupt/steer/compact/respondApproval/shutdown`，事件包含 `textDelta/toolCall/toolResult/approvalRequested/usage/error/completed`。
- 每个请求与事件绑定 `workspaceId/sessionId/turnId/requestId/toolCallId`；stdout 只传协议，stderr 为脱敏诊断，设置长度上限、背压、超时和取消。
- Loom 的状态、Source 凭据、持久化、调度、审计由 Rust 持有；worker 不直接维护第二套 Workspace/Session 数据库。SDK 必需的私有缓存目录独立声明。
- 所有可控制工具必须在执行前经 Rust policy；可替换工具优先 Rust handler，SDK 原生工具须有不可绕过的 pre-tool hook 和执行绑定。provider 故障时不以自动重跑掩盖副作用。
- 若某 SDK 工具无法按上述边界管理，M0 必须记录冲突并改用受控工具实现/更合适协议，**不得绕过 Rust、伪造审批或把不支持能力显示为已支持**。
- CLI adapter 可作为另一个可选 backend，能力不足明确显示；它不能成为 Craft 全功能能力的默认替代。

这是技术方案而非已证明可行的承诺。M0 的真实写文件/拒绝/停止/续聊实验决定详细实现。

### 5.3 领域与权限契约

- `Workspace` 拥有 Sources/Skills/标签/状态/Pages/Tasks/Automations；`Project` 是可选的 cwd/上下文关联。
- `Session` 拥有消息、notes、provider 配置、资源选择、状态、归档/未读、父子关系；业务状态与执行状态分开。
- `Turn` 与 `ToolCall` 有稳定 ID、明确终态和关联；`PendingApproval` 与具体工具、参数摘要、cwd、有效策略、有效期绑定。
- `TaskSpec/TaskRun` 是子会话 DAG；`Automation` 是触发与执行规则，二者不复用旧 Loom Task 阶段枚举。
- Rust 保存审批决定后再回复工具；取消、超时、旧回合响应、重复点击、客户端断线、应用退出均有测试。
- 按上游还原规则优先级与模式语义；OS sandbox 是另一个明确声明的能力。Guarded 失败回退遵循经核验的产品契约，不能被误述成强隔离。
- schema 是单一来源，生成 TS 类型并以运行时校验约束 IPC/WS；前端不能传任意程序/路径来绕过服务层。

### 5.4 持久化与前端投影

默认继续采用**版本化追加事件 journal + 原子快照 + 可重建索引**，保留现有正确机制但使用新领域 schema。无需为仿上游文件扩展名而放弃 Loom 的幂等/恢复设计。

- 同一 Session 单 writer；一个活动 turn，中途输入按 queue/steer 规则处理。
- seq 与 clientRequestId 支持补流和去重；持久化成功先于对外成功通知，失败留下可诊断终态。
- 列表只读轻量元数据，消息/附件按需加载；派生索引不是第二个事实源。
- React 用按会话订阅的 store，隔离元数据/转录/草稿，避免全局 Context 触发整棵应用流式更新。具体选库在移植依赖审计后决定，不先重写纯 UI。
- Agent 修改配置、UI 操作、远程客户端共用服务与文件监控；同时写入检测版本冲突，不互相覆盖。

### 5.5 浏览器是前置风险，不放到最后

Craft 使用 Chromium/CDP；Tauri 在 macOS 使用 WebKit。不能把 Tauri WebView 当作等价 Chromium，也不能以外部 Safari/Chrome 打开 URL 就声称复刻完成。

M0 验证候选是 **Rust BrowserHost 管理的 Chromium helper/引擎及原生窗口**，与 Tauri 应用桥接；比较可嵌入引擎与独立受控窗口方案，最终选择须覆盖参考窗口/工具栏、显示隐藏、焦点、登录态、下载、AX/CDP、跨 Workspace 归属及打包签名。参考是独立浏览器窗口的行为，不先假定必须嵌进某个面板。

PoC 必须验证会话窗口关闭后隐藏保留、多个 Workspace、Agent 调用和人工操作共存、远程能力桥，以及 helper 崩溃回收。若仍有交互差异，列出差异与成本，不能未经用户同意缩小“一比一”目标。

## 6. 旧代码与数据的退出方案

| 处置 | 具体范围 | 条件 |
| --- | --- | --- |
| 作为历史保留，不继续扩张 | 旧 Board/Planning/Review/Testing、任务阶段模型、旧设计和 dogfood | 新路线由文档明确；历史报告带原基线，不覆盖新计划。 |
| 审查后抽取 | process_supervisor、原子写、脱敏、路径校验、幂等/seq、相关行为测试 | 无旧 Task 依赖，符合新契约，并通过新生产路径测试。 |
| 重建 | AppStateContext/reducer、ChatSession/parts、provider 调用、权限 UI/协议、Settings | 完整能力与边界不兼容旧模型。 |
| 最终移除 | 旧路由、commands、store、旧专用依赖/测试/打包入口 | M8 完成引用扫描、真实切换、旧数据读取验证之后。 |

实施第一步记录 Git 基线与现有 dirty files；不用 `reset --hard`、`git clean` 或整目录覆盖。使用 `codex/craft-parity-rebuild` 类独立分支逐里程碑交付；若当前目录不宜并行修改，再创建隔离 checkout。该步骤仅在开始实施时执行。

新领域数据使用独立 schema 与目录（建议 app data 下 `workspaces/<id>/`，最终在 M1 契约固化），旧 `<project>/.loom/` 只读。

迁移器需要 dry-run 报告、来源版本/hash、ID 映射、缺失附件/损坏记录清单和幂等标记。迁移先写 staging，再原子发布；重复执行不复制出多个会话。旧 status、权限与 provider 不直接按字符串等同转换：旧 Ask 表示回合授权，转成新 Ask 后也不能继承旧授权或不兼容续聊句柄。

旧 Task 日志/计划/Review 以历史记录或导出包保留，不伪造为已跑过的新 DAG。回退启动旧构建读取未修改的旧目录，新版新增数据留在新目录；不保证旧版可读取新 schema。切换前保存双方导出与校验清单，说明回退后新增记录的访问方式。

## 7. 里程碑与可 Review 产物

以下工作按依赖实施，不隐含授权创建子 Agent。每个里程碑可以拆成数个可独立 Review 的 PR；同轮更新本计划和索引状态。

### M0 — 固定产品规格与高风险验证

依赖：本轮参考包已准备。

1. 用隔离的测试配置运行固定版本 Craft，逐页枚举导航、菜单、settings、tool catalog、provider/认证与功能开关。任何真实账户调用单独记录凭据条件和证据。
2. 建立完整 F01–F23 子案例及截图基线，放 `designs/craft-v0.14.0/`；收集窗口尺寸、字体、主题、焦点、滚动和异常态。
3. SDK 双向审批 PoC：工具可见→请求审批→拒绝无副作用→允许执行→停止→恢复；覆盖 Claude/Pi，不只测文本。
4. BrowserHost PoC：受控窗口、CDP/AX、输入/焦点、cookie、显示隐藏、崩溃、安装包；做平台差异表。
5. 托管服务盘点：分享/Page publish、OAuth relay、更新/遥测/反馈、决策服务；标出源码位置、账户、可替换 endpoint、需新建服务。
6. 写架构决策，更新 requirements、architecture、Chat contracts 和 AGENTS 的当前范围；只修改过时产品约定，不削弱安全/数据边界。

产物：行为规格、截图、完整 inventory、两项 PoC 的执行记录、服务清单、确定的依赖/模块边界。

出口：SDK 和 Browser 的关键风险有实际证据或具体未解问题；失败不进入大规模 UI 移植。其余盘点工作继续，无需为普通可逆实施反复确认。

### M1 — 干净内核与数据闭环

依赖：M0 的领域/平台决策。

1. 建立新入口与最小 core/storage/protocol 模块，Workspace/Project/Session/Turn/ToolCall 等 schema。
2. 实现 repository、seq、幂等、恢复、日志、事件订阅/补流和快照；保持 Tauri 宿主与 core 解耦。
3. 建新数据目录与旧 Chat/Task 只读导入器，完成 dry-run、重复导入、附件缺失及回退 fixture。
4. 新前端 store + HostAPI 仅依赖新契约；服务层使用生产实现做协议回归。

产物：能创建 Workspace/Session、发送 fixture 事件、重启恢复的新应用骨架；迁移报告。

出口：重复发送、乱序/遗漏事件、写盘失败、进程退出、跨会话和迁移失败都有实际测试，旧目录未修改。

### M2 — 真实 Agent 与执行权限

依赖：M1、M0 SDK PoC。

1. 接入 Claude 与 Pi provider，凭据 vault、连接测试、模型/thinking 能力、resume/compact/steer、usage 和错误映射。
2. 实现 policy、ApprovalBroker、基础 session tools、进程组回收与运行审计；认证/计划请求使用同一双向交互框架。
3. 覆盖支持的 API/OAuth/自定义端点类型，缺凭据项单独 blocked，不以一个 CLI 成功替代整个 provider 家族。

产物：真实 Agent 三轮对话和工具执行、拒绝、停止、恢复的录屏/事件/脱敏日志。

出口：审批前无工具副作用，错误不自动重跑副作用；两类后端能力分别实测。不支持的 provider 特性保持明确未通过。

### M3 — 桌面交互与会话工作台

依赖：M1–M2。

1. 移植并拆分 Shell/Sidebar/SessionList/Chat/Composer/PanelStack，剥离 Node/Electron import。
2. 实现 Inbox/status/labels/filter/unread/search、草稿/排队/steering、notes/branch/compact、窗口导航与快捷键。
3. 对齐 onboarding、Workspace/Project、AI/基础 settings、主题语言、通知、深链和焦点逻辑。
4. 建立相同数据/窗口/主题的自动截图及真实桌面交互记录。

产物：日常完整会话路径可用且可对照的桌面应用；不是静态 UI mock。

出口：流式期间切会话、多窗口、IME、快捷键、长历史和错误态均通过；Source/内容相关项仍按后续里程碑记录。

### M4 — Sources、Skills 与 Agent 自配置

依赖：M2–M3。

1. MCP stdio/HTTP/SSE，API/本地 Source、连接测试、auth/refresh、分层规则、调用取消与诊断。
2. Skill 发现、导入/编辑、详情、显式引用、requiredSources、上下文注入及热更新。
3. 对照 session tool catalog，实现配置校验、Source/Skill 管理及认证交互；共享 schema、服务、watcher。

产物：真实 Source 工具调用与 Skill 激活记录、设置一致性测试。

出口：至少覆盖全部传输/认证机制案例；过期 token、断线、子进程退出、配置并发和符号链接越界有测试。真实服务项目按 inventory 核验。

### M5 — 文件内容、Browser 与 Pages

依赖：M4、M0 Browser PoC。

1. 接入附件/转换管线与所有富内容查看器，多文件 diff、文件定位与错误反馈。
2. 交付 BrowserHost、工具栏/标签、Cookie/下载、AX/CDP、人工与 Agent 协作和清理。
3. Pages 的文件/数据/HTML sandbox、grant/lease、内容变更失效；授权 action 经 Rust 执行，配置与数据变更通知到 UI。

产物：标准图像/PDF/Office/HTML/diff fixture、浏览器真实操作、Page 数据和权限案例。

出口：功能与窗口体验可对照；不可信页面不能取得 HostAPI/凭据，Page 变更不能继承过期授权；重启可恢复资源。

### M6 — 后台工作、DAG、自动化与决策

依赖：M4–M5。

1. 后台任务/子会话/消息/父子状态与输出；原生运行和 UI 状态分开。
2. TaskSpec/TaskRun、看板、DAG ready 调度、并发上限、暂停/停止/恢复、验证/修复；按上游实际执行节点类型实现。
3. Automation 时间/事件、prompt/script/webhook、重试/历史/循环防护，Page refresh 复用调度。
4. Decision providers、记录、相关开关和逐项功能；Guarded 及失败回退保持明确语义，不替代确定性权限。

产物：依赖 DAG、失败修复、重启、跨会话消息、定时/事件/Page 刷新的可重复案例。

出口：不重复执行已完成副作用；失败/取消不会继续派生任务；重启恢复、时区和重复事件可预测。

### M7 — 远程与外部服务闭环

依赖：M6；协议边界从 M1 已保留。

1. Rust headless + WS/HTTP，认证、版本握手、重连、隔离；共用 React Web UI 与 CLI。
2. 本地/远程工作区混用、导入/导出/转移、浏览器 host capability；远程身份不能借助客户端越权。
3. Telegram/WhatsApp/Lark 渠道、准入、pairing、消息/事件去重、解绑与恢复。
4. 自有 Session/Page 分享服务及 Viewer、密码/更新/撤销；OAuth relay 与反馈/更新端点按 M0 清单替换或部署。

产物：两客户端+headless 的断线恢复记录，专用测试渠道消息往返和自有分享 URL 生命周期记录。

出口：F20–F22 在实际服务上验收；fixture 只证明协议，不证明真实 OAuth 或渠道已通。

### M8 — 一次切换、旧架构退出与发布

依赖：全部范围项完成，外部阻塞已解决或用户明确批准差异。

1. 新入口成为默认构建，更新 scripts/debug.sh 与 CI；所有预览仍使用该脚本、端口 1420，启动前和结束后执行 stop。
2. 按依赖扫描删除旧 App/Workflow/store/commands 与专用依赖，保留必要的只读导入器和历史证据。删除有单独 Review 清单。
3. 迁移 dry-run→真实导入→重启→回退全流程；导入前后清点记录、附件、失败与不兼容 resume。
4. 完成功能/视觉/性能/系统集成矩阵及三平台验证；配置独立更新源，检查同一候选产物的签名/公证/安装与升级。
5. 回填文档、完成矩阵、发布说明与剩余差异；不能靠删掉失败测试通过门禁。

产物：新默认应用、旧架构退出证明、迁移/回退报告、同一候选的发布证据。

出口：完整验收矩阵核销，旧业务不再进入运行路径，干净机器可安装运行。

## 8. 验证策略

- 本轮文档变更：相对链接、计划索引、Markdown、状态/需求一致性、引用路径、Git diff；不运行与文档无关的全量应用测试。
- 实施局部变更按验证矩阵运行相关测试；每个里程碑保留完整门禁。目录迁移后同步 `pnpm check` 和桌面构建脚本到新路径，使 `pnpm check`、`pnpm tauri build --no-bundle` 或已记录的等价入口持续有效。
- 协议测试必须经过生产 wrapper/service；fixture 覆盖乱序、重复、丢包、磁盘失败、审批迟到、取消与子进程故障。
- 真实 provider、MCP/OAuth、桌面窗口、渠道与部署必须用实际环境；无凭据写 blocked，未运行写 not-tested。
- 视觉：M0 同尺寸/主题/字体截图，建立遮罩（时间/随机输出）、组件与整页对比；阈值以参考重复截图的自然波动校准，交互/focus 单独检查，不能仅用像素比例代替。
- 性能：测参考与新实现的冷启动、会话打开、长历史、流式 UI、空闲内存；M0 记录数据规模与硬件，再定预算。此时不写无测量依据的性能保证。
- 日志包含 app commit、上游 SHA、OS、provider/worker 版本、cwd/权限、关键 request IDs 和退出状态；敏感信息脱敏。

## 9. 成本、风险与缓解

这是产品重建，范围远大于聊天界面。初步预算为 **25–40 人周工程量**，不含外部服务等待；属于源码盘点后的粗估，可信度低，M0 后按实际案例和 PoC 重新估算，不作为工期承诺。首个真实会话版本只对应 M1–M3，不能称完整复刻完成。

| 风险 | 影响 | 缓解/完成证据 |
| --- | --- | --- |
| SDK 无法完整拦截工具副作用 | Rust 权限边界失效 | M0 先验证；不可证明的能力不进入正式适配器。 |
| Tauri/WebKit 与 Chromium 不同 | 浏览器与富内容交互、打包成本增加 | BrowserHost PoC 前置；不以外部打开 URL 降级冒充等价。 |
| 托管 API/OAuth relay 未提供可部署实现 | 分享和登录无法闭环 | 建完整服务清单，自建/替换，独立集成验收。 |
| 上游 main 快速变化、README 落后源码 | 无法稳定验收，遗漏 Pages/Guarded/Task 等 | 固定 SHA + 启动实查；更新作为下一基线。 |
| 新老数据与权限语义不一致 | 记录丢失或权限提升 | 只读导入、字段映射、原目录保留、旧凭据/授权不继承。 |
| 机械移植巨大 manager | 重复形成遗留架构 | 按领域拆服务、协议驱动；薄宿主，独立 provider worker。 |
| 太早重构所有组件/更换所有库 | 扩大无关工作量 | 复用纯 React 与行为 fixture，仅替换实际平台耦合。 |
| 三平台验证、后台 helper 和更新源 | 本机能跑但不能分发 | 平台 spike 和签名构建提前；正式交付逐平台核销。 |

## 10. 待确认问题与下一步

产品方向无阻塞问题：已确认 Tauri/Rust 和完整复刻。SDK/browser/services 的未决事项是 M0 的调查与验证任务，不能提前要求用户替工程实现做猜测。

本计划默认允许“Rust 管理的最小 SDK/helper 进程”，因为 Agent 与浏览器运行时本身就是外部进程；Loom 领域控制仍在 Rust。如果 PoC 发现只有把业务/权限移入 TS 才能运行，必须把冲突带回方案评审，而不是擅自改变技术边界。

2026-09-30 的下一步曾是执行 **M0：参考应用实查 + 双向审批/BrowserHost 验证 + 新需求与架构契约**。这是历史快照，不是当前状态。当前残差见第 24 节。用户已授权持续实施和测试验收。

## 11. 本轮验证记录

以下只记录 2026-09-30 编写计划当轮，不描述此后的应用、SDK、浏览器或 UI 证据。

- 已确认本地参考 checkout 的 HEAD 和 tag 为指定 v0.14.0；旧参考 commit 有保留分支。
- 已确认 `.ref/` 被忽略，参考仓库工作区干净。
- 已通过文档相对链接/目标文件、Markdown 围栏/尾随空白、参考包必需章节、F01–F23 唯一编号、计划索引日期/同日时间倒序及索引文件存在性检查；`git diff --check` 通过。
- 未运行应用、SDK、浏览器或 UI 测试；未宣称功能已完成。

## 12. 实施进度（2026-09-30）

- 已建实施分支；需求入口、参考启动/盘点脚本、真实 SDK 审批实验已落地。
- 参考启动/首批界面实查、35 工具/11 设置/8 认证盘点及两 SDK 的 10 个单写工具场景通过。
- 截至 2026-09-30：Rust/CEF 原生宿主已打包，CDP/AX、隐藏/恢复、原生关闭隐藏、profile 重启和中文粘贴取得证据；完整 M0 出口未满足。当天 M1 独立内核已开始，M2–M8 尚未实施。该句不是当前状态。随后切片见第 13–23 节，当前残差见第 24 节。
- 每项验证范围和未完成项见 [M0 实施记录](../../dogfood/craft-v014/m0-progress.md)。

- 阶段性技术决策见 [重建运行时决策](../../architecture/rebuild-runtime-decisions.md)；明确已验证能力与后续缺口。

## 13. M1 首个实施切片

M0 已提供 SDK/CEF 可行性证据与明确缺口，满足开始独立领域内核建设的依赖；完整 UI 基线和其它工具/平台验证仍继续，不把 M0 全部标记完成。

1. 新建 `crates/loom-core`，不依赖 Tauri、旧 models/Task 或实验目录；先保持一个有实际实现的 crate。
2. 建立 Workspace/Project/Session/Message/Turn/ToolCall、队列和运行租约；Workspace 不要求代码目录，Project 可选。
3. Catalog 与每 Session 分别使用追加 journal，checksum/seq/作用域验证；持久化后返回成功，读模型/summary 可重建，损坏记录不静默丢弃。
4. 提供创建、元数据、发送幂等、排队、领取运行、runtime event 去重、停止/完成/中断恢复、事件分页等直接生产 API；恢复不启动进程或重跑工具。
5. 用真实文件、另一个 writer、截断/损坏日志、缓存丢失与并发发送测试内核，而非仅验证字段序列化。
6. 新桥接入口、前端投影和只读迁移随后依赖此内核落地；该切片不等同于 M1 或完整复刻验收。

- M1 首个内核切片已实现：见 [内核契约](../../architecture/workbench-core.md)。尚缺 Tauri/前端接线和迁移，不标记 M1 完成。

- M1 内核最终 26 个测试入口通过，类型生成/TS 编译/Clippy 通过；完整项目检查与最终局部补验范围见 [M1 记录](../../dogfood/craft-v014/m1-core-progress.md)。下一步为 Tauri/前端接线与只读迁移。

## 14. M1 桌面与前端接线切片

- 接入新 core 的 Tauri 请求/Channel 订阅，限制可信主窗口；所有 IO 在后台线程，数据目录独立于旧 `.loom/`。
- 订阅具备关闭/窗口销毁清理和补流；前端按 Session 隔离快照、事件、草稿和请求状态。
- 新 Shell 通过独立调试开关启用，正常构建不自动替换旧入口；先交付 Workspace/Session 与基础元数据，M3 继续完整 Craft 交互对齐。
- 仅显式 debug 环境启用 fixture runtime，走真实 core Lease/持久化/事件路径；UI 明示测试回复，生产不可启用。
- 验证真实 IPC、乱序/补流/迟到快照、切换/停止、原生桌面、重启及订阅清理；只读导入继续作为 M1 未完成项。

- M1 桌面基础接线已落地并通过原生交互、重启、后台通知和 Cmd+Q 修复复验；[桌面证据](../../dogfood/craft-v014/m1-desktop-progress.md)。只读导入及完整 M1 出口仍待完成，M2–M8 不据此标记通过。

## 15. M1 旧数据迁移切片（实施中）

1. 独立只读扫描旧项目：Chat v1 JSON、v2 journal、Task v1 裸对象或版本封套。不得调用会截断残尾、重写快照或升级文档的旧加载器；同 ID 优先 v2，v2 损坏不回退 v1。
2. 预检返回逐项来源、版本/hash、消息数、损坏与缺失资源；v2 严格校验序号/归属，未提交残尾仅报告，绝不修改源文件。路径读取限制在项目内，拒绝符号链接、越界路径和非普通文件，设定扫描预算。
3. 将预检结果绑定不可变来源清单；执行前重新验证，变化要求重新预检。采用 staging 与原子发布，来源身份映射稳定 ID，重复执行幂等，失败逐项可诊断。
4. Chat 转录可读，历史回合不取得运行租约；旧 native resume、凭据、审批均不继承。Task 保留历史与附件/计划/Review，不伪造为新的 DAG 执行。
5. 在新工作台接入预检、执行与报告；以合成项目验证重启、重复导入、来源变化、残尾、损坏隔离、附件缺失和源目录字节不变。发布及回退验收仍属于 M8。

当前先实现独立读取及预检，随后接入持久化导入和桌面操作；预检通过不代表数据已迁移。

- 只读预检及桌面报告已实现：core 27+13 测试、前端定向 5 项、Tauri Workbench 5 项、原生 bundle/合成项目验收通过；源文件哈希不变且未新增 Session。[证据与限制](../../dogfood/craft-v014/m1-legacy-preview-progress.md)。实际导入、资源复制、staging/幂等恢复仍待完成。

当前实施实际导入：预检指纹经再次扫描核对后保存为有校验和的计划；执行按计划重新捕获来源和资源，来源变化拒绝导入。历史消息使用独立的只读历史模型，不伪造 live Turn/SendReceipt；完成的会话目录与资源一并原子发布。同一项目/旧 ID 映射稳定 Session ID，重复执行返回已导入，源内容修订则显式冲突。逐项报告持久化，重试从已发布目录确认完成，未发布 staging 不出现在会话列表。旧项目继续保持只读。

- macOS 项目格式实际导入已落地：48 个 core 测试入口、13 项前端定向回归、5 项 Tauri Workbench 测试与原生导入/去重/历史阅读/源离线重启通过。[导入证据](../../dogfood/craft-v014/m1-legacy-import-progress.md)。Windows/Linux、超预算增量迁移、导出包格式、完整回退和真实模型历史续聊未通过，不据此标记整体 M1 或重建完成。

## 16. M2 运行权限与 SDK 接线切片（实施中）

1. 在独立 core 中保存逐工具待审批、用户决定和一次性授权消费，绑定租约、工具 ID、参数 digest、cwd、连接与策略版本。决定持久化后才允许宿主回应 SDK。
2. 取消/超时/完成/租约失效同步撤销未消费授权，恢复时不能继承旧批准。并发点击、跨会话、参数变化、迟到决定、写盘失败和重启分别验证。
3. 桌面通过生成协议展示脱敏参数和允许/拒绝结果，不由 UI 分类工具或拼接程序参数。自动策略与 Guarded/记住授权按固定上游契约继续接入，不以单次审批覆盖全部策略功能。
4. 将 M0 受监督 SDK worker 接入 core 运行/审计协议，继而加入连接配置、vault、完整工具策略与历史上下文。真实 SDK + 本地模型 fixture 用于可重复副作用验证；实际模型账户验收另留独立证据，不互相冒充。

当前先落实持久化审批边界及桌面投影；完整 Claude/Pi 运行、账户与工具能力仍为 M2 交付要求。

- 2026-10-07：持久化单次审批、一次授权消费、撤销/重启恢复和桌面面板已落地；core 60、前端定向 12、宿主 6，以及真实 SDK + 本机模型 fixture 10/10 和原生审批路径取得证据。普通 task 路径误脱敏已修复。[审批切片记录](../../dogfood/craft-v014/m2-approval-progress.md)。生产 SDK 宿主、完整 policy、vault/账户和真实模型仍未完成，M2 保持实施中。

## 17. M2 连接与凭据切片（实施中）

- 建立全局命名模型连接目录、修订号和修改冲突检测，先接入 API key/bearer/无认证配置；其余认证流程仍按完整 inventory 实施，不伪装为已支持。
- 普通配置与秘密分离。Rust 宿主使用独立 vault，macOS 接系统 Keychain；不把密钥写入 catalog、journal、前端回读或命令参数。测试只使用新建的合成凭据项。
- 发送时保存连接快照；运行绑定须与已接受快照一致。已有会话锁定连接身份，模型选择仍可调整；导入历史不继承旧连接。
- 新桌面配置表单和会话选择器调用生成契约，明确“已保存凭据”不等于“模型连接通过”。SDK 运行接线、完整认证、跨平台 vault 与凭据回收继续交付。
- 验证端点校验、版本冲突、秘密不落盘/不回显、失败重试、排队配置冻结和原生保存/重启。新增依赖仅用于标准 URL 解析和系统秘密存储。

- 2026-10-07：目录/快照/Keychain 与前端配置已实现，core 68、前端/契约 17、宿主 11 和独立原生 Keychain 1 项通过，debug bundle 构建通过；Mac 锁屏导致原生界面验收待补，已请求解锁。[连接切片记录](../../dogfood/craft-v014/m2-connections-progress.md)。生产 SDK 与完整认证/凭据回收仍未完成。

## 18. M2 正式 worker 通信与监督（实施中）

- 新 worker 使用命名连接/模型/认证参数，输出标准化文本、工具请求/结果和终态；不继承 M0 固定 prompt、单文件白名单或 fixture 密钥。
- Rust 提供独立于 Tauri 的受监督进程层：私有 stdin 启动帧、作用域与序号校验、有界帧/队列、超时、取消、EOF 与进程组回收。秘密不进入 argv/日志。
- 工具执行前必须等待 Rust 的决定；未配置策略不能默认批准。完整工具分类与已实现审批 broker 在后续同一链路接入，不能以 worker 自己的提示词代替授权。
- 用真实 SDK 和本机模型接口验证通信/副作用；额外进程 fixture 验证错误作用域、坏帧、无终态退出、挂起、子进程及断线。正式桌面调度、账户、恢复/steering 和完整策略仍继续，测试程序不冒充最终宿主。

- 2026-10-08：新 supervisor/worker/生成协议已落地；workspace 80 测试入口、worker 类型检查和真实 Pi SDK 注入 fetch 6 场景通过。发现并修复无认证头泄漏与 Pi apiKey 表达式误用。真实 HTTP 命令因自动审批两次超时未启动，普通沙箱拒绝 bind；明确许可待回复，Claude 新 worker 未有运行证据。[切片记录](../../dogfood/craft-v014/m2-worker-progress.md)。M2 继续实施，不将测试桥称为桌面正式调度完成。

## 19. M2 生产内核运行桥（实施中）

- 有效租约取得冻结连接、本轮输入、已结束回合及导入历史；排除未来排队消息和 native resume，超预算明确失败。
- 生产 Handler 连接 SDK 事件、审批消费、取消和终态保存；必需 Rust policy 接口负责工具分类，无默认放行，自动决定保存审计。
- 持久化 token 使用量；测试桥改用生产适配器，以真实 SDK 注入 fetch 验证上下文、配置冻结与终态；桌面调度、完整 policy 与 HTTP 验收继续。

- 2026-10-09：生产 `run_session` 已接 core，增加明确 cwd、原始参数摘要/脱敏预览、累计用量与取消终态原子裁决。workspace 91 测试入口、前端投影 7 项、worker 类型检查、前端构建、真实 Pi SDK 注入 fetch 6 场景通过；后者核验历史/排队隔离与连接冻结。[运行桥记录](../../dogfood/craft-v014/m2-core-bridge-progress.md)。正式桌面调度、完整权限分类、账户、native resume/compact 与 HTTP/原生 UI 验收仍待完成。

## 20. M2 权限分类与正式调度（实施中）

1. 根据固定上游 `mode-manager`、`bash-validator`、`permissions-config`、`pre-tool-use` 和 Guarded 实现逐项对齐；不能用工具名称或命令前缀代替 Shell 语法分析。
2. Rust 使用 Bash 语法树实现组合命令/管道、引号、重定向、替换、环境赋值和程序危险参数检查。增加 tree-sitter/Bash grammar 与正则库仅用于这一需求；固定依赖版本，默认规则保留上游来源和许可证。
3. 建立可组合的默认/工作区/Source 规则、写路径匹配和模式判断，再接记住授权、管理员与 Guarded 决策。用上游实际分类生成差分用例，兼容差异与安全修正明确登记。
4. 正式调度使用冻结连接/策略、vault 私有读取、受监督 worker 与 core；排队、取消、退出、失败与重启不能丢失归属。不以全放行或测试策略让桌面提前显示可运行。
5. 每个已接工具分别验证 Explore/Ask/Execute，Guarded/Source 与未实现能力仍保留完整交付工作；原生与真实模型/HTTP 验收继续单列，不由静态或模拟测试抵扣。

- 2026-10-09：Rust Bash AST 分类和程序参数检查已完成首个切片；真实上游生成的 141 个差分/argv 用例及 workspace 95 测试入口通过。6 个分类安全修正和 1 个 argv 修正单独登记。[Shell 分类记录](../../dogfood/craft-v014/m2-shell-policy-progress.md)。完整 ToolPolicy、工作区/Source 配置、记住授权与正式调度仍实施中。

- 2026-10-09：生产 PermissionPolicy 接上运行桥；分层规则、原生模式、Source/API 分类、路径及 Guarded 接口和策略 digest 已实现。workspace 103、前端投影 7、宿主 11、上游 MCP 132 用例与真实 Pi SDK 注入 fetch 9 场景通过。[策略核心记录](../../dogfood/craft-v014/m2-tool-policy-progress.md)。配置存储/热更新、Shell 会话目录例外、记住/管理员授权、真实 Guarded provider 和正式桌面调度仍继续。

## 21. M2 Tauri 宿主 SDK 调度（实施中）

- 宿主私有加载冻结连接对应的不可变 vault slot；密钥只借用给 worker 私有 stdin，不进入 IPC/前端/执行参数，缺失或读取失败持久化为回合失败。
- SDK 与 fixture 共用每会话队列所有权；一个活动运行、重复发送不重跑、出错不自动重试副作用、取消/退出回收，排队准备失败也需可见且持久化。
- Rust 构造 cwd、策略快照与工具能力。开发运行显式选择 SDK，并从受信任宿主配置找到 Bun/worker；发行版本只认打包资源，未打包不能称为可分发完成。前端仅消费运行能力和命名连接。
- 补真实宿主路径的队列/并发/失败/重启/退出验证；以实际 Pi SDK + 注入 fetch 验证宿主到工具的副作用链路。原生操作、Claude 新 worker/HTTP 和发行包仍另行验收。
- 第 20 节未完策略与配置继续保留；接线默认规则不等于完整权限设置、热更新、记住授权或 Guarded provider 完成。

- 2026-10-09：Tauri 开发宿主已接 SDK/vault/生产策略/队列，补精确队首领取与准备失败事件、跨会话独立运行及重启恢复队列。workspace 104、宿主 17、前端/脚本 18、实际 Pi SDK 经 Host 的 3 模式通过。[宿主接线记录](../../dogfood/craft-v014/m2-desktop-sdk-progress.md)。发行资源/签名、原生 UI、HTTP/Claude、完整权限配置和其他 M2 能力仍未完成。

## 22. M2 权限配置持久化与策略审计（实施中）

- Core 保存应用/工作区/Source 权限文档，版本冲突检查、幂等操作、重启重放与重置；写操作由宿主完成配置解析/编译校验后调用，不向前端暴露无校验目录写入。
- SDK 宿主从持久化配置构造规则，保存完整生效策略快照后领取回合；审计通过会话/回合读取绑定的快照，不能任意读其他路径。
- 先实现下一回合自动读取新配置与运行回合冻结的一致性，再接文件外部编辑、热更新/待批准授权重评及设置 UI；不能把下一回合生效标为逐工具热更新已完成。
- 验证原子写、版本冲突、失效规则、损坏/链接替换、跨会话隔离、重启恢复，以及配置规则确实改变宿主执行结果。完整 UI 与实时撤销仍跟随第 20 节交付。

- 2026-10-09：权限文档 catalog/CAS/幂等/重置、宿主编译保存、下回合配置应用和完整策略快照审计已接线。workspace 109、宿主 19、契约 6、实际 Pi SDK Host 3 模式与前端回归通过；原生权限页校验/保存/重启/重置通过，锁屏阻塞已解除。[配置记录](../../dogfood/craft-v014/m2-permissions-store-progress.md)。外部文件热更新、授权重评、上游表格/UI 对齐和其他 M2 范围继续。

## 23. M2 会话记住授权（实施中）

- 依据固定上游 `agent/core/permission-remember.ts` 实现命令前缀/精确 argv、写目录、全部网络目标域名、MCP 工具与 API 方法/路径的选择器；复杂 Shell、危险命令/包装器、管理员操作和有效 Guarded 风险提示不提供记住选项。
- 选择器由 Rust 策略产生，审批显示范围，用户只能选择允许一次/此会话记住/拒绝，不能从前端注入授权键。原始精确命令不作为明文记忆键落盘。
- 用户决定、记忆授权和工具准入持久化且绑定会话/cwd/策略；支持显式清除。按上游运行生命周期处理，重启撤销旧授权但保留审计，不以持久化历史扩大成永久批准。
- 差分验证原始键算法，覆盖大小写、链/重定向/展开、危险子命令、多域名、隐藏配置目标；实际宿主两轮工具请求验证记住后匹配、拒绝/撤销、跨会话和重启隔离。

- 2026-10-09：会话记住选择器、原子授权准入/审计、清除与重启撤销已接生产运行桥和原生 UI。workspace 117、宿主默认套件 21、本切片前端定向 11、79 个上游选择器用例、2 项实际 Pi SDK Host 测试与原生记住/复用/清除/重启失效通过。[记住授权记录](../../dogfood/craft-v014/m2-remember-progress.md)。宿主 21 不是测试总数，前端 11 也不计全部前端测试；口径见第 24 节。管理员/热更新/真实 Guarded/其他账户继续，未进入最终验收。

## 24. 当前残差

本节是唯一当前状态，工作日期为 2026-10-10。第 10–12 节和各实施切片里的「剩余工作」都是当时记录，不回写；与本节冲突时以本节为准。本次补验见第 25 节和对应证据。状态只使用 `not-tested`、`partial`、`blocked`、`passed`；目前没有任何 F 行或里程碑为 `passed`。

### 里程碑

- M0 未收口：F01–F23 仍未拆成正常/异常/恢复案例；缺固定尺寸和深浅主题截图基线；SDK 的 Read/Bash/MCP、超时和 resume/steering 实验未做；BrowserHost 仍是 CEF 原型，不是产品浏览器。证据仍见 [M0 记录](../../dogfood/craft-v014/m0-progress.md)。
- M1 未收口：macOS 内核、调试 Shell 和只读导入已有证据。未做 Windows/Linux 安全读取、独立导出包、超预算增量，以及导入历史的真实模型续聊。回退仍属于 M8。
- M2 实施中，未到出口：已有单次审批、连接目录/Keychain、受监督 worker、生产运行桥、Shell/工具策略、权限配置、会话记住授权和开发宿主调度。本机 HTTP 与连接页、Claude SDK 发送/停止/再发及重启已补验；Pi 原生拒绝已有证据；未到出口的是 Pi SDK 的原生停止/再发路径、真实模型账户、resume/compact/steer、OAuth 及其他账户、真实 Guarded、管理员审批、外部文件热更新、Windows 进程树和发行包中的 Bun/worker。失败不自动重跑已有测试，但不能代替两类后端分别实测。
- M3–M8 未开始。默认构建仍是旧入口；新 Shell 只在 `VITE_LOOM_WORKBENCH=1` 时启用。

### 阻塞

- 先前本机 bind/升级审批阻塞已解除：本轮环境审批成功，正式 Claude/Pi worker 本机 HTTP 10/10 和 Claude 原生 SDK 路径已执行。[补验记录](../../dogfood/craft-v014/m2-http-native-progress.md)。真实账户与外部服务尚未验收，当前不能依据合成模型推断其可用性。

- Pi 原生第二轮停止按钮操作因 Mac 锁屏为 `blocked`，需手动解锁继续。首轮拒绝已测、退出时待审批已撤销；脚本退出不抵扣原生停止验证。

### 数字口径

- workspace：`crates/loom-core` 与 `crates/loom-runtime` 现有 117 个 `#[test]`。2026-10-09 的 `pnpm check:core` 记录为 117 项入口通过，见 [记住授权记录](../../dogfood/craft-v014/m2-remember-progress.md)。本轮没有重跑。
- 宿主：`src-tauri/src/workbench/` 现有 24 个 `#[test]`，其中 3 个默认 `#[ignore]`（1 个 Keychain、2 个 Pi SDK 注入 fetch）。2026-10-09 默认套件记录为 21 项通过；同轮单独执行了 2 个 Pi SDK ignore，Keychain ignore 未重跑。21 不是测试总数。
- 前端 11 只是记住授权切片的定向子集，不是全部前端测试总数。

### 下一步

1. 解锁后补 Pi SDK 的原生停止/再发/允许路径，完善正常/异常/恢复子案例。
2. 实施 resume/compact/steer，逐项固化能力声明与验证。
3. 推进其他认证类型及真实账户验收，继续管理员/Guarded/热更新与发行资源。
4. M3 界面移植仍依赖前置出口；完整 F 行核验后才能改成 `passed`。

## 25. M2 新 worker HTTP 与原生 SDK 补验

- 2026-10-10：正式 Claude/Pi worker 本机 HTTP allow/deny/cancel/stale/disconnect 10/10 通过，使用重新构建的生产 Rust 运行桥。先前环境审批阻塞已解除，模型响应仍为 fixture。
- 原生连接保存、重命名、留空保留凭据与重启无回显通过；Claude SDK 原生三轮拒绝/停止/再发允许、实际文件副作用与重启历史通过。Pi 原生首轮拒绝通过，第二轮停止时锁屏阻塞；脚本已正常清理，真实模型账户继续。
- 修复 bundled SDK 启动跳过 Bun 初始化；补验夹具支持持续服务；修复验收表扩列后的静态 inventory 解析。前端/脚本定向 6/6、脚本语法与 inventory 检查通过。[补验记录与产物](../../dogfood/craft-v014/m2-http-native-progress.md)。所有 F 行和 M2 保持未完整验收。
