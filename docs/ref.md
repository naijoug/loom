# Reference Pack

- Topic: 保留 Tauri + React + Rust，以 Craft Agents v0.14.0 为基线完整重建 Loom 的功能与交互。
- Generated: 2026-09-30，Asia/Shanghai。
- Scope: 参考仓库更新、源码阅读与实施方案；实施进度见 [M0 记录](dogfood/craft-v014/m0-progress.md)，已启动参考桌面和完成局部 SDK PoC，完整复刻尚未验收。
- 用户已确认：保留 Tauri/Rust，复刻完整功能与交互，不采用 Electron fork 路线。
- 实施入口：[完整重建计划](plans/2026-09-30/13:17-craft-parity-rebuild.md)。
- [2026-05-13 参考包](references/2026-05-13-agent-workbench-ref.md)保留为历史；其中 Task 优先、CLI 优先等推荐不再代表本轮方向。

## Search Queries

实际检索/查阅围绕以下问题。用户指定 Craft 为唯一产品对标，不为凑数量新增近似仓库。

- `site:github.com/craft-ai-agents/craft-agents-oss SessionManager permissions automations`
- `site:agents.craft.do Craft Agents sources skills permissions architecture`
- `Craft Agents 开源 架构 会话 权限 Electron`
- `site:thecraftagents.com/docs automations browser remote server Craft Agents`
- 定向查阅：Tauri Webview Versions、Electron webContents debugger。

## Official Docs

