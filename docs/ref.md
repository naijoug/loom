# Reference Pack

- Topic: Loom 本地多 Agent 桌面编排工作台参考包：研究现有 Agent 桌面端、CLI、会话、工具、安全权限、进程执行和多 Agent 协作实现。
- Generated: 2026-05-13 Asia/Shanghai
- Scope: 本轮按用户指定加入两个开源项目，重点做本地代码阅读笔记，不做依赖引入或实现改动。

## Search Queries

- local multi-agent desktop orchestration session management tool execution
- Codex CLI architecture app server sandbox approval subagents
- Craft Agents session MCP server desktop agent workflow
- Agent desktop app permission modes session workflow background tasks
- 本地 Agent 桌面端 多 Agent 编排 会话 工具调用 权限
- Codex CLI Rust 会话 沙箱 审批 子 Agent

## Official Docs

| Title | Link | Why It Matters |
| --- | --- | --- |
| Craft Agents README | https://github.com/craft-ai-agents/craft-agents-oss | 说明其桌面端会话、权限模式、Sources、MCP、本地服务端和后台任务产品形态。 |
| Craft Agents CLI docs | https://github.com/craft-ai-agents/craft-agents-oss/blob/main/docs/cli.md | 可参考其命令行/桌面协作入口，但本轮未深入展开。 |
| Codex Documentation | https://developers.openai.com/codex | Codex 的官方使用与概念文档入口，适合作为 Loom 接入 Codex 的行为边界参考。 |
| Codex repo docs | https://github.com/openai/codex/tree/main/docs | 覆盖安装、配置、sandbox、exec policy、AGENTS.md、skills、slash commands 等本地代理运行约束。 |

## Open Source Projects

| Project | Link | Local Path | License | Why Selected |
| --- | --- | --- | --- | --- |
| Craft Agents OSS | https://github.com/craft-ai-agents/craft-agents-oss | `.ref/repos/craft-agents-oss` | Apache-2.0 | 与 Loom 的桌面端 Agent 工作台目标最接近，包含 Electron UI、会话收件箱、权限模式、MCP Sources、session-scoped tools 和后台任务。 |
| OpenAI Codex | https://github.com/openai/codex | `.ref/repos/openai-codex` | Apache-2.0 | 直接对应 Loom 计划接入的 Codex Agent，Rust 代码覆盖会话、子 Agent、权限、沙箱、命令执行、app server 协议和持久化。 |

## Repo Notes

### Craft Agents OSS

- Local path: `.ref/repos/craft-agents-oss`
- Coverage: Bun/TypeScript monorepo，包含 Electron 桌面端、server-core、session tools、MCP server、shared config、sources 和 UI 包。
- Key files/modules:
  - `package.json`: monorepo workspace、Electron/server/webui 构建脚本、验证脚本和核心依赖。
  - `packages/core/src/types/session.ts`: 会话是主隔离边界，`SessionStatus` 包含 `todo`、`in_progress`、`needs_review`、`done`、`cancelled`。
  - `packages/server-core/src/sessions/SessionManager.ts`: 会话创建、恢复、消息发送、运行时配置、权限模式、sources、skills、attachments、automation、持久化队列的核心汇合点。
  - `packages/session-tools-core/src/tool-defs.ts`: session-scoped tools 的单一注册表，用 Zod schema 同时服务 Claude/Pi/MCP 消费者。
  - `packages/session-mcp-server/src/index.ts`: 为 Codex 暴露 session-scoped MCP tools，使用 stdio transport，并通过 stderr `__CALLBACK__` 给 Electron 主进程回调计划提交、认证请求等事件。
  - `packages/session-tools-core/src/runtime/path-security.ts`: 同时做词法路径校验和 realpath 校验，处理 symlink escape。
  - `packages/server-core/src/services/privileged-execution-broker.ts`: 把高权限请求、命令 hash、TTL、策略校验和审计日志独立封装。
  - `apps/electron/src/transport/*`: Electron preload/renderer/main 之间的 RPC/transport 边界。
