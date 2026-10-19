# REA 调研模块接入计划

## 目标

研究并接入 [REA](https://github.com/morluto/rea) 6.5.0（MIT）作为 Loom 的本地调研/逆向工程 Source，使 Agent 能在需要闭源二进制、打包产物、运行时行为或版本对比证据时调用 REA 工具，并把 Evidence、审批与执行记录纳入 Loom 会话。

## 研究结论

- REA 是 Node.js CLI + MCP server，npm 包为 `rea-agents`，注册名 `io.github.morluto/rea`，要求 Node.js 22.19+ / 24.11+ / 26+；本机为 Node 26.11.0，满足要求。
- 官方 stdio 命令是版本固定的 `npx -y rea-agents@6.5.0 mcp`；`server.json` 声明 stdio transport。
- 能力覆盖本地二进制、JavaScript/Electron、.NET、Android、网站观察、保存的网络捕获、包/资源、进程行为等。深度原生分析按需使用 Hopper/Ghidra/IDA；静态 JavaScript/Electron 和 .NET 不要求原生引擎。
- MCP 契约核心是 Evidence：成功结果包含 `normalized_result`、`evidence_id`、`raw_result`、analysis profile 和 coverage/limitations/unknowns。工具会保留 session bundle，可用 `get_evidence_bundle(detail:"summary")` 分页/摘要发现，用 `inspect_analysis_view` 读取局部视图，用 `export_evidence_bundle` 导出完整包。
- `binary_session({})` 是运行时权威发现入口，返回包/协议身份、完整 catalog digest、按组折叠的 tool availability、provider 候选和 effects/annotations；不能把静态清单当作当前可用性。
- REA 自身不做授权控制；工具 annotations 只是提示。进程、浏览器、设备、提取和写文件等副作用必须由 Loom 的 MCP Source 审批层判定。
- 结果明确标记 target-derived untrusted content。Loom 必须把 Evidence/字符串/反编译文本当数据，不能让目标内容变成模型或宿主指令。
- 更新很活跃：仓库 HEAD `4692bb3`（2026-10-14），package 版本 6.5.0。接入必须固定版本并记录 catalog digest，禁止漂移到 `@latest`。
- 源码研究显示仓库自带大量测试、架构守卫和真实 provider 验收脚本；但本计划不引入其源码依赖，只以独立进程集成。

## 非目标

- 不在本切片实现通用 MCP 平台、Source 管理 UI、认证 refresh、HTTP/SSE 传输或 Skill 管理。
- 不捆绑、安装或自动升级 REA、Node.js、Ghidra、Hopper、IDA、JADX 等外部工具。
- 不把 REA 当成普通源码仓库分析器；完整源码仓库调研优先使用现有文件/grep/git 工具。
- 不绕过 M4 依赖做旧 Chat 或旧 Task 的临时直连。

## 架构方案

### 1. 定位

在 Loom 新内核中把 REA 作为第一个 **本地 MCP Source / Research toolset** 试点，而不是新的 Agent Adapter：

- Agent Adapter 继续负责模型/provider；REA 是模型可调用的工具源。
- Rust 负责 Source 配置、进程启动、catalog 缓存、审批、取消、输出预算、Evidence 持久化和诊断。
- React 只显示连接状态、工具目录、审批卡片、Evidence 摘要和失败/恢复操作。

这符合 `docs/requirements.md` 中命令行、MCP、HTTP API、插件协议作为不同适配器类型的扩展方向，也符合 `docs/architecture.md` 的本地副作用归 Rust、交互归前端边界。

### 2. 接入路径

优先顺序：

1. **CLI 桥（最小验证）**：Rust 为每个显式 REA 调用启动一次性 `npx -y rea-agents@6.5.0 <command> --json --format json`，作为 M4 前的 research provider 冒烟通道。只暴露 `analyze-javascript-application`、`inspect-evidence-bundle`、`doctor` 等无交互命令。
2. **MCP stdio Source（正式路径）**：Rust 启动/监督 `npx -y rea-agents@6.5.0 mcp`，完成 initialize、tools/list、`binary_session` 发现，并把允许的工具按 `mcp__rea__<tool>` 暴露给 provider worker。
3. **Evidence 视图（产品化）**：会话内保存 Evidence ID、catalog digest、摘要、coverage/unknowns 和导出文件，不做全文 UI 替代。

不建议把 REA 安装到用户全局，或让 provider worker 直接读取用户级 MCP 配置自行连接。Loom 应显式配置、显式启用、显式授权。

### 3. 权限与安全

- Source ID 使用 `rea`；MCP 工具名约束为 `rea__<tool>`，禁止 `__` 出现在 tool action，避免跨 Source 混淆。
- 按 REA `binary_session` 的 effects/annotations 与工具清单建立 Loom 本地元数据：
  - 只读静态分析：Explore 可用，但结果仍落 Evidence/日志。
  - 进程、浏览器、Android、提取、快照导出、写 analyst metadata：一律 Mutation，Explore 拒绝，Ask/Execute 走 Loom 审批。
  - 导出/提取路径必须显式列出，绑定 workspace/project 数据目录，不允许目标内容影响路径。
- 审批预览显示工具名、目标路径/URL、进程参数、输出目录、effects 和 untrusted-content 标记；完整原始参数只在 Rust 审计记录中摘要/哈希，不进入模型正文。
- `npx` 下载需要在连接设置中显式授权；无缓存/离线时给出可复现错误，不静默换版本。
- 严禁把 target-derived strings、伪代码、网页文本、日志当作指令；审批 UI 需显著标记“来自分析目标的内容”。
- MCP 超时必须按工具分层。静态分析可短；Ghidra 首查按 REA 默认 330s + 操作余量（官方 Codex 注册为 450s），并支持取消和 partial Evidence 恢复。

### 4. 数据与持久化

建议新增 Source 配置与会话资源模型，避免把 Evidence 塞进普通消息：

- Workspace Source catalog：`sourceId=rea`、固定包版本、命令、身份/catalog digest、能力快照、健康状态。
- Session enabledSources 延续现有字段；工具调用事件记录 `sourceId/toolName/evidenceId/digest/effects/status`。
- Evidence 附件采用内容仓库式存储：摘要/coverage/unknowns 入 journal，完整 JSON bundle 为资源文件，记录 SHA-256 与字节大小。
- 大结果先 `detail:"summary"`，需要时由用户/Agent 请求 `inspect_analysis_view` 或显式导出，防止单帧超过上下文。
- 重启后可恢复：连接状态只代表配置；REA MCP session 不承诺跨进程复活，未完成调用标记 interrupted，保留已发布 Evidence。

## 里程碑

### R0 — 可执行性冒烟（无 UI）

1. 在隔离临时目录运行固定版本 CLI `doctor --json` 与一个静态 JavaScript fixture 分析。
2. 记录 Node/npx/版本、catalog digest、输出 schema、耗时和退出码。
3. 验证失败：无网络/无缓存、无效路径、超时、取消。

出口：命令、版本、JSON 输出和错误分类可复现；不修改用户级 agent 配置。

**2026-10-15 执行结果：通过。**

- 官方包元数据：`rea-agents@6.5.0`，Node 要求 `^22.19.0 || ^24.11.0 || >=26.0.0`；本机 Node `26.11.0`、macOS `26.5.2`。
- 官方 tarball：`https://registry.npmjs.org/rea-agents/-/rea-agents-6.5.0.tgz`，npm shasum `7ea5e6f669bc18c45dbf5dba653edaa1ada07ab3`，本地 SHA-256 `531717a06e92ca651aab52691190f916fac044272c01f875e22c5e293218e699`。
- 固定入口：`npm exec --yes --package rea-agents@6.5.0 -- rea ...` 与官方 tarball 本地安装均输出 `6.5.0`。
- `doctor --json`：CLI 包版本和 catalog 可读取；本机缺 Hopper、Ghidra、IDA registration、JADX。全局 doctor 还报告用户级 agent skill/registration drift；Loom 不得读取或修复这些全局配置。
- 静态 fixture：`package.json + index.js`，限定 `--source-path package.json` 后成功，产出 Evidence `ev_b3389e13b49ad19f942a586881818c55130808a20dcdcb9ef47ce95df939aff8`，结果 34,076 bytes。
- 输入错误：不存在路径返回 `invalid_request` / `invalid_input`，保留 `details.issues`，可机器分类。
- 取消：1,202 文件 fixture 运行约 2 秒后发送 SIGINT，进程退出码 130，返回 `cancelled` 且保留 partial Evidence `ev_f917214e2edf709b48c31dee43b2842d86ae37ea6f74d375559c26237eb8f0b7`。
- 大结果观察：1,202 文件完整分析约 39.34 秒，最终 JSON 40,261,564 bytes；进度为 JSONL 且每个文件都会输出，Loom 必须有界缓存/采样进度，不能逐条转发到 UI 或模型上下文。
- CLI 混合输出：成功/失败结果与 `rea_progress` JSONL 混在 stdout；最后一行是结果 envelope，进度行必须先分流。取消测试曾把 stderr 也重定向，确认诊断仍可分离。
- 发现的 R1 修正：源码 checkout 的 doctor catalog digest 与官方 6.5.0 包不同（源码 `631538...`，官方 `405725...`）。Loom 必须以实际安装包运行 `binary_session` 的 digest 为准，不能用 GitHub HEAD fixture 固定 npm 包契约。
- 证据保存在 `docs/dogfood/rea-r0/`；取消结果只入库摘要和 SHA-256，大结果、tarball 和 fixture 未入库。

### R1 — MCP 探针与契约固化

1. Rust 独立小进程或测试 harness 完成 initialize/tools/list/`binary_session`。
2. 解析 catalog digest、工具 availability、effects、client requirements、provider candidates。
3. 生成 REA 工具风险映射 fixture，并加入策略单测。

出口：完整工具清单和不可用原因不丢失；固定版本 digest 变化时测试失败。

**2026-10-15 执行结果：通过。**

- 新增 `scripts/rea-mcp-contract-probe.mjs`：完成 MCP `initialize`、初始化通知、`tools/list`、`tools/call binary_session`，并在 stderr 0 bytes 时导出稳定契约。
- 固化 fixture：`tests/fixtures/rea/mcp-contract-6.5.0.json`。内容包含握手身份/能力、catalog counts/digests、207 个工具名、74 个 availability group、provider candidate 摘要，以及每个工具 input/output schema 与 annotations 的 SHA-256。
- `binary_session` 展开 group 后与 `tools/list` 完全一致：均为 207 个工具，无缺失、无重复，满足契约完整性检查。
- 新增 `tests/unit/reaMcpContract.test.cjs`：固定版本/catalog digest/schema digest，校验 availability 全覆盖，并把 effects 映射为保守 Loom 分类：`read-only` / `mutation` / `unavailable`。
- 关键分类结果：`analyze_javascript_application`、`get_evidence_bundle`、`inspect_analysis_view`、`evaluate_reconstruction_coverage` 为 read-only；`binary_session`、`open_binary`、`close_binary`、`export_evidence_bundle`、进程/浏览器场景为 mutation；provider 不可用时即使静态 effect 看似安全也返回 unavailable。
- MCP 请求层身份校验参数已确认：`expected_package_version`、64 位小写 hex `expected_catalog_digest`、绝对路径 `expected_server_path`。R2 启动时应同时传入三者。
- 发现 npm runner 边界：在本机用 `npm exec --package rea-agents@6.5.0 -- rea mcp` 作为长驻 stdio 子进程时，Node 未收到子进程输出；直接执行官方包内 `rea mcp` 正常。R2 应由 Rust 获取/验证包内可执行路径后直接 spawn，不通过 npm wrapper 保活。
- 定向验证：`node scripts/rea-mcp-contract-probe.mjs`、`pnpm test tests/unit/reaMcpContract.test.cjs` 4/4 通过；`node --check` 和 `git diff --check` 通过。

### R2 — Loom Source 服务与审批

1. 新增 MCP stdio supervisor：启动、ready、请求/通知、取消、超时、stderr 脱敏、进程组回收、重启。
2. 把 `mcp__rea__*` 接入 PermissionPolicy；实现只读/变更/不可用三类效果。
3. Provider worker 暴露已启用的 REA custom tools，所有调用先经 Rust 审批，结果作为 tool part 回写。
4. 增加审批卡片、Source 开关和调用详情。

出口：审批前无副作用；拒绝、取消、进程退出、重启和会话恢复都有事件证据。

**2026-10-15 实施状态：部分完成。**

已落地：

- `crates/loom-runtime/src/rea.rs`
  - 固定官方包身份：`rea-agents@6.5.0`、catalog digest、MCP 协议 `2025-06-18`。
  - `verify_package_root` 校验包名/版本；`executable` 从受信 package root 解析 `node_modules/.bin/rea`，不经过 npm/npx 常驻链。
  - `worker_command` 生成 30 秒启动预算、3 秒回收宽限的 supervisor 描述；这是启动/探针预算，具体工具仍需独立请求超时。
  - `probe_identity` 直接启动 `rea mcp`，验证 initialize 协议和 `rea@6.5.0` 身份；renderer 不能传入可执行路径。
  - `split_tool_name` 只接受 `mcp__rea__<Id>`，action 禁止 `__`，避免其它 MCP source 或嵌套 action 混入。
  - `ReaCatalog` 在编译期嵌入 R1 官方包契约 fixture，校验 207 个工具和 catalog digest；只把明确无副作用且可用的 Evidence 查询列为 read-only。
- `crates/loom-runtime/src/policy.rs`
  - `PermissionPolicy` 编译 REA catalog。
  - `mcp__rea__*` 在 Source 激活后走专用分类：read-only 工具按 Explore 可用；其余一律 Mutation，Explore 拒绝、Ask 审批、Execute 执行。
  - `analyze_javascript_application` 因 effect 包含 `mutatesSession`，按 Mutation 处理；这是比 R1 简化映射更保守的修正，符合“会话状态也是副作用”的注释。
- `src-tauri/src/workbench/runtime.rs`
  - 会话启用 `rea` Source 时把 `rea` 加入本回合 `PolicyScope.built_in_sources`。
  - 未启用时 `mcp__rea__*` 仍为 `Unavailable`，不能由前端直接开启执行路径。
- `crates/loom-runtime/tests/tool_policy.rs`
  - 覆盖 REA read-only、Mutation、未启用拒绝和工具名边界。

尚未完成：

- 常驻 stdio supervisor 已新增 `ReaMcpClient`：
  - 直接 spawn 受信 `rea mcp`，使用 Unix process group、非阻塞管道、1 MiB 请求帧、11 MiB 响应帧和有界接收队列。
  - 串行 JSON-RPC：initialize、initialized 通知、tools/list、`binary_session(expected_package_version, expected_catalog_digest)`、tools/call。
  - 请求超时后发送 MCP cancelled 通知，再按 SIGTERM → 3 秒宽限 → SIGKILL 回收进程组；stderr 保留 64 KiB，诊断输出截断 2048 字符。
  - 真实官方包集成测试验证 207 个工具、server identity 和 catalog digest；无 `LOOM_REA_PACKAGE_ROOT` 时测试跳过，不依赖全局 REA。
- Provider worker 动态工具桥已接线：
  - Rust `RunConfig` 新增受信 `externalTools`，由宿主按会话启用 Source 注入；协议重新生成 TS 声明。
  - `rea_external_tools()` 第一批只暴露 5 个低风险工具：`binary_session`、`get_evidence_bundle`、`inspect_analysis_view`、`list_unknowns`、`verify_unknown_resolution`；其余 202 个工具继续留在 host 内部，避免第一桥接层放大副作用面。
  - Claude worker 将外部工具加入 native tool 列表，`PreToolUse` 反查 `mcp__rea__*` 并继续先经 Rust 授权。
  - Pi worker 将外部工具加入 `customTools`，同样包装 `authorize` 后执行。
  - Worker 侧严格校验名称只能为 `mcp__rea__<Id>`，并把工具描述声明为 target-derived Evidence/data，不能作为指令。
- 允许后的工具调用已回调真实 REA：
  - Runtime 协议升级到 v2，`ToolDecision` 增加可选 `output`；内置工具仍只收到 allow/deny，外部工具由 Rust 执行后直接收到结果。
  - `Handler::external_tool` 只在 Rust 策略允许且请求 ID/tool ID 匹配时执行；结果超过 512 KiB 直接失败，不放大到 worker。
  - `ReaRunner` 从受信 `LOOM_REA_PACKAGE_ROOT` 解析官方 6.5.0 可执行文件，启动 `ReaMcpClient` 并将 `mcp__rea__*` 转发到 `call_tool`；单次请求 30 秒预算。
  - Worker 侧 `authorizeResult` 解析 decision 的 `output`，要求必须为对象结果；拒绝/取消仍为 false，不执行。
  - Claude/Pi 外部工具现在返回 host bridge 的真实结构化结果，并继续由 PostToolUse/包装器记录 ToolResult 和脱敏输出。
- 请求级进度通知和并发请求所有权还未映射到会话事件。
- 请求级进度通知和并发请求所有权还未映射到会话事件。
- REA 包的下载/安装/发行资源定位仍未实现；当前只支持受信宿主提供的 package root。
- Evidence UI、审批卡片中 REA 专属效果/untrusted 标记和会话资源持久化未完成。

验证：

- `cargo test -p loom-core`：86/86 通过。
- `cargo test -p loom-runtime`：35/35 通过（含 REA catalog、工具名、策略和 supervisor 测试）。
- `LOOM_REA_PACKAGE_ROOT=/private/tmp/loom-rea-r0-project cargo test -p loom-runtime --lib rea::tests::serial_client_verifies_identity_and_fixed_catalog -- --exact --nocapture --test-threads=1`：通过，实际连接官方 6.5.0 包。
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`：通过。
- `pnpm check:core`：通过，包含格式、clippy、测试和 workbench/runtime TS 绑定检查。
- `pnpm test tests/unit/reaMcpContract.test.cjs`：4/4 通过。
- `git diff --check`：通过。
- 2026-10-15 动态工具桥追加验证：`pnpm exec tsc --noEmit -p packages/provider-workers/tsconfig.json`、`cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`、`cargo test -p loom-runtime` 35/35、runtime bindings check、`pnpm exec tsc -p packages/protocol/tsconfig.json`、`git diff --check` 均通过。

### R3 — Research 工作流与 Evidence UI

1. 提供“调研目标”入口：本地文件/目录、网站观察、网络捕获、进程场景等显式选择。
2. Agent 引导先 `binary_session` 选择可用工具，再按 Evidence/coverage/unknowns 迭代。
3. Evidence 摘要、分页视图、导出和引用进入会话；导出文件可打开。
4. 与普通源码调研区分：没有 shipped artifact/runtime evidence 需求时不主动启用 REA。

出口：一条真实 JavaScript/Electron 调研和一条原生/运行时调研（如可用）完成从提问到 Evidence 引用的闭环。

## 里程碑依赖

- R0–R1 可在 M2 当前切片旁并行，不需要 M3 UI 完成。
- R2 依赖 M2 的 tool policy、审批 broker、runtime supervisor 已有基础；provider worker 需要补 MCP custom tool 桥。
- R3 的目录/资源 UI 依赖 M3/M5 的面板与内容仓库能力；若内容仓库未完成，先只做摘要 + 外部文件导出。
- 正式 Source 设置页归 M4，但可以用 REA 作为该里程碑的第一验收样本。

## 成功标准

- 用户能为 workspace 显式启用固定版本 REA，看到本机引擎/工具可用性，而不是全局隐藏配置。
- Agent 能调用允许的 REA 工具，且每次调用都有可见命令/参数摘要、审批结果、耗时、退出/错误和 Evidence 引用。
- Explore 不产生进程执行、浏览器/设备操作、文件提取或任意写；Ask/Execute 的危险操作必须逐项审批。
- 大结果不挤爆上下文，完整 Evidence 可校验、可恢复、可导出。
- 目标内容不会提升权限或改变请求路径；模型遵循 Evidence 的 coverage/limitations/unknowns，不把不确定结论写成事实。

## 风险

- **依赖更新频繁**：固定 6.5.0 和 catalog digest；升级作为显式配置变更。
- **副作用面广**：第一版建议白名单少量静态/摘要/视图工具，运行时场景和提取后续分批开放。
- **Node 运行时差异**：要求本机 Node 检查与 doctor 证据；不自动安装。
- **超时/取消复杂**：MCP 客户端取消不等于子进程立即停止；必须复用进程组回收与 partial Evidence 语义。
- **上下文污染**：所有 target-derived 文本都按数据渲染；提示词和审批层明确防注入。
- **与 M4 抢范围**：只做 REA 试点抽象，避免提前实现完整 Source 平台。

## 验证策略

- Rust 单测：工具名解析、catalog fixture、effects 映射、路径/输出边界、超时/取消、进程退出、审计脱敏。
- 协议测试：MCP initialize/list/call JSON fixture、partial result、错误 envelope、进度通知。
- 前端测试：Source 开关、工具审批、untrusted 标记、Evidence 摘要/分页/导出状态。
- 集成：`npx` 不可用、无网络、REA 失败、provider 拒绝/允许、Explorer 调用被拒。
- 真实验收：静态 JavaScript/Electron fixture；本机已有 Ghidra/Hopper/IDA 时再跑对应 provider，不声称未测引擎可用。

## 当前状态

- 2026-10-15：完成 REA 6.5.0 源码/文档研究并形成本计划。
- 2026-10-15：R0 固定版本 CLI 冒烟通过；R1 官方包 MCP 探针与契约 fixture 通过。未修改用户级 agent 配置，未接入生产 Source 服务。