| 文档 | 链接 | 贡献与边界 |
| --- | --- | --- |
| 固定版本源码 | [v0.14.0](https://github.com/craft-ai-agents/craft-agents-oss/tree/73bd9c2a3573158bea880984eb8d5fdb41e0cac2) | 唯一产品基线；动态官网与 README 不能覆盖版本源码。 |
| Permissions | [权限说明](https://thecraftagents.com/docs/core-concepts/permissions/) | 操作与规则；源码另有可选 Guarded，须纳入。 |
| Skills | [技能说明](https://thecraftagents.com/docs/skills/overview/) | SKILL.md、显式引用、requiredSources 自动激活。 |
| MCP Sources | [连接服务](https://thecraftagents.com/docs/sources/mcp-servers/connecting-servers/) | stdio/HTTP/SSE、认证、测试连接、启用。 |
| Browser | [浏览器说明](https://thecraftagents.com/docs/browser/overview/) | 原生窗口、快照引用、会话归属、远程调用本地浏览器。 |
| Automations | [自动化说明](https://thecraftagents.com/docs/automations/overview/) | 定时与事件触发、配置热更新。 |
| Remote Server | [服务端说明](https://thecraftagents.com/docs/server/headless/) | Desktop/Web/CLI 共用服务，连接认证和能力限制。 |
| Tauri | [WebView 实现](https://v2.tauri.app/reference/webview-versions/) | macOS/Linux 为 WebKit，不能假定 Craft Chromium/CDP 可直接运行。 |
| Electron | [debugger API](https://www.electronjs.org/docs/latest/api/web-contents#contentsdebugger) | 核对浏览器与桌面宿主耦合。 |
| 上游声明 | [LICENSE](https://github.com/craft-ai-agents/craft-agents-oss/blob/73bd9c2a3573158bea880984eb8d5fdb41e0cac2/LICENSE)、[NOTICE](https://github.com/craft-ai-agents/craft-agents-oss/blob/73bd9c2a3573158bea880984eb8d5fdb41e0cac2/NOTICE)、[TRADEMARK](https://github.com/craft-ai-agents/craft-agents-oss/blob/73bd9c2a3573158bea880984eb8d5fdb41e0cac2/TRADEMARK.md) | 复用保留声明；Loom 使用自己的品牌和服务地址；NOTICE 另列 SDK 与依赖条款。 |

## Open Source Projects

| 项目 | 本地路径 | 基线 / 许可证 | 用途 |
| --- | --- | --- | --- |
| [Craft Agents OSS](https://github.com/craft-ai-agents/craft-agents-oss) | `.ref/repos/craft-agents-oss` | v0.14.0，`73bd9c2a3573158bea880984eb8d5fdb41e0cac2`，Apache-2.0 | 唯一完整产品对标，fetch 后 detached checkout 固定。提交时间 2026-09-30 12:22:07 +08:00。 |
| [OpenAI Codex](https://github.com/openai/codex) | `.ref/repos/openai-codex` | 历史快照 `5e3ee5eddfa5333f2e0b011880abf0cbf92bd295`，Apache-2.0 | 已有辅助参考，本轮未更新；不能据此推断当前协议。 |

Craft 原本地版本 `c310624fda0530722f5d4c6d1f8f2b7ba172090e` / v0.9.3 保存在参考仓库内的 `ref-snapshot-2026-05-13` 分支。没有删除已有参考内容。`.ref/` 已被 `.gitignore` 忽略。参考仓库不是生产依赖，不能从这里直接 import 或依赖本机绝对路径构建。

### M0 新增浏览器参考

- [tauri-apps/cef-rs](https://github.com/tauri-apps/cef-rs)，本地 `.ref/repos/cef-rs`，固定 SHA `a712ad9bacd234c050631a95b9745b27d346b7cc`，版本 `154.3.0+154.0.32`，Apache-2.0 OR MIT。
- 用途：验证由 Rust 承载 Chromium 原生窗口及 helper 打包，保留 Tauri 主应用；不把第三方 Tauri CEF runtime 的宣传视为当前 Tauri 支持证明。
- 阅读入口：`examples/cefsimple/src/shared/{mod,simple_app}.rs`、`examples/cefsimple/src/mac/`、`cef/` 的 bundle 工具、`sys/build.rs`。
- CEF 运行时下载到 `.ref/cef-runtime`，CMake 4.4.3 装于 `.ref/toolchain`，不替换 Loom 的现有依赖。官方示例及适配的本机原生窗口已构建运行；能力与剩余缺口见 [运行时决策](architecture/rebuild-runtime-decisions.md)。

## Repo Notes

以下路径相对于 `.ref/repos/craft-agents-oss/`。已阅读关键实现与部分测试入口；这是源码调研，不是运行验收。

### 启动、传输与平台边界

- `apps/electron/src/main/index.ts` 经 `bootstrapServer` 构造 `SessionManager`、注入平台服务、注册 RPC；薄客户端模式跳过本地服务初始化。
- `packages/server-core/src/bootstrap/`、`packages/server/src/index.ts` 使桌面与 headless 共用服务核心。
- `packages/server-core/src/transport/server.ts` 的 `WsRpcServer` 负责握手、认证、heartbeat、workspace 路由、客户端序号和断线缓冲；`packages/shared/src/protocol/{channels,dto,events}.ts` 定义契约。
- 借鉴：Rust core 不依赖 Tauri Window/AppHandle，IPC 与 WebSocket 是薄传输；远程到本地浏览器需要单独的宿主能力边界。

### 会话生命周期与持久化

- `packages/server-core/src/sessions/SessionManager.ts` 的 `createSession/sendMessage/cancelProcessing/respondToPermission` 汇聚队列、steering、后台任务、Sources、自动化及 SDK 回调。文件当前 9,855 行，不宜整体翻译成同样大的 Rust service。
- `packages/shared/src/sessions/jsonl.ts` 使用第一行 header、后续 message 的格式；列表仅加载 header，portable path token 支持转移。
- `sessions/persistence-queue.ts` 默认 500ms 合并写，每会话串行，临时文件 rename，并协调外部元数据修改。它不是 Loom v2 的追加事件 journal，不能因同为 JSONL 就混用。
- `apps/electron/src/renderer/atoms/sessions.ts` 用 atomFamily 隔离会话，列表元数据与消息分开，避免其他会话的流式重渲染。
- 测试入口：`packages/server-core/src/sessions/{sendmessage-durability,midstream-queue,cold-session-metadata,background-task-surface}.test.ts`。
- 借鉴：Workspace/Session/Turn/ToolCall 新模型；Loom 的幂等、seq、恢复经验按新契约重验，不以旧测试通过替代新验收。

### Agent 后端与逐工具权限

- `packages/shared/src/agent/backend/factory.ts` 创建 `ClaudeAgent/PiAgent`；`backend/types.ts` 的 `AgentBackend` 定义流式、停止、中途消息、模型、permission 与 Source/auth 回调。
- `claude-agent.ts` 使用 Claude Agent SDK + PreToolUse；内部 `bypassPermissions` 依赖宿主 hook 实施权限，不可复制 flag 却省略校验。
- `pi-agent.ts`、`packages/pi-agent-server/`、`backend/pi/` 提供子进程与事件适配。当前根依赖固定 Claude SDK `0.3.280`、Pi SDK `0.87.1`。多 provider 不等于多个 CLI。
- `agent/mode-types.ts` 包含 `safe/ask/guarded/allow-all`，规范界面名 Explore/Ask/Guarded/Execute；旧 Auto 对应 allow-all。`mode-manager.ts`、`core/pre-tool-use.ts`、`permissions-config.ts` 是分层规则入口。
- `agent/core/guarded-mode.ts` 在 Execute 上增加风险检查；模型不能提升权限，项目外写入有固定提示规则。模型失败/无结果时回到 Execute，不是失败自动拒绝，也不是 OS sandbox。
- `shared/src/decisions/` 与 `server-core/src/decisions/` 包含可选决策 provider、记录、adaptive thinking、语义标签、中途消息与任务结果判断；要逐项核对开关和回退。
- `server-core/src/services/privileged-execution-broker.ts` 单独封装高权限执行。
- 移植：Rust RuntimeAdapter + 双向审批；必要时用最小 TS provider worker 承载官方 SDK，由 Rust 管理进程、审批、归属和持久化。SDK 的全部副作用是否可在执行前拦截，先做 M0 验证。

### Sources、Skills 与内置工具

- `shared/src/sources/server-builder.ts` 的 `SourceServerBuilder` 由 Source 构造 stdio/HTTP/SSE MCP 或 API server，凭据获取独立；`credential-manager.ts/token-refresh-manager.ts/api-tools.ts` 管理认证、续期与 HTTP 工具。
- `credentials/backends/secure-storage.ts` 是本地加密文件，不是 OS Keychain 接口；Loom 可实现独立凭据 vault，不能把明文传进 React。
- `skills/storage.ts` 读取 workspace/project/global skill，解析 frontmatter 和 requiredSources；显式引用需真正激活 runtime。
- `session-tools-core/src/tool-defs.ts` 集中 schema、描述、handler、安全属性并派生 SDK/MCP 格式；覆盖计划、认证、会话、Pages、Task、消息、浏览器等，部分 handler 由宿主注入。
- `session-tools-core/src/runtime/path-security.ts` 是 realpath/符号链接边界入口。
- 移植：工具 catalog 单一 schema，Rust 执行器；UI 与 Agent 写配置共用校验、存储、watcher，避免“界面设置一套、Agent 配置另一套”。

### UI、Browser、Pages、Task、Automation

- `apps/electron/src/renderer/components/app-shell/{AppShell,LeftSidebar,SessionList,ChatDisplay,PanelStackContainer}.tsx` 与 `input/FreeFormInput.tsx` 定义布局、导航、列表和输入交互；AppShell 当前 3,970 行，应按 feature 拆分。
- `renderer/pages/settings/` 包括 AI/App/Workspace/Appearance/Input/Preferences/Permissions/Labels/Messaging/Server/Shortcuts。`packages/ui/src/components/overlay/` 包含 diff、Markdown、PDF、图像、HTML、Mermaid、表格、终端查看器。
- `apps/electron/src/main/browser-pane-manager.ts` 依赖 BrowserWindow/BrowserView、cookie partition、`browser-cdp.ts`。WebKit 不直接兼容，需先验证受 Rust 管理的 Chromium 宿主和原生窗口。
- `packages/core/src/types/page.ts` 定义 static/interactive/live Pages；sandbox iframe 渲染，刷新脚本写快照，特权 action 经过 grant/lease broker。`shared/src/pages/`、`server-core/src/pages/` 实现存储、刷新、授权和发布；SQLite 是脚本私有工作数据，跨进程读 `snapshot.json`。
- `server-core/src/tasks/TaskRunner.ts` 调度 task.yaml DAG 的 ready 节点，以子会话执行并由 onSessionComplete 推进，保存 run log，支持验证/修复机制。源码明确 route/loop/approval 等控制流节点可解析但尚未执行，不把预留 schema 计为上游已交付功能。
- `shared/src/automations/automation-system.ts` 的 `AutomationSystem` 将 workspace 事件交给 prompt/webhook/script/event-log handlers，包含 SchedulerTick、Page refresh；不是仅做 cron 提示。
- 移植：设计与截图放 `designs/craft-v0.14.0/`；区分 Session、TaskSpec/TaskRun、Automation，旧 Loom 固定四阶段工作流不作为新 DAG 内核。

### 远程、消息渠道与托管服务

- `packages/messaging-gateway/src/registry.ts` 按 workspace 管理 gateway、pairing、bindings、凭据、生命周期；接入 Telegram/WhatsApp/Lark，WhatsApp 有独立 worker。
- `apps/webui`、`apps/cli`、`apps/viewer` 是独立入口，完整复刻不能漏掉。
- `shared/src/branding.ts` / `SessionManager.ts` 使用 `https://thecraftagents.com/s/api` 分享会话，`shared/src/pages/publisher.ts` 使用 `https://thecraftagents.com/p/api` 分享 Page。存在 viewer 前端不等于托管 API、存储、OAuth relay 全部可自部署。
- 移植：列全第一方端点，提供自己的服务或兼容端点；缺服务/凭据标记 blocked，不能用导出文件冒充在线分享。

## Cross-Project Patterns

主要对照 Craft 与 Loom；Codex 历史快照只保留入口。

| 对照项 | Craft | Loom 当前 | 重建选择 |
| --- | --- | --- | --- |
| 主模型 | Workspace/Project/Session，项目可选 | ChatSession 强关联 project_path，另有 Task | Workspace 资源隔离，Project 可选，Session 为执行中心。 |
| 执行 | SDK + 结构化事件、回调 | adapter 子进程 + stdout 解析 | Rust runtime + capability + 双向 provider 协议。 |
| 权限 | 逐工具审批、分层规则 | Ask 是发送前回合确认 | 重建协议，不用重命名宣称对齐。 |
| 持久化 | header/messages JSONL + 合并原子写 | journal/seq/幂等回执 | 新 schema、单 writer、读模型；导入器隔离格式。 |
| 工作流 | 会话之上的 DAG/自动化 | 固定四阶段开发工作流 | 新 Task 域；旧任务只读保留，不自动转 DAG。 |

## Recommended Directions

1. 固定 v0.14.0，按功能、状态、异常行为验收；不跟随 main 漂移。
2. 新 Rust core、typed IPC、React feature domains 独立建设，最终一次切换；迁移层不能反向依赖旧业务层。
3. 先验证 provider 审批与浏览器，再做会话闭环、Sources/Skills、内容、自动化/任务和远程入口。
4. 复用纯 UI 组件与测试思想，避免整搬巨大 manager；保持 Loom 品牌与独立服务配置。
5. 每项完成需要真实证据；源码存在、测试文件存在、静态截图都不能代替端到端验收。

## Open Questions

- 已确定：保留 Tauri/Rust，目标完整功能与交互，旧高级工作流不再约束新模型。
- M0：SDK 是否能拦截全部工具副作用并往返 Rust；不可拦截需改实现，不得默默降级成 CLI 文本模式。
- M0：Chromium 窗口、焦点、cookie、CDP、打包签名与多平台组合能否满足原体验；尚无完成证明。
- M0：分享/OAuth relay/决策 provider/更新等服务哪些可部署替换，哪些需外部账户；先列清单，不提前索要密钥。
- 验证排序建议 macOS 优先，最终覆盖上游 Windows/Linux 支持；排序不等于缩小功能或平台范围。
