# OpenUI 内联卡片 O0 可行性探针记录（2026-10-20）

- 对应计划：[OpenUI 内联卡片实验](../plans/2026-10-20/18:25-openui-inline-cards-experiment.md) O0
- 基线：Loom `codex/simple-cli-chat` @ `fd6ca41`。本探针**不改 Loom 产品代码**，所有代码和依赖都在仓库外的临时目录（会话 scratchpad 下的 `openui-probe/`），不保留为仓库产物。
- 环境：
  - macOS（Darwin 25.5.0），Node 26.11.0，pnpm 10.29.2
  - Claude Code 2.1.304
  - 依赖：`@openuidev/react-lang` 1.0.1（传递依赖 `lang-core` 1.0.1、`devtools` 0.3.0、`observability` 0.0.4、`ci-info` 4.4.0），`zod` 4.6.7，React 18.3.1，Vite 7.3.7，`@vitejs/plugin-react` 4.7.0
- **模型说明**：本机 Claude Code 通过 `~/.claude/settings.json` 指向本地代理，实际后端模型是 `GLM-5.3`（配置名 `group/auto-glm-5-3[1m]`），不是 Anthropic 的 Claude 模型。下面的真实模型结论只代表“Claude Code CLI + GLM-5.3”这个组合。
- 结论：**O0 通过，可以进入 O1**。CLI 支持追加提示词；预期出卡片的有效回合都产出了可解析的卡片（3/3，加续聊 1/1），不需要卡片的问题没有出卡片。需要据此调整计划中的两处设计（见“对计划的修正”）。

## 1. 提示词

组件库共 5 个组件：根容器 `CardStack`，加 `FileList`、`ChangeSummary`、`DataTable`、`Checklist`。所有 props 都是纯文本、数字或布尔。

用 `library.prompt({ inlineMode: true, preamble, additionalRules, examples })` 生成提示词，额外规则有四条：

- 只在结构化视图确有帮助时才用卡片。
- 每个代码块都是完整、独立的程序。
- 每次回复最多一个代码块。
- 卡片文字是纯文本。

| 指标 | 结果 |
|---|---|
| 提示词大小 | 4682 字符 / 4708 字节 / 78 行，按字符数估约 1.2k token |
| 实测每次 API 调用的输入增量 | 同一条无工具提问，追加提示词后输入为 12277 token，不追加为 11229 token，**增量 1048 token**（GLM 分词，单次样本） |

OpenUI 内置的内联模式说明是按“仪表盘迭代”写的：要求修改时只输出变更语句，提问时只回文字。这和 Loom 每张卡片相互独立的场景冲突。本次通过追加规则覆盖，续聊回合实际输出了完整程序（见 §4 的 q5）。

## 2. 渲染与加载（工程层）

| 检查 | 结果 |
|---|---|
| React 18.3.1 + `renderToString` 渲染合法卡片 | passed |
| 未知组件（`Chart`） | 解析器报 `unknown-component`，`Renderer` **静默渲染空容器**，SSR 下 `onError` 未被调用 |
| 参数类型错误 | 解析器报 `type-mismatch` ×2，`Renderer` 同样静默渲染空容器 |
| 流式截断（字符串没写完） | 渲染出半截文本（`src/ma`），完成后正常 |
| 字符串中的 `<img onerror>`、`<script>` | 以转义文本输出，不执行 |
| CommonJS `require("@openuidev/react-lang")` | passed（`Renderer`、`createLibrary`、`defineComponent`、`createParser` 均可用） |

Vite 生产构建（入口里用 `React.lazy` 拆分卡片）：

| chunk | 原始大小 | gzip |
|---|---|---|
| 卡片 chunk（react-lang + zod + 组件库） | 194,526 B | 60,478 B |
| 入口 chunk（React） | 143,022 B | 46,242 B |

构建产物中没有 CDN 地址；是否使用 devtools 别名，产物大小相同。zod 4.6.7 会产生 Rollup 的 `@__PURE__` 注释位置警告，不影响构建。

## 3. 网络与安装脚本

| 检查 | 结果 |
|---|---|
| `vite` 开发服务器，不加别名 | **页面挂载 devtools，并加载外部脚本** `https://cdn.jsdelivr.net/npm/@openuidev/devtools@0/dist/devtools.browser.js` |
| `vite` 开发服务器，把 `@openuidev/devtools` 别名到本地空模块 | 外部请求为 0；页面仍会插入一个空的挂载节点，卡片正常渲染 |
| pnpm 10.29.2 + 与 Loom 相同的 `allowBuilds: { esbuild: true }` | 输出 `Ignored build scripts: @openuidev/lang-core@1.0.1`，postinstall 遥测脚本未执行 |