- Mechanisms worth borrowing:
  - 把“任务/会话”作为权限、日志、工具、上下文、附件和持久化的共同边界，避免 UI 状态和 Agent 运行状态分散。
  - 工具注册表集中定义 schema、描述、执行模式、安全模式和 handler，再派生 MCP/SDK 所需格式；Loom 的 Agent adapter tool 能采用同样的单一事实源。
  - 将安全模式拆成可见产品概念：Explore/read-only、Ask to Edit、Auto；Loom 的 Agent 配置可以用能力标签和权限配置映射到类似档位。
  - 对需要主进程参与的工具，子进程只发结构化 callback，主进程负责 UI、credential、审批和状态变更，符合 Loom “本地进程/文件系统优先放 Rust/Tauri commands”的方向。
  - 高权限执行使用 request id、command hash、TTL、allowlist 和 JSONL 审计，适合 Loom 的删除文件、安装依赖、生产服务访问等高风险操作。
- Caveats:
  - Craft Agents 是 Electron+Bun+TypeScript，不是 Tauri+Rust；Loom 应借鉴边界和数据流，不直接照搬运行时。
  - `SessionManager.ts` 承担过多职责。Loom MVP 应避免形成单个巨大管理器，可按 Agent adapter、task runtime、command runner、audit log、UI projection 拆开。
  - Craft 的产品偏文档/Craft Sources，Loom 的核心是开发项目闭环，应弱化文档平台耦合。

### OpenAI Codex

- Local path: `.ref/repos/openai-codex`
- Coverage: Rust workspace，包含 CLI、core session runtime、app server、exec server、sandboxing、exec policy、subagents、MCP、skills、plugins、thread store 和 TUI。
- Key files/modules:
  - `codex-rs/Cargo.toml`: workspace crate 划分，显示 Codex 把 core、app-server、exec-server、execpolicy、thread-store、tools、MCP 等拆成独立 crate。
  - `codex-rs/core/src/session/session.rs`: `Session` 明确包含 event channel、agent status、active turn、mailbox、goal runtime、guardian review、services 和 runtime configuration。
  - `codex-rs/core/src/session/mod.rs`: session 初始化和 turn 管理的汇合处，负责 context、permissions、MCP、skills、rollout、mailbox、thread store 等。
  - `codex-rs/core/src/agent/control.rs`: 子 Agent 控制面，支持 spawn、fork history、角色配置、状态列表、父子线程元数据。
  - `codex-rs/core/src/agent/registry.rs`: 子 Agent 注册、最大线程数、路径、昵称、角色、last task message 和释放逻辑。
  - `codex-rs/core/src/agent/mailbox.rs`: Agent 间通信 mailbox，使用序号和 pending 队列，可触发目标 turn。
  - `codex-rs/core/src/exec_policy.rs`: 命令执行策略、审批需求、prefix rule、危险命令启发式、sandbox bypass 判定和规则持久化。
  - `codex-rs/app-server/src/request_processors/command_exec_processor.rs`: app server 命令执行入口，处理 cwd/env/tty/stdin/stdout/stderr/timeout/output cap/sandbox/permission profile。
  - `codex-rs/exec-server/src/*`: 将执行环境、远程 executor、sandboxed filesystem、process event 协议拆成可独立服务。
  - `docs/sandbox.md`、`docs/execpolicy.md`、`docs/agents_md.md`、`docs/skills.md`: Loom 接入 Codex 时需要遵守和呈现给用户的行为说明。
