# Chat GFM Markdown 渲染与消息分隔修复计划

- 状态：完成；`pnpm check` 与 `smoke:visual` 通过；2026-10-26 用户在桌面端验收外链由系统浏览器打开、WebView 不导航；已提交
- 基线：`codex/simple-cli-chat` @ `c8313dd`
- 触发：用户截图——Codex Research 会话中 `**加粗**` 与 `[文本](url)` 原样显示，且“…使用成本。**WeKnora…”两条消息粘连成一段
- 范围：前端 Markdown 渲染、外链打开、Rust 流解析中文本块之间的分隔；不改 v2 存储格式、IPC 和 OpenUI 实验范围

## 1. 背景

1. `src/features/chat/markdown.ts` 只解析段落、围栏代码、列表和行内代码，不支持加粗、链接、标题、表格、引用。模型（尤其 Codex、GLM）默认输出 GFM，于是标记原样露出。
2. Codex `--json` 每条 `agent_message` 以 `item.completed` 给出整段文本；`runtime.rs` 把每个 delta 直接 `push_str` 到正文，两条消息之间没有分隔。Claude 在工具调用后开启新的文本块（`content_block_start`）或一次给出多个 text block 时也有同样问题，OpenCode 的多个 `text` part 亦然。
3. 内置渲染不认识链接；如果改为渲染 `<a>`，WebView 内点击会让整个应用跳走，必须改为系统浏览器打开。

## 2. 目标

1. 完整 GFM：标题、加粗/斜体/删除线、链接与自动链接、引用、有序/无序/任务列表、表格、分隔线、围栏与行内代码。
2. 中文友好：`**…。**中文` 这类 CommonMark 侧翼规则下不闭合的加粗也能正确识别。
3. 禁止原始 HTML：HTML 标签按字面文本显示，不解析、不执行；`javascript:` 等非安全协议链接不可点击。
4. 外链：`http(s)` 与 `mailto` 链接在 Tauri 下用 `@tauri-apps/plugin-opener` 的 `openUrl` 交给系统浏览器，网页预览下 `window.open(…, "noopener,noreferrer")`；任何情况下 WebView 不导航。其他协议或相对路径（如本地文件路径）显示为普通文本。
5. 分隔：Codex 相邻 `agent_message`、Claude 新文本块和同一消息内多个 text block、OpenCode 多个 text part 之间补空行，使其成为独立段落。
6. OpenUI 实验保持可用：开启卡片时 ` ```openui-lang ` 代码块继续渲染为 `InlineCard`；代码块保留 `data-language` 属性。

非目标：不重写已持久化的历史消息（旧会话中已粘连的文本保持原样）；不做代码高亮；不改 OpenUI 的提示词或出卡策略。

## 3. 依赖理由

| 依赖 | 版本 | 理由 |
|---|---|---|
| `react-markdown` | 9.1.0（精确） | 生成 React 元素，不用 `dangerouslySetInnerHTML`；默认 `urlTransform` 清除不安全协议；可覆盖 `a`/`code`/`pre` 组件 |
| `remark-gfm` | 4.0.1（精确） | 表格、任务列表、删除线、自动链接 |
| `remark-cjk-friendly` | 2.3.2（精确） | 修正中日韩标点相邻时加粗/斜体不闭合；探针中缺少它时中文加粗失败 |

探针结论（scratchpad `md-probe`）：上述组合通过中文加粗、冒号加粗、链接、表格、标题、任务列表、自动链接全部用例；`markdown-to-jsx@7.7.17` 把 `javascript:` 链接误渲染且表格单元格多出空格，放弃。三个包均为 ESM，Node 26 `require(esm)` 可用，现有 tsc→CJS 单测流程不变。

## 4. 实施项

| 编号 | 内容 | 文件 |
|---|---|---|
| M1 | 引入依赖；`MarkdownView.tsx` 改用 `react-markdown` + `remark-gfm` + `remark-cjk-friendly`，自定义 remark 插件把 `html` 节点改为 `text` 节点；覆盖 `pre`/`code`（保留 `data-language`，卡片开启时 `openui-lang` → `InlineCard`）；删除不再使用的 `markdown.ts` | `package.json`、`MarkdownView.tsx`、`markdown.ts` |
| M2 | 外链：`externalLink.ts` 判定安全协议并打开；`a` 组件 `preventDefault` 后调用；能力文件增加 `opener:allow-open-url`，作用域仅 `http://*`、`https://*`、`mailto:*` | `src/features/chat/externalLink.ts`、`src-tauri/capabilities/default.json` |
| M3 | 样式：标题、引用、表格（可横向滚动）、任务列表、分隔线、链接色，深浅主题共用 `--chat-*` 令牌 | `chat.css` |
| M4 | 分隔：`ParsedChatLine` 增加 `block_start`；Codex `agent_message`、Claude `content_block_start`(text) 与非流式 `assistant`、OpenCode `text` 置位；`runtime.rs` 记录待插入分隔，下一段非空 delta 到达且正文非空时按已有结尾补齐到一个空行；同一事件内多个 text block 以空行连接 | `src-tauri/src/chat/mod.rs`、`runtime.rs` |
| M5 | 测试：前端单测覆盖 GFM 元素、中文加粗、HTML 字面量、`javascript:` 不可点、外链点击不导航且调用打开函数、`openui-lang` 卡片；Rust 单测覆盖 Codex 两条消息、Claude 工具后新文本块、首块无前导空行 | `tests/unit/*`、`src-tauri/src/chat/*` |
| M6 | 文档：安全策略写明 GFM 渲染、HTML 字面显示和外链策略；需求与 Chat 契约同步 | `docs/security-policy.md`、`docs/requirements.md`、`docs/architecture/chat-contracts.md` |

