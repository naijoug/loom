# Learn 新建页：GitHub 热门与发现对话计划

- 创建时间：2026-11-03 18:12，America/Los_Angeles。
- 状态：**D1、D2 已完成；D3 部分完成**（真实 GitHub API 与真实 Codex 回合已验证；桌面窗口内点击操作未验证）。
- 代码基线：`main` `b39e27d`。
- 用户要求（2026-11-03，桌面截图）：Learn 新建学习页需要优化；接入 GitHub API 显示 GitHub trending；右侧支持 Chat 模式，与 LLM 对话搜索感兴趣的项目。

## 1. 现状

- `NewStudyForm.tsx` 只有一张居中表单（URL、Agent/模型、语言、深度、ref），用户必须事先知道仓库地址；1280px 窗口两侧大片空白。
- 后端只有 Learn 学习流水线命令；仓库没有“列出热门仓库”或“按兴趣找仓库”的能力。`skills/github_api.rs` 有 GitHub API 客户端，但与 Skills 操作记录耦合，不复用。
- `reqwest` + `rustls` 已是依赖（固定主机读取 GitHub API），无需新增依赖。

## 2. 目标

1. 新建页改为两栏：左侧主列为紧凑的新建表单 + “GitHub 热门”列表；右侧为“发现”对话面板。窄窗口（< 1180px，按 D2 实现同步）对话面板收起为可切换的抽屉。
2. GitHub 热门：用 GitHub REST 搜索 API 近似 trending——指定时间窗内**新创建**的仓库按 star 排序（今日 / 本周 / 本月），可按语言筛选。界面如实标注“按时间窗内新建仓库的 star 排序”，不宣称等同 github.com/trending。
3. 发现对话：用户用自然语言描述兴趣；所选 Agent（内建 CLI 或直连 API）回复推荐说明，并在末尾输出结构化 JSON（搜索词 + 具体仓库名）；Loom 在 Rust 中校验后调用 GitHub 搜索，把真实存在的仓库作为卡片附在回复下。
4. 热门列表与对话卡片都可“学习此项目”（填入表单 URL 并聚焦）或在系统浏览器打开 GitHub。

## 3. 非目标

- 不抓取 github.com/trending HTML；不新增依赖；不支持 GitHub 登录或在 UI 配置 token（后端可选读取 `GITHUB_TOKEN` / `GH_TOKEN` 环境变量以提高限额，沿用 Skills 约定）。
- 发现对话不持久化为会话历史（Learn 页保持挂载，切页不丢）；每回合写脱敏执行记录。
- 不让 Agent 直接联网或使用工具；GitHub 请求只由 Rust 发起。

## 4. 设计

### 4.1 IPC（仅主窗口）

| 命令 | 参数 | 返回 |
|---|---|---|
| `learn_github_trending` | `{ period: "day" \| "week" \| "month", language? }` | `{ repos: GithubRepoCard[], query, fetchedAtMs, cached }` |
| `learn_discover_send` | `{ input: { runId, agentId, model?, language, messages: [{ role: "user" \| "assistant", text }] } }` | `{ reply, repos, queries, warnings }` |
| `learn_discover_cancel` | `{ runId }` | 是否有回合被取消 |

`GithubRepoCard`：`{ fullName, owner, name, url, description?, language?, stars, forks, topics, pushedAt?, createdAt? }`；`url` 一律由 Rust 按 `https://github.com/<owner>/<name>` 生成。

### 4.2 GitHub 客户端（`learn/github.rs`）

- 只访问 `api.github.com` 固定路径：`/search/repositories`，以及搜索未命中的具名仓库（≤ 3 个）的 `/repos/<名>` 与一次改名跳转 `/repositories/<数字 id>`（D3 补充）；客户端不自动跟随重定向，连接 10 s / 总 20 s 超时，响应 ≤ 4 MiB；错误不回显响应正文，按 401/403/422/429/限流分类为受控文案。
- 热门查询：`created:>=YYYY-MM-DD`（UTC，1/7/30 天）+ 可选 `language:<名>`（白名单字符），`sort=stars&order=desc&per_page=20`；进程内缓存 10 分钟。
- 对话检索：Agent 给出的仓库名经 `owner/repo` 格式校验，合并为一次 `repo:a/b repo:c/d` 搜索；搜索词去控制字符、≤ 200 字符，最多 2 条、每条取前 5；对话检索用 GitHub 默认相关度排序（D3 补充：按 star 排序会把高 star 的无关仓库排到前面）。每回合最多 3 次搜索请求，结果去重、上限 12 张卡片。

### 4.3 发现对话回合（`learn/discover.rs`）