- Mechanisms worth borrowing:
  - Rust workspace 按“协议/执行/策略/状态/核心 runtime/UI server”分层，适合 Loom 的 Tauri 后端：把本地进程、文件系统、命令执行、日志采集和权限策略放在 Rust commands 或独立模块。
  - Session configuration 保存 approval policy、permission profile、cwd、codex_home、thread source、dynamic tools 等快照；Loom 的任务记录也应保存每轮运行的有效配置，而不只保存用户输入。
  - 子 Agent 使用 registry + control + mailbox，而不是临时 UI 列表；Loom 多 Agent 讨论、Review、调试验收可以用类似的 task graph/thread graph 表示。
  - 命令执行从 request processor 到 exec request 有清晰参数校验：互斥 sandbox/permission profile、TTY 尺寸、output cap、timeout、env override、网络代理和权限 profile。Loom 的 debug runner 应把这些作为一等字段持久化。
  - Exec policy 将“规则匹配”“是否需要审批”“能否持久化 allow prefix”“是否可绕过 sandbox”分开，适合 Loom 做可解释的安全边界。
  - App server protocol/processor 模式适合 Tauri 前端：UI 只发结构化请求，Rust 后端返回可订阅事件，而不是让前端直接驱动 shell。
- Caveats:
  - Codex 是单一 Agent 产品加子 Agent 能力，不是多供应商工作台；Loom 仍需要 Agent adapter 抽象来兼容 Claude Code、OpenClaw、Hermes 等。
  - Codex 的权限和 sandbox 体系很深，MVP 不应一次性完整复刻；应先实现最小可解释权限档位、命令日志、审批和可中断进程。
  - 直接绑定 Codex 内部协议会造成耦合。Loom 应以可执行命令/JSON/MCP/插件 API adapter 方式接入，把 Codex 作为一个 adapter 实例。

## Cross-Project Patterns

- 会话/任务是核心隔离边界：消息、工具、附件、cwd、权限、运行配置、状态、日志和审计都挂在 session/task 下。
- 工具系统需要单一注册表：schema、描述、执行归属、是否只读、安全模式和 handler 不应散落在 UI 与后端。
- 前端不直接执行高风险操作：桌面主进程或 Rust 后端持有 credential、审批、安全策略、命令执行和审计。
- 命令执行要事件化：启动、stdout/stderr delta、退出码、超时、终止、resize、stdin 写入都应是结构化事件，便于 Loom 做实时日志和调试验收循环。
- 权限模型要同时服务产品和执行层：用户看到的是 Explore/Ask/Auto 或类似模式，执行层保存的是 sandbox、network、approval、allowlist、cwd 和 env policy。
- 多 Agent 协作不只是并行调用：需要 registry、角色、状态、mailbox、父子关系、上下文 fork 策略和最终结果整合。
- 参考仓库都把测试放在关键边界旁边：path security、config、command exec、session persistence、registry/mailbox 都有测试价值，Loom MVP 应优先保护这些边界。

## Recommended Directions

- MVP 数据模型先围绕 `Task`、`AgentAdapter`、`AgentRun`、`CommandRun`、`Review`、`ValidationEvidence` 建立，避免把 UI session 和 Agent process 混成一个对象。
- Tauri/Rust 后端优先实现 command runner、process supervisor、log event stream、permission decision、audit log 和 adapter invocation；前端只订阅状态和提交结构化意图。
- Agent adapter 接口至少包含 capability tags、input protocol、cwd policy、write permission、command permission、stream parser、cancel/terminate 和 artifact collector。
- 任务状态机可借鉴 Craft 的 `todo/in_progress/needs_review/done/cancelled`，但要增加 Loom 自身阶段：discussion、implementation、review、debugging、fixing、accepted、blocked。
- 对 Codex adapter，先支持 CLI 调用和可见日志，再考虑更深的 app-server/MCP 集成；不要在 MVP 中依赖 Codex 内部未稳定模块。
- 对工具/命令安全，先做命令可见记录、工作目录、退出状态、审批档位、危险命令拦截和审计，再扩展成完整 sandbox/execpolicy。

## Open Questions

- Loom 是否要在 MVP 中直接支持子 Agent fork，还是先把每个外部 Agent run 当作独立任务节点？
- Agent 输出协议是否统一为文本流 + artifact parser，还是为 Codex/Claude Code 分别做结构化 adapter？
- 调试验收阶段的长进程是否需要 first-class TTY 支持，还是 MVP 只支持非交互 stdout/stderr？
- 权限策略是否按项目保存，还是按任务/Agent run 保存并允许用户临时覆盖？