开发服务器的验证用的是内置浏览器，读取 `performance` 资源列表；端口用 5199，不占用 Loom 的 1420。验证结束后服务已停止。

## 4. 真实 Claude Code

调用参数与 Loom 的 `ClaudeAdapter` 一致：

```text
claude -p --verbose --output-format stream-json --include-partial-messages --permission-mode plan [--append-system-prompt <INLINE_PROMPT>] [--resume <id>]
```

正文走 stdin。工作目录是一个含 7 个文件的合成 git 项目（`fixture-calc`）。

| 回合 | 追加提示词 | 结果 | 卡片 | 解析 |
|---|---|---|---|---|
| q1 列入口文件 | 是 | 读取文件后作答 | 1（FileList，7 项） | root 在首行，0 错误 |
| q1-baseline 同上 | 否 | 用 Markdown 表格作答 | 0 | — |
| q2 对比两个文件，用表格 | 是 | **无效**：模型声称没有文件读取工具，未作答 | 0 | — |
| q3 add 的返回类型 | 是 | **无效**：同样未读到文件；回复是纯文字，没有卡片，符合预期 | 0 | — |
| q4 发布前检查清单 | 是 | 读取后作答 | 1（Checklist，12 项） | root 在首行，0 错误 |
| q5 续聊 q1：“把 tests 也加进去” | 是 | 输出完整的两张卡片程序，而不是增量语句 | 1 | root 在首行，0 错误 |
| q6 续聊 q5，**不追加**提示词 | 否 | 只回 Markdown 列表 | 0 | — |

由此得出：

- `--append-system-prompt` 在 `-p` 模式下生效，配合 `--resume` 也生效。
- 追加的提示词**不会随续聊句柄保留**，每轮都必须重新传入（q6）。
- 能解析的卡片：预期出卡片的有效回合 2/2，加续聊 1/1，合计 3/3，解析错误 0。q2/q3 因工具不可用而无效，不计入成功率。
- 合成项目在全部回合后 `git status` 为空，没有被写入。

**附带观察**（与 OpenUI 无关，但影响验收口径）：

- **q2/q3 的工具问题**：GLM-5.3 在 plan 模式下先反复调用 ToolSearch，随后误判没有 Read 工具。这是本机代理模型的行为，Loom 现有的 Research 模式也会遇到。
- **plan 模式会写文件**：q4 在 plan 模式下写了 `~/.claude/plans/0-2-0-drifting-cosmos.md`。这是 Claude Code plan 模式自己的计划文件，在项目之外，不是项目写入。
- **全局指令污染输出**：用户全局的 `~/.claude/CLAUDE.md`（英语改写指令）会出现在部分回复的开头，真实 Loom 使用中也会如此。
- **耗时差异大**：q1-baseline 用了 18 个回合、1137 秒，耗时由工具探索主导。各回合的累计 token 和 `total_cost_usd` 受工具路径影响，不能用来比较提示词成本；`total_cost_usd` 是按未识别模型估算的，不是真实费用。

**环境注意**：在 Claude Code 桌面会话内直接启动子 `claude`，会继承宿主会话注入的 `ANTHROPIC_AUTH_TOKEN`、`ANTHROPIC_BASE_URL` 等变量，覆盖用户自己的配置，导致 `401 Invalid bearer token`。本次第一轮 7 个回合全部因此失败。改用 `env -i HOME PATH USER LANG TMPDIR` 的干净环境后通过。如果从代理会话中启动 Loom 调试并调用 CLI，也会遇到同样的问题。

## 5. 对计划的修正

1. **回退判断改用解析结果。** 不能依赖 `Renderer` 的 `onError`：非法卡片会被静默渲染成空容器，SSR 下也不触发 `onError`。O3 应该用 `createParser(library.toJSONSchema(), "CardStack")` 或 `onParseResult` 判断：完成后 `meta.errors` 非空，或 `root` 为空、没有子卡片时，回退为源码。
2. **提示词每轮注入。** O2 必须在每轮（包括续聊）都追加提示词。计划原本就按每轮准备参数，这里明确记录依据。
3. **保留覆盖规则。** 内联模式的默认说明会诱导模型只输出增量语句，O1 的额外规则必须保留“每个代码块都是完整程序”，并在单测中固定。
4. **组件签名把必填参数放前面。** 可选参数只能从末尾省略，`FileList(title?, files)` 这类“可选参数在前”的签名迫使模型必须写出 title。O1 应把必填参数放前面，例如 `FileList(files, title?)`。