## 5. 验证

- `pnpm check`（含 lint、单测、构建、Rust 测试，按验证矩阵）。
- 记录主包与懒加载 chunk 体积变化。
- `scripts/debug.sh stop` → `pnpm smoke:visual` → `scripts/debug.sh stop`；内置浏览器打开合成会话检查表格、加粗、链接在深浅主题下的显示。
- 桌面端点击外链由系统浏览器打开：需用户人工验收（`openUrl` 权限只在 Tauri 运行时生效）。

## 6. 风险与回滚

- 体积：react-markdown 生态约 40–50 KB gzip，进入主包；若超出预期可改为懒加载 Markdown 组件。
- 流式期间未闭合的标记会短暂原样出现，回合结束后正确渲染；与现状一致，不额外处理。
- 回滚：M1–M3 与 M4 互不依赖，可分别回退提交；能力文件回退即撤销外链权限。

## 7. 实施记录

| 编号 | 状态 | 说明 |
|---|---|---|
| M1 | 完成 | 依赖精确版本 `react-markdown@9.1.0`、`remark-gfm@4.0.1`、`remark-cjk-friendly@2.3.2`；`literalHtml` 插件把行内 HTML 变为文本、块级 HTML 变为字面段落；`pre` 覆盖保留 `data-language` 与 `openui-lang` 卡片；删除 `markdown.ts` |
| M2 | 完成 | `externalLink.ts`：`isExternalUrl` 仅接受可解析的 `http(s)://` 与 `mailto:`；Tauri 下 `openUrl`，网页预览下 `window.open(noopener)`；`a` 始终 `preventDefault`；远程图片改为链接；能力文件新增作用域受限的 `opener:allow-open-url` |
| M3 | 完成 | 标题、加粗、链接、引用、表格（`display:block` 横向滚动）、任务列表、分隔线样式，深浅主题共用 `--chat-*` |
| M4 | 完成 | `ParsedChatLine.block_start`；`Output.block_pending` 在下一段非空 delta 前插入 `block_separator`（已有空行则不加）；同一 `assistant` 消息内多个 text block 以空行连接；分隔随 delta 下发并入日志 |
| M5 | 完成 | `tests/unit/chatMarkdown.test.cjs` 6 项；Rust `text_blocks_are_marked_so_the_runtime_can_separate_them`、`consecutive_text_blocks_become_separate_paragraphs`（真实 `sh` 进程，Codex 消息间夹命令、Claude 工具前后两个文本块） |
| M6 | 完成 | `security-policy.md`、`requirements.md`、`chat-contracts.md`（delta 分隔约定）；预览会话补充标题、加粗、表格、链接与字面 HTML |

验证（2026-10-21）：

- `pnpm check` 通过：前端单测 92/92，`cargo fmt --check`、`clippy -D warnings`、Rust 测试 153 通过（17 ignored 为真实 CLI 用例）。
- 体积：主 chunk 52.04 kB → 225.10 kB（gzip 16.21 → 70.25 kB，+54 kB gzip，高于预估 40–50 kB）；OpenUI 懒加载 chunk 不变（gzip 63.09 kB）。本地桌面应用从磁盘加载，暂不拆分；如需收紧，可把 `Markdown` 改为懒加载。
- `scripts/debug.sh stop` → `pnpm smoke:visual` → `scripts/debug.sh stop` 通过；`designs/exports/simple-chat/chat-{light,dark}.png` 目视确认标题、加粗、表格、链接与字面 `<b>` 显示正常；OpenUI 卡片冒烟同时通过。
- 未验证：真实 Codex 会话的新回合分隔（单测以 Codex `--json` 事件格式覆盖，旧会话中已粘连的正文不会被改写）。

提交前复核（2026-10-26，当前分支已含 Skills 迁移）：

- 用户经 `scripts/debug.sh desktop` 验收：Chat 外链在系统浏览器打开，WebView 不导航。
- Rust `cargo test --lib chat` 100 通过（16 ignored），前端单测 125/125，`pnpm build` 通过。
- 体积现状：Skills 面板也引用 `Markdown` 后，构建把它拆为共享 chunk `MarkdownView`（175.62 kB，gzip 54.82 kB），主 chunk 190.92 kB（gzip 63.64 kB）；上文 225.10 kB 为 2026-10-21 单页构建数据。