- Agent 解析：已启用且可用；内建 CLI 或 `api` 执行器（`needs_tools: false`），可选模型经 `validate_model_name`。
- 提示词 `prompts/discover.md`：对话历史作为非可信资料隔离，最近 12 条、总计 ≤ 16k 字符；要求用所选语言简短回答，末尾给 ```json `{ "queries": [...], "repos": [...] }`。
- 运行：`prepare_invocation`（`stage = Planning`、`explore`、cwd = `<appData>/learn/discover/workspace/` 空目录），`ProcessOwner::Feature { feature: "learn-discover" }`，超时 5 分钟、输出 8 MiB；取消登记在进程内 runId 表。
- 记录：`<appData>/learn/discover/runs/<runId>/{prompt.md, stdout.log, stderr.log, run.json}`，脱敏，保留最近 50 个。
- 回复文本去掉 JSON 块后返回；JSON 缺失或无效只记警告，不让回合失败。

### 4.4 前端

- `NewStudyForm` 拆为表单（支持外部预填 URL）+ `TrendingList` + `DiscoverChat`，新增 `api/learn.ts` 方法；DEV fixture 提供合成热门与回复。
- 对话面板：消息列表（Markdown 渲染）、仓库卡片、Agent 选择（含直连 API）、输入框（Enter 发送、Shift+Enter 换行）、停止、清空。

## 5. 里程碑

| 阶段 | 内容 | 出口 |
|---|---|---|
| D1 · 后端 | `github.rs`、`discover.rs`、提示词、三个命令、契约 JSON / TS 常量 | `cargo test` 覆盖查询构造、仓库名校验、JSON 提取、响应解析；契约测试通过 |
| D2 · 前端 | 两栏新建页、热门列表、发现对话、i18n、fixture | `pnpm check` 通过 |
| D3 · 验收 | Web fixture 预览目视；桌面真实 GitHub 请求与一次真实 Agent 回合 | 截图存 `.designs/`；未跑的项明确标注 |

## 6. 风险

- 未认证搜索 API 限额 10 次/分钟：热门缓存 + 每回合 ≤ 3 次请求；限流时显示受控提示。
- “热门”只是近似：界面标注口径。
- Agent 可能编造仓库：只展示 GitHub 搜索实际返回的仓库。

## 7. 待确认问题

无。口径（搜索 API 近似 trending、对话不持久化）按第 3 节默认实施，用户可在验收时调整。

## 8. 验证策略

按 [验证矩阵](../../testing.md#开发变更验证矩阵)：Rust 单测、`pnpm check`、Web fixture 预览；真实 GitHub / Agent 链接在桌面运行验证。

## 9. 完成记录（2026-11-03）

- D1：`learn/github.rs`、`learn/discover.rs`、`prompts/discover.md` 与 `learn_github_trending` / `learn_discover_send` / `learn_discover_cancel` 已落地，契约 JSON、`contracts.rs`、`src/api/contract.ts` 同步。
- D2：`NewStudyPage`（两栏，< 1180px 改为窗口栏按钮打开的面板）、`TrendingList`、`DiscoverChat`、`RepoCard`、en / zh-CN 文案、DEV fixture 已完成；对话状态放在 `LearnPage`，打开研究再返回不丢失。
- 验证（已运行）：`pnpm check` 通过（层检查、Prettier、发布文档、159 个前端单测、构建、`cargo fmt --check`、clippy `-D warnings`、`cargo test` 438 通过）；Web fixture 在 1280×820 深色与 960×760 浅色下目视检查两栏 / 面板、示例提问、仓库卡片、警告和“学习”填入 URL。实施前用 curl 确认 `created:>… language:Rust` 按 star 排序与多 `repo:` 查询返回预期结果。
- D3（用户确认后执行）：
  - 新增 opt-in 真实测试 `learn::real_tests::github_trending_live`（不耗模型额度）与 `codex_discovers_repositories` / `claude_discovers_repositories`。
  - `github_trending_live` 通过：三种窗口各 20 个仓库、第二次命中缓存、`zyedidia/micro` 解析为 `micro-editor/micro`、不存在的仓库返回空。
  - 真实运行暴露并修复三处问题：①“今日”原用 `created:>昨天`，只覆盖 UTC 当天几小时（Rust 榜首 ★3），改为 `>=`；② 搜索词按 star 排序返回 `icloudbypass` 等无关仓库，改为默认相关度；③ 改名仓库（`zyedidia/micro`）在 `repo:` 搜索中被判为不存在、单独出现时整条查询 422，改为逐个补查并跟随一次改名。
  - `codex_discovers_repositories` 通过（Codex CLI，zh-CN，两回合 40.1 s / 30.2 s）：回答去掉 JSON 块，卡片均为相关的终端编辑器仓库，改名提示与 `pflenker/hecto`（实际 404）未找到警告正确，`run.json` 状态 complete。
  - `claude_discovers_repositories` 未通过：本机 Claude Code 在测试进程环境下报 `unrecognized_model`（`auto/glm-5.3[1m]`），属本机 CLI 模型配置，未改动该配置。
  - 修复后 `pnpm check` 与 `pnpm smoke:visual`（含 Learn 合成矩阵）通过；Web fixture 中文浅色 1280 目视正常，截图在 `.designs/exports/learn-discover/`。
- 未验证：在桌面窗口内点击操作（本会话无桌面操控工具）、直连 API（`api`）Agent 回合、真实限流提示。
