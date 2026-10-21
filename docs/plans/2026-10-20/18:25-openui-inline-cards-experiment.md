# OpenUI 内联卡片实验计划

- 状态：O0 已通过（2026-10-20，见 [探针记录](../../dogfood/2026-10-20-openui-probe.md)）；O1 已完成（2026-10-20，`b6c4d47`）；O2 已完成（2026-10-20，未提交）；O3–O5 未执行
- 基线：`codex/simple-cli-chat` @ `fd6ca41`（[05:55 UI 打磨](05:55-chat-ui-polish.md) 已提交）
- 调研依据：[thesysdev/openui](https://github.com/thesysdev/openui) @ `9823ec0`（2026-10-19，MIT）；`@openuidev/react-lang` 1.0.1、`@openuidev/lang-core` 1.0.1
- 范围：实验开关默认关闭，首个 Agent 只开 Claude Code；不改变 [需求文档](../../requirements.md) 的现行范围。实验转正前需先更新需求与 Chat 契约。

## 1. 背景

Loom 的助手回复目前只渲染一个 Markdown 子集，见 `src/features/chat/markdown.ts` 和 `MarkdownView.tsx`。渲染方式是 React 文本节点，不执行 HTML。

OpenUI 让模型用 OpenUI Lang 这种紧凑格式描述界面，再由前端按已注册的组件渲染。它的 `inlineMode` 允许模型照常输出 Markdown，只在需要时附带一个 ` ```openui-lang ` 代码块。这和 Loom 现有的“Markdown 代码块按语言标记分流”的方式相容。

本实验验证三件事：

1. 本地 CLI Agent 能否在不破坏编码行为的前提下，稳定产出可解析的内联卡片。
2. 卡片在转录、续聊和导出中能否保持现有的安全与持久化约束。
3. 依赖、包体积和提示词成本能否接受。

## 2. 目标

1. 在设置中加入实验开关“内联卡片（OpenUI，实验）”，默认关闭。关闭时，调用参数、转录渲染和包加载都与现状一致。
2. 开关打开且当前 Agent 声明支持时，适配器向 CLI 追加一段 OpenUI 内联模式的系统提示词。首版只支持 Claude Code，方式是 `--append-system-prompt`。
3. 前端把 ` ```openui-lang ` 代码块交给 OpenUI `Renderer`，按 Loom 自有的小型组件库渲染。解析失败或渲染异常时，回退成原样显示的代码块。
4. 组件库首版包含 4 个组件：
   - `FileList`：路径加说明。
   - `ChangeSummary`：文件、增删行数、要点。
   - `DataTable`：列与行，均为纯文本。
   - `Checklist`：条目与完成状态。
   样式沿用 `chat.css` 的令牌，深色和浅色主题、中英文都可用。
5. 交互仅限两个安全的本地动作：复制卡片的纯文本，以及把指定文本填入 Composer。不允许触发网络请求、命令执行或任何写操作。

## 3. 非目标

- 不引入 `@openuidev/react-ui`、`react-headless`、`browser-bundle`、`server` 或 `cli`，也不使用 OpenUI Cloud/Gateway。
- 不传 `toolProvider`，不支持 `Query()`/`Mutation()`、`defineFunction` 和数据请求。
- 不改 Chat v2 磁盘格式、IPC 命令和事件 schema，也不新增消息片段类型：卡片源码仍作为助手正文的一部分存储。
- 不让模型重试或自动纠错。语法错误只在前端回退，不另起一轮调用。
- 首版不覆盖 Codex、OpenCode、Grok 和自定义 CLI。它们的注入方式另行评估（见第 8 节）。
- 不修改旧 Task 数据，也不触及冻结的 Workbench 和新内核。

## 4. 成功标准

| 编号 | 标准 | 证据 |
|---|---|---|
| S1 | 开关关闭时，四个内建适配器的 argv、配置指纹和转录 DOM 与基线一致 | Rust 适配器快照测试；前端单测 |
| S2 | 开关打开时，Claude 的 argv 含 `--append-system-prompt <提示词>`；配置指纹改变，旧续聊句柄失效并按现有规则提示重建 | Rust 单测；`chat_context` 指纹测试 |
| S3 | 合法卡片在流式中逐步呈现，完成后稳定；非法、未知组件或参数类型错误时回退为代码块，且同一条消息的其余 Markdown 不受影响 | 前端单测（固定 fixture）；`smoke:visual` 截图 |
| S4 | 关闭开关时 OpenUI 相关代码不进入首屏 chunk；打开后按需加载。记录增量体积 | `pnpm build` 产物对比 |
| S5 | 开发和生产运行都没有对外网络请求：不加载 devtools CDN，没有遥测 | 依赖安装日志；dev 下的网络面板或 CSP 报错为零 |
| S6 | 真实 Claude Code：10 个固定提问中，至少 7 个产出能解析的卡片；不需要卡片的问题不硬塞卡片；Research 模式不发生写入 | 真实模型验收记录（可能消耗额度，未运行写 not run） |
| S7 | 每轮追加提示词的 token 增量有记录，并给出“可接受/不接受”的结论 | O0 测量数据 |

## 5. 当前状态（2026-10-20 核对）

已有：

- 适配器能力：`src-tauri/src/agent_adapter.rs` 的 `AgentCapabilities` 只有 `structured_stream`、`tool_events`、`native_resume`、`modes`。Claude 的参数在 `ClaudeAdapter::prepare` 中构造，目前没有任何系统提示词参数。
- 调用准备：`src-tauri/src/chat_context.rs::prepare_chat_invocation` 组装 `AdapterInvocationRequest`；`build_prompt` 只用于没有原生续聊的回退路径。续聊配置指纹由 `crate::chat::resume::binding` 生成。
- 设置：`src-tauri/src/models.rs::AppSettings` 含 `locale`、`theme_mode`、`confirm_before_commands`、`command_timeout_seconds`，新字段用 `#[serde(default)]` 可以兼容读取旧文件。
- 前端：`parseMarkdown` 已经把代码块的语言写入 `block.language`；`ChatTranscript` 知道 `message.status === "streaming"`，但目前没有传给 `Markdown`。
- 导出：`transcript.md` 用动态围栏保留字面内容，卡片源码会原样导出，无需改动。
- 依赖与安全：
  - Loom 使用 React 18.3.1，没有 `zod`。`react-lang` 对 React 的 peer 是 `*`，OpenUI 的 catalog 允许 `react ^18.3.1 || ^19`、`zod ^3.25 || ^4`。
  - Tauri CSP 的 `script-src`/`connect-src` 只允许本机。
  - pnpm 10.29.2 的 `allowBuilds` 只放行 `esbuild`，所以 `lang-core` 的 postinstall 遥测脚本默认不会执行。

调研所得（未在 Loom 内验证）：

- `react-lang` 在 `NODE_ENV=development` 时自动挂载 devtools，并从 CDN 拉取浏览器构建；生产构建会消除这段代码。
- `lang-core` 的运行时遥测需要 `OPENUI_RUNTIME_TELEMETRY_ENABLED=1` 才会开启，在浏览器环境中不发送。
- `generateSystemPrompt` 支持 `inlineMode: true`。
- `react-lang` 声明了 ESM/CJS 双构建，但能否在 Loom 测试的 `tsc → CommonJS` 流程中加载尚未验证。

假设（O0 验证）：

- Claude Code 2.1.304 在 `-p` 模式下接受 `--append-system-prompt`，并且与 `--resume` 同时使用时每轮都生效。
- 只含 4 个组件的内联模式提示词约 1–3k token。

## 6. 里程碑与任务

依赖顺序：O0 → O1 → (O2 ∥ O3) → O4 → O5。O2 和 O3 可以分开 Review。

### O0 可行性探针（不改产品代码）

- 产出：在 `scratchpad` 或 `experiments/openui-probe/`（不进入默认构建）中完成以下验证：
  - 用 `@openuidev/react-lang` 定义 4 个组件，生成 `inlineMode` 提示词，记录字数和 token 估算。
  - 用真实 Claude Code 跑 `-p --append-system-prompt`，包括一次 `--resume`，确认输出中能出现可解析的 ` ```openui-lang ` 块。
  - 确认 `react-lang` 能在 React 18 + Vite 7 下渲染，并能在测试的 CommonJS 流程中加载。
- 验证：探针记录写入 `docs/dogfood/2026-10-xx-openui-probe.md`，包括 CLI 版本、命令（正文用 `[PROMPT]` 脱敏）、结果和 token 数据。
- 退出条件：如果 CLI 不支持追加提示词，或卡片解析成功率低于 50%，实验终止，在本计划回填结论。

### O1 依赖与组件库

- 依赖：`pnpm add @openuidev/react-lang@1.0.1 zod`（版本锁定）。不把任何 OpenUI 包加入 `pnpm-workspace.yaml` 的 `allowBuilds`。
- 屏蔽 devtools：在 `vite.config.ts` 中把 `@openuidev/devtools` 解析到本地空模块 `src/features/chat/openui/devtoolsStub.ts`，确保开发模式也不请求 CDN。
- 组件库：新建 `src/features/chat/openui/library.tsx`，用 `defineComponent` 定义 4 个组件，props 全部是字符串、数字、布尔和数组，不接受 URL 或 HTML。新增 `actions.ts`，只定义 `copyText` 和 `insertToComposer` 两个动作。
- 提示词产物：新建 `scripts/openui-prompt.mjs`，从组件库生成内联模式提示词，写入 `src-tauri/resources/openui-inline-prompt.md`。`--check` 模式比较已提交的产物与当前组件库是否一致，并接入 `pnpm check`。
- 验证：`pnpm test -- tests/unit/openuiLibrary.test.cjs`，覆盖合法 fixture、未知组件、类型错误和流式截断的解析结果；运行 `pnpm build`，记录体积。
- 实施偏差（见第 10 节 O1）：不新增 `actions.ts`，组件不含任何动作 props；复制和插入到 Composer 改由 O3 的 Loom 外壳按钮提供。

### O2 后端：设置、能力与参数注入

- 设置：给 `AppSettings` 新增 `experimental_inline_cards: bool`，默认 false；前端的 `AppSettings` 类型和契约样本 `contracts/tauri-contract.json` 同步更新。
- 能力：给 `AgentCapabilities` 新增 `inline_cards: bool`，只有 `ADAPTER_CLAUDE` 为 true。前端 `src/domain/agent.ts::AgentCapabilities` 同步更新。
- 注入：`AdapterInvocationRequest` 新增 `system_append: Option<&str>`。`ClaudeAdapter::prepare` 在它为 `Some` 时追加 `--append-system-prompt <text>`；其他适配器忽略该字段。提示词用 `include_str!` 编译进二进制，不在 UI 拼接。
- 接线：`prepare_chat_invocation` 根据设置和能力决定是否传入提示词；执行记录中该参数的值用 `[OPENUI_PROMPT sha256:…]` 替代，避免日志膨胀。
- 指纹：把“卡片开关状态和提示词 sha256”加入续聊配置指纹。开关切换后，旧句柄按现有规则要求“开新 CLI 会话”，不静默续用。
- 验证：
  - 相关 `cargo test`：关闭时参数快照不变（S1），打开时 Claude 带参数而其他适配器不带，指纹变化（S2），执行记录已脱敏。
  - `cargo fmt`、Clippy、`pnpm test -- tests/unit/contract.test.cjs`。

### O3 前端：渲染与回退

- `MarkdownView.tsx`：`Markdown` 新增可选参数 `streaming` 和 `inlineCards`。当 `block.language === "openui-lang"` 且 `inlineCards` 为 true 时，渲染 `<InlineCard source streaming />`，否则保持原代码块。
- `src/features/chat/openui/InlineCard.tsx`：
  - 用 `React.lazy` 加载 `Renderer` 和组件库，外层包 ErrorBoundary。
  - 不传 `toolProvider` 和 `onAction`；组件没有动作 props（O1 偏差）。卡片外壳由 Loom 提供“复制源码”和“插入到输入框”按钮。
  - 流式中容忍未解析的引用；完成后用 `checkInlineCard(source)`（解析器结果）判定，`ok` 为 false 或渲染抛出异常时回退为 `<pre><code>`，并显示“卡片无法渲染，已显示源码”。
  - 提供“查看源码”切换，方便核对。
- `ChatTranscript.tsx`：把消息状态和设置中的开关传给 `Markdown`。“插入到输入框”通过现有草稿接口写入，不直接发送。
- 设置页：在“实验”分区加开关和说明，说明需提到“仅 Claude Code 生效，会增加每轮提示词长度，切换后需开新 CLI 会话”。i18n 中英文同步。
- 验证：
  - `pnpm test -- tests/unit/openuiInlineCard.test.cjs`（jsdom）：合法卡片、回退、流式到完成、关闭开关时不渲染卡片，以及外壳按钮不触发发送。
  - `pnpm build` 后确认 OpenUI 代码在独立 chunk 中（S4）。

### O4 合成与视觉验证

- `mockStore.ts` 增加一个含卡片和坏卡片的示例会话。`scripts/simple-chat-smoke.sh` 补充截图：深浅主题、中英文、窄窗口。
- 在 `scripts/debug.sh web` 下检查网络面板，确认没有 CDN 或遥测请求（S5）。结束后执行 `scripts/debug.sh stop`。
- 验证：`pnpm smoke:visual`，截图放在 `designs/exports/openui-inline-cards/`。

### O5 真实模型验收与结论

- 用固定的 10 个提问（列在验收记录中），覆盖四类：
  - 列出文件
  - 总结改动
  - 对比表格
  - 纯问答，期望不出卡片
- 在 Research 和 Code 两种模式下各跑一遍，记录解析成功率、回退次数、token 增量和是否发生写入。
- 桌面端：`scripts/debug.sh desktop` 实测开关切换、续聊句柄失效提示和导出内容。
- 结论写回本计划和 `docs/PLANS.md` 摘要：转正、继续实验或移除。决定转正时，再另写需求和契约的更新计划。

## 7. 风险与回滚

| 风险 | 缓解 |
|---|---|
| 追加提示词改变 Claude 的编码行为，或增加成本 | 默认关闭；只追加不替换；O0/O5 记录 token 和行为；设置说明成本 |
| 模型在不合适的场景输出卡片，或语法错误 | 内联模式保留 Markdown 为主；前端回退为代码块；不自动重试 |
| devtools 在开发模式拉取 CDN；依赖安装时触发遥测 | Vite 别名指向空模块；pnpm `allowBuilds` 不放行；必要时在 `scripts/debug.sh` 里设置 `OPENUI_TELEMETRY_DISABLED=1` |
| OpenUI 1.x 后续发生破坏性变更 | 锁定确切版本；组件库与提示词产物由 `--check` 绑定；升级单独评估 |
| 测试流程（tsc→CJS）无法加载 ESM 包 | O0 提前验证；必要时只在 jsdom 测试中用 esbuild 预打包，不改生产构建 |
| 续聊句柄与开关不一致导致行为混杂 | 开关和提示词哈希进入配置指纹，切换即失效 |

回滚：关闭设置开关即可恢复原有行为。要彻底移除，删除 `src/features/chat/openui/`、提示词产物、两个依赖和 `AppSettings`/`AgentCapabilities` 的新字段（旧设置文件中的多余字段会被 serde 忽略）。会话数据无需迁移：卡片源码仍是普通的 Markdown 代码块。

## 8. 待确认问题

- 无阻塞问题。
- 后续评估（不在本计划内）：Codex 可能通过 `-c developer_instructions=…` 或项目 `AGENTS.md` 注入，OpenCode 通过 agent 配置，Grok 通过 rules。每种方式都需要按适配器单独实测，再决定是否开放 `inline_cards`。

## 9. 验证策略

按 [验证矩阵](../../testing.md#开发变更验证矩阵) 执行：

- 每个里程碑运行相关的单测、`cargo test`、`pnpm build`。
- O4 运行 `pnpm smoke:visual`。
- 合入前运行完整的 `pnpm check`。
- 真实模型（O0/O5）可能消耗额度。未运行、登录失败或额度不足分别如实记录，不写成 passed。合成网页结果不能当作桌面或真实模型的证据。

## 10. 验证结果

### O0（2026-10-20，passed）

详见 [探针记录](../../dogfood/2026-10-20-openui-probe.md)。本机 Claude Code 2.1.304 的后端是本地代理上的 GLM-5.3，不是 Anthropic 的 Claude 模型。

- 第 5 节的两条假设均已验证：
  - `--append-system-prompt` 在 `-p` 模式下生效，配合 `--resume` 也生效，但不随句柄保留，必须每轮传入。
  - 提示词约 4.7k 字符，实测每次 API 调用的输入增加 1048 token。
- 真实回合：预期出卡片的有效回合与续聊合计 3/3 产出可解析卡片，解析错误为 0；不需要卡片的问题没有出卡片。有 2 个回合因模型误判工具不可用而无效。合成项目没有被写入。
- 工程层：
  - React 18 和 CommonJS 加载通过。
  - 卡片 chunk gzip 约 60 KB。
  - 开发模式会从 jsdelivr 加载 devtools 脚本，Vite 别名到空模块后外部请求为 0。
  - pnpm `allowBuilds` 会跳过 `lang-core` 的 postinstall。
- 成功标准：S7 partial（只有单模型、单样本，尚未给出最终的可接受结论，留给 O5）；S5 的开发和安装部分已有探针证据，正式实现后需要复验。
- 设计修正，O1/O3 实施时生效：
  1. 回退依据改为解析器的 `meta.errors` 或空 root，因为 `Renderer` 会把非法卡片静默渲染成空容器，`onError` 不可靠。
  2. 内联模式的默认说明会诱导模型只输出增量语句，必须保留“每个代码块都是完整程序”的追加规则。
  3. 组件签名把必填参数放前面，例如 `FileList(files, title?)`。

### O1（2026-10-20，passed，`b6c4d47`）

- 依赖：`@openuidev/react-lang` 1.0.1 和 `zod` 4.6.7，精确版本。
  - 仓库 `node_modules` 由 pnpm 11.20.0 安装，本机 pnpm 10.29.2 报 store 版本不符，因此用 `npx pnpm@11.20.0 add --save-exact`。
  - pnpm 11 对被跳过的安装脚本报错，`pnpm-workspace.yaml` 显式写入 `'@openuidev/lang-core': false`。postinstall 遥测仍不执行，与计划意图一致；`pnpm install --frozen-lockfile` 通过。
  - 锁文件同时清理了 `package.json` 中已不存在的 `@xterm` 条目。
- devtools：`vite.config.ts` 把 `@openuidev/devtools` 别名到 `src/features/chat/openui/devtoolsStub.ts`。为避免给 `tsconfig.node.json` 引入 Node 类型，路径用 `new URL(...).pathname` 计算。O0 已在开发服务器上验证外部请求为 0；接入 UI 后在 O3 复验。
- 组件库：`src/features/chat/openui/library.tsx` 定义 `CardStack` 根组件和 4 个卡片组件，必填参数在前，导出 `inlineCardPrompt()` 与 `checkInlineCard()`。`checkInlineCard` 依据解析器的 `meta.errors`、`incomplete`、`unresolved` 以及空 root/空卡片判定能否渲染。
- 偏差：不新增 `actions.ts`。组件不暴露任何动作 props，模型可触发的动作面为零；复制和插入到 Composer 改由 O3 的 Loom 外壳按钮实现。
- 提示词产物：`scripts/openui-prompt.mjs` 用 Vite 的 esbuild 转译组件库并生成 `src-tauri/resources/openui-inline-prompt.md`（4700 字符）。`pnpm openui:prompt --check` 已接入 `scripts/check.sh`；同步检查只在脚本中做，不在单测重复。
- 验证：
  - `pnpm test -- tests/unit/openuiLibrary.test.cjs`：5/5 通过。覆盖提示词规则与签名、schema 无 action/url/html、合法卡片、`unknown-component`/`type-mismatch`/`unresolved`/`empty`/`no-root`/`incomplete` 回退，以及 `<script>`、`<img onerror>` 被转义。
  - `pnpm test` 全量 87/87 通过；`tsc -p tsconfig.json` 与 `tsc -p tsconfig.node.json` 无错误。
  - `pnpm build` 通过；`library.tsx` 尚未被应用引用，`dist/` 中不含 OpenUI 代码（S4 的独立 chunk 在 O3 验证）。
  - `pnpm openui:prompt --check` 通过。未运行完整 `pnpm check` 和 Rust 检查（本阶段未改 Rust）。

### O2（2026-10-20，passed，未提交）

- 设置：`AppSettings.experimental_inline_cards`（前端 `experimentalInlineCards`），`#[serde(default)]` 默认 false，旧设置文件无需迁移；前端 `normalizeSettings` 保留该字段，避免保存设置时丢失。契约样本 `contracts/tauri-contract.json` 与 `src/api/contract.ts` 同步。
- 能力：`AgentCapabilities.inline_cards` 只对 `ADAPTER_CLAUDE` 为 true，前端类型与诊断样本同步。
- 注入：`AdapterInvocationRequest.system_append`；`ClaudeAdapter` 在 `--permission-mode` 之后、`--resume` 之前追加 `--append-system-prompt <text>`，续聊回合同样追加。提示词在 `src-tauri/src/chat/inline_cards.rs` 中用 `include_str!` 编译进二进制。
- 接线：`chat_send` 读取设置（读取失败按关闭处理，不阻断 Chat），`prepare_chat_invocation` 新增 `inline_cards` 参数，经 `inline_cards::system_append` 结合适配器能力决定是否传入。
- 脱敏：`turns::snapshot` 把等于提示词的参数替换为 `[OPENUI_PROMPT sha256:<前 16 位>]`。
- 指纹：只在提示词生效时把 `["system-append", sha256]` 追加到续聊配置，关闭时指纹与现状完全一致，已有句柄不失效；开关切换后旧句柄按现有规则提示“开新 CLI 会话”。
- 与计划的差异：计划写“开关状态和提示词 sha256 都加入指纹”，实施改为“仅生效时加入提示词 sha256”，以满足 S1（关闭时不变）并避免所有现有会话失效；对 Codex/OpenCode/Grok/自定义 CLI 开关无效果，指纹也不变。
- 验证：
  - 新增 `agent_adapter::tests::only_claude_appends_a_system_prompt_on_every_turn` 与 `chat_context::tests::inline_cards_append_prompt_only_for_claude_and_rebind_resume`：关闭时参数与指纹不变（S1）；打开时仅 Claude 带参数、去掉该参数后与关闭时一致、指纹变化、执行记录不含提示词原文；两种句柄跨开关被拒绝；续聊回合仍带参数；其他适配器参数和指纹不变（S2）。
  - `pnpm check` 通过（含 `openui:prompt --check`、`pnpm test` 87/87、`pnpm build`、`cargo fmt --check`、Clippy `-D warnings`、`cargo test` 150 passed / 16 ignored）。
  - 未运行真实 CLI 验收（`real_agent_tests` 默认忽略）；真实 Claude 回合留到 O4/O5。
