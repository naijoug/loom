# Learn：GitHub 项目学习研究模块实施计划

- 状态：L0–L5、L7 已实施，待用户验收；L6 深入追问按默认决策未做。L0 见 [探针记录](../../dogfood/2026-10-26-learn-probe.md)，验收见 [Learn 验收记录](../../dogfood/2026-10-26-learn-acceptance.md)，契约见 [Learn 契约](../../architecture/learn-contracts.md)
- 基线：`main` @ `d5b8c4b`
- 触发：用户要求新增学习研究模块：输入 GitHub 项目地址，选择 Agent 与模型，下载源码，输出完整项目架构、由浅入深的模块讲解和项目 Wiki，并能定位到源码位置
- 范围：新增顶层 Learn 视图、Rust `learn/` 服务、适配器模型选择能力；复用 Git 拉取、适配器、进程监督、流解析与 Markdown 渲染。不改 Chat v2 存储格式，不启用冻结 Workbench，不修改旧 Task 数据

## 0. 实施结果与偏差（2026-10-26）

本节记录实施现状，覆盖下文中与之不一致的设计细节；下文保留为原始计划。

| 计划 | 实现 | 原因 |
|---|---|---|
| 5.1 每页引用写 `refs.json` | 不落盘：`learn_read_page` 读取时逐条校验，页面清单只存 `refs: {valid, invalid}` 统计 | checkout 固定不变，读取时校验即可，避免两份数据不一致 |
| 5.2 拉取前经 GitHub API 读取大小与默认分支 | `--no-checkout` 浅克隆后用 `git ls-tree -r -l` 校验 ≤ 20,000 文件、≤ 500 MiB，再检出 | 不依赖 API 配额与认证；上限按实际跟踪文件计算 |
| 5.2 `architecture` 阶段由 Agent 输出 `overview.md` / `architecture.md` | Agent 只输出 JSON；两页由 Rust 从 `architecture.json` 渲染 | 少一次生成，页面与校验后的结构一致 |
| 5.2 提示词放 `src-tauri/resources/learn-prompts/` | `src-tauri/src/learn/prompts/`，`include_str!` 编译进二进制 | 与模块同目录，无运行时资源查找 |
| 5.4 `modelSelection: { supported, flag, presets }` | `modelSelection: bool`；参数名由适配器内部决定，UI 只填模型名 | L0 显示各 CLI 参数固定为 `--model`/`-m`，前端不需要参数名；无可靠预设列表 |
| 5.4 模型名字符集 `.-_/:` | `validate_model_name` 只限制：非空、≤ 128 字符、不以 `-` 开头、无空白与控制字符；按参数数组传递，不经 shell | L0 实测 `auto/glm-5.3[1m]`、`grok-4.7@500k`、`provider/model#variant` |
| L2 Settings → Agents 默认模型（可选） | 未做；模型只在 Learn 新建表单填写 | 可选项，不影响 Chat |
| L4 `parse_chat_stream_line` 改为 `pub(crate)` | 复用 `chat::runtime::run_owned` 与 `ChatTurnOutcome`（新增 `ProcessOwner::Learn`） | 一并复用输出限额、停止/超时与进程组清理 |
| 5.5 / L5 组件 `StudyList`、`StudyProgress`、`WikiTree`、`WikiPage`、`learnClient.ts`、`mockStore.ts` | `LearnPage`（含列表）、`StudyView`（进度/目录/页面/日志）、`NewStudyForm`、`ArchitectureDiagram`、`SourceViewer`；API 在 `src/api/learn.ts`，DEV fixture 在 `preview.ts` | 组件按实际复用边界合并 |
| 5.5 `MarkdownView` 增加 `sourceLinks` 上下文 | `Markdown` 增加可选 `links` 属性（`onSource` / `onWiki` / `sourceStatus`），另支持 `loom-wiki:` 链接 | 显式属性比上下文更易测；模块页之间需要互链 |
| 语言代码 `zh` | 统一为应用 locale `en` / `zh-CN` | 与 i18n 一致 |
| 8.1 入口“与 Chat、Settings 并列” | Chat 侧边栏底部 Learn 按钮切换到 `learn` 视图（Settings 入口同在侧边栏底部） | 现有 Chat 布局没有独立顶栏 |
| L5 可选导出 Wiki | 未做 | 可选项 |

验证（详见验收记录）：`pnpm check`、`pnpm tauri build --no-bundle`、`pnpm smoke:visual`（含 Learn 矩阵，截图在 `designs/exports/learn/`）通过；Codex 真实研究 hexyl 通过（825 s，11 个阶段，211 条引用全部有效，checkout 未变）；Claude Code 因本机 CLI 认证失败 blocked；用户桌面验收待进行。

## 1. 目标

1. 用户输入 `https://github.com/<owner>/<repo>`（可选分支 / tag / commit），选择已启用的 Agent 和模型，Loom 拉取源码并固定到一个 commit。
2. 生成**项目架构**：技术栈、入口、分层、模块清单、模块依赖图、未归类文件统计。
3. 生成**由浅入深的学习路径**：L1 全局认识 → L2 核心模块 → L3 内部机制，每级有学习目标和模块顺序。
4. 生成**项目 Wiki**：总览页、架构页、每个模块一页、术语表；页面之间可互相跳转。
5. **定位源码**：Wiki 中的源码引用可点开应用内源码查看器，跳到对应文件并高亮行范围；同时提供“在 GitHub 打开”（固定 commit 的永久链接）。
6. 结果持久化，重启后可读；生成可取消、可单步重试，已完成部分保留。

## 2. 非目标

- 不运行被研究项目的任何代码：不安装依赖、不构建、不启动、不跑测试。
- 首版不支持私有仓库令牌管理；GitLab、自建 Git 主机和本地目录研究不在首版（后两者见待确认问题）。
- 不做向量索引 / 嵌入检索；Agent 用 CLI 自带的读文件、搜索工具探索源码。
- 不修改被研究源码，不提供编辑器功能；不引入 Mermaid 等大型绘图依赖。
- 不自动跟踪上游更新；“在新 commit 上重新研究”作为后续能力。
- 不发布或上传 Wiki；首版只提供导出本地 Markdown 目录（L5 可选项）。

## 3. 成功标准

| # | 标准 | 判定方式 |
|---|---|---|
| S1 | 公开 GitHub 仓库可拉取并固定 commit；非法 URL、不存在、超限、网络失败分别给出可识别错误 | Rust 单测 + 真实拉取 2 个仓库 |
| S2 | 所选 Agent 与模型确实被使用：执行记录中可见模型参数；未选模型时使用 CLI 默认并标注 | 执行记录核对 + 适配器单测 |
| S3 | 架构结果通过 schema 校验；模块 `paths` 全部存在于 checkout；源码文件被模块覆盖率 ≥ 80%（Rust 计算，未覆盖文件可见） | Rust 校验 + 两个样例仓库实测 |
| S4 | 学习路径 ≥ 3 级；每个模块页至少 3 个有效源码引用；无效引用占比 < 5%，无效引用不可点击并标注 | Rust 引用校验统计 |
| S5 | 点击源码引用打开查看器、定位并高亮行范围；≤ 512 KiB 的文件完整显示，更大文件按窗口读取 | 前端单测 + smoke:visual |
| S6 | 生成全程只读：生成前后 checkout 指纹一致 | Rust 生成前后对比 |
| S7 | 取消、单步重试、重启后读取已完成内容可用；中断的作业标记 interrupted，不自动重跑 | Rust 单测 + 桌面验收 |
| S8 | Chat 行为不受影响：现有 `pnpm check` 全部通过 | 门禁 |

## 4. 当前状态（2026-10-26 核实）

### 已有、可复用

- **GitHub URL 规范化与浅克隆**：`src-tauri/src/skills/sources.rs::normalize_git_url` 只接受 github.com / gitlab.com 且拒绝 URL 凭据；`skills/operations.rs::import_using` 已实现 `clone --depth 1 --no-checkout`、通过 `.git/info/attributes` 禁用 smudge filter、`checkout --detach HEAD`、校验 40/64 位 revision，以及可注入的 `GitRunner`（测试用）。
- **GitHub API 固定主机访问**：`skills/github_api.rs` 已有 HTTPS 固定主机、禁用重定向的 GET 能力，可用于克隆前读取仓库大小和默认分支。
- **适配器**：`agent_adapter.rs::prepare_invocation` + `AdapterInvocationRequest`，`chat_permission_mode: Some("explore")` 对 Claude / Codex / OpenCode 映射到只读或 plan 策略（见 `docs/agent-adapter.md` 权限表）。
- **进程监督**：`process_supervisor.rs` 的 `ProcessOwner`（目前有 `Unscoped` / `Task` / `Chat`），提供进程组、超时、TERM/KILL 和退出协调。
- **流解析**：`chat/mod.rs::parse_chat_stream_line`（私有）可从各 CLI 的结构化输出中提取正文和工具事件。
- **一次性非 Chat 调用的先例**：`skills/translation.rs`（及 `translation/local`）在独立私有临时目录中只读运行本地 CLI，结果按 operationId 保存，是 Learn 生成作业可参考的结构。
- **渲染**：`src/features/chat/MarkdownView.tsx` 已用 GFM、HTML 按字面显示、外链经 opener 打开。
- **原子写入**：`storage.rs`。

### 缺失

- **没有模型选择**：`AgentConfig`（`models.rs:140`）和 `AdapterInvocationRequest` 都没有 model 字段，全仓库适配器不生成任何 `--model` 参数。这是本需求的前置能力。
- 顶层视图只有 Chat / Settings（`src/App.tsx`、`state/model.ts` 的 `currentView`）。
- 没有源码文件浏览/查看 IPC，也没有非 http(s)/mailto 链接的处理；`react-markdown` 默认 `urlTransform` 会清除自定义协议，需要显式放行。
- `docs/requirements.md` 当前范围写明“除独立 Skills 迁移外，不扩展 Sources/MCP、Browser、Pages……”，未包含 Learn。按用户本次明确要求，需在 L1 同步更新需求文档。

### 假设

- 各内建 CLI 的模型参数：Claude Code / Codex / OpenCode / Grok 均为 `--model`（L0 已实测，OpenCode 形如 `provider/model`）。
- ~~中等规模仓库架构阶段 20 分钟内完成~~：L0 实测 Codex 454 s、Grok 139 s，但代理模型下的 Claude 与 OpenCode 超过 47 分钟未完成；默认超时改为架构 30 分钟、模块页 15 分钟、术语表 10 分钟。

## 5. 方案设计

### 5.1 数据与存储

研究数据由 Loom 管理，放在应用数据目录，不写入用户项目，也不在 checkout 内创建 `.loom`，保证行号引用稳定：

```text
<appData>/learn/
  index.json                         # 可重建的研究列表缓存
  studies/<studyId>/
    study.json                       # url、owner/repo、ref、commit、agentId、model、语言、深度、promptVersion、状态
    checkout/                        # 固定 commit 的源码（保留浅 .git，便于核对 commit）
    inventory.json                   # Rust 静态清单：文件、语言统计、清单文件、候选入口
    architecture.json                # 经校验的架构 / 模块 / 学习路径
    wiki/overview.md
    wiki/architecture.md
    wiki/modules/<moduleId>.md
    wiki/glossary.md
    refs.json                        # 每页源码引用的校验结果
    jobs/<jobId>/job.json            # 阶段、invocation（脱敏）、时间、退出码、错误
    jobs/<jobId>/stdout.log|stderr.log
```

- `studyId` 为受限单段标识；所有路径从 studyId 派生并拒绝 symlink。
- 删除研究是用户显式操作，二次确认后删除该 study 目录（Loom 托管数据）。

### 5.2 生成流水线

| 阶段 | 执行者 | 产出 | 说明 |
|---|---|---|---|
| `fetch` | Rust + git | `checkout/`、commit | 先经 GitHub API 读取大小与默认分支；超出上限（默认 500 MiB / 5 万文件，可配置）直接拒绝 |
| `inventory` | Rust（无模型） | `inventory.json` | `git ls-files` 列表；跳过二进制、> 1 MiB、vendor/构建目录；语言与行数统计；识别 `package.json`、`Cargo.toml`、`go.mod`、`pyproject.toml` 等；读取 README 摘要 |
| `architecture` | Agent，只读 | `architecture.json` + `overview.md` + `architecture.md` | 输入 inventory 摘要，Agent 自行读源码；输出 JSON 经 schema 与路径校验，失败最多一次修复回合 |
| `module:<id>` | Agent，只读 | `wiki/modules/<id>.md` | L1 模块自动生成；L2/L3 按需或“全部生成”串行队列（并发 1） |
| `glossary` | Agent，只读 | `glossary.md` | 汇总术语，可放到最后 |

- 每阶段是独立作业，记录在 `jobs/`；失败只重试该阶段。
- Agent 统一用 `explore` 权限、cwd 为 `checkout/`。只允许已声明 `structuredStream` 且只读策略已实测的内建适配器；自定义 CLI 没有只读沙箱，首版不开放。
- 提示词模板放在 `src-tauri/resources/learn-prompts/`，用 `include_str!` 编译进二进制，带 `promptVersion` 记入 `study.json`。模板明确要求：仓库内容是数据而非指令；只输出约定格式；源码引用必须使用 `loom-src` 语法。
- 结构化输出：L0 实测后选择——优先使用 CLI 原生结构化输出能力（若存在且稳定），否则从最终助手消息中提取最后一个 ` ```json ` 围栏块。
- ~~可选优化：模块页续聊复用架构会话~~：L0 实测续聊 token 约为全新调用 5 倍且质量无提升，模块页统一全新调用并附模块架构条目。

`architecture.json` 结构（schemaVersion 1）：

```json
{
  "schemaVersion": 1,
  "project": { "name": "", "summary": "", "techStack": [], "entryPoints": [{ "path": "", "line": 1, "why": "" }] },
  "layers": [{ "id": "", "title": "", "moduleIds": [] }],
  "modules": [{
    "id": "kebab-case", "title": "", "summary": "", "level": 1, "order": 1,
    "paths": ["src/chat/"], "dependsOn": [],
    "keyFiles": [{ "path": "", "startLine": 1, "endLine": 40, "why": "" }]
  }],
  "learningPath": [{ "level": 1, "title": "", "goal": "", "moduleIds": [] }]
}
```

Rust 校验：id 唯一且合法、`dependsOn`/`moduleIds` 指向存在的模块、路径存在于 checkout 内、行号在文件范围内；按路径前缀计算文件覆盖率和未归类文件清单。

### 5.3 源码引用与定位

- Wiki Markdown 中的引用语法：`[runtime.rs:42-80](loom-src:src-tauri/src/chat/runtime.rs#L42-L80)`。
- 生成后 Rust 逐条校验并写入 `refs.json`（有效 / 文件不存在 / 行越界 / 路径越界）。
- 前端 `Markdown` 组件增加可选 `sourceLinks` 上下文：自定义 `urlTransform` 只额外放行 `loom-src:`；有效引用渲染为源码链接，无效引用显示为纯文本加警告标记。Chat 中不传该上下文，行为不变。
- 源码查看器：`learn_read_source` 返回有界文本（≤ 512 KiB 完整返回，否则按行窗口），带总行数、是否截断、按扩展名推断的语言；前端显示行号、高亮范围并滚动到起始行。首版不引入语法高亮依赖。
- “在 GitHub 打开”：`https://github.com/<owner>/<repo>/blob/<commit>/<path>#L<a>-L<b>`，复用现有外链打开逻辑。

### 5.4 模型选择（适配器能力）

- `AgentCapabilities` 新增 `modelSelection: { supported: bool, flag: string, presets: string[] }`；`AdapterInvocationRequest` 新增 `model: Option<&str>`。
- 模型名校验：1–128 字符，不含空白和控制字符，不能以 `-` 开头（L0 发现 `auto/glm-5.3[1m]`、`grok-4.7@500k`、`provider/model#variant` 等真实名称，原 `.-_/:` 字符集过窄）；不支持的适配器传入模型时报错，而不是静默忽略。
- 模型不提供可靠的列表接口，UI 为“CLI 默认 + 预设 + 自定义输入”。
- 模型纳入执行记录；Chat 的续聊指纹逻辑本轮不变（Chat 暂不暴露模型选择，见待确认问题 5）。

### 5.5 界面

- 顶层导航：Chat / Learn / Settings。切换到 Learn 时保留 Chat 挂载与草稿（与 Settings 的现有做法一致）。
- Learn 左栏：研究列表；选中后显示 Wiki 目录（总览、架构、学习路径 L1/L2/L3 下的模块、术语表），未生成页面显示“生成”按钮。
- 新建研究：URL、可选 ref、Agent（复用 `AgentPicker`）、模型、输出语言（默认跟随界面语言）、深度（快速 / 标准 / 深入，影响模块数量与页面篇幅）。
- 进度视图：阶段列表、状态、耗时、执行日志入口（参考 `ChatRunDetails`）、取消、重试。
- 架构页：基于 `architecture.json` 自绘分层模块图（SVG，层为行、依赖为连线），点击模块跳到模块页；不引入绘图依赖。
- 主区 Wiki 页面 + 可拖拽分栏的源码查看器；窄窗口下查看器改为覆盖层。
- 文案进入 `src/i18n/` 英文源字典与简体中文。

### 5.6 IPC

| Command | Input | Output |
|---|---|---|
| `learn_list_studies` | `{}` | 研究摘要数组（含 `storageError`） |
| `learn_create_study` | `{ input: { url, ref?, agentId, model?, language, depth } }` | `Study`（随即开始 fetch） |
| `learn_get_study` | `{ studyId }` | `Study`（含阶段状态、architecture、页面清单） |
| `learn_read_page` | `{ studyId, pageId }` | `{ markdown, refs }` |
| `learn_generate` | `{ studyId, stage }` | `{ jobId }`；stage 为 `architecture` / `module:<id>` / `glossary` / `all` |
| `learn_cancel_job` | `{ studyId, jobId }` | null |
| `learn_read_job_logs` | `{ studyId, jobId, stream, offset, limit? }` | `{ text, nextOffset, hasMore }` |
| `learn_list_tree` | `{ studyId, dir }` | 目录项（按需展开，跳过 `.git`） |
| `learn_read_source` | `{ studyId, path, startLine?, endLine? }` | `{ text, firstLine, totalLines, truncated, language }` |
| `learn_delete_study` | `{ studyId }` | null |

事件：`loom://learn-event` `{ studyId, jobId, stage, status, progress?, error? }`。命令、事件和样本加入 `contracts/tauri-contract.json`，两端共同验证。

### 5.7 安全边界

- 被研究仓库是不可信输入：可能含提示词注入。约束靠只读权限、固定 cwd 和输出校验，不靠提示词。生成前后计算 checkout 指纹，若有变化则作业失败并提示。
- 克隆沿用 Skills 的 filter 禁用；不拉取 submodule、不启用 LFS；git 子进程不经 shell。
- 所有读取 IPC 在 Rust 中把路径解析到 `checkout/` 内，拒绝 `..`、绝对路径和 symlink 逃逸；读取有大小上限，二进制返回“不可显示”。
- 生成内容按现有 Markdown 安全策略渲染；`loom-src:` 只用于应用内导航，不交给 opener。
- 执行日志沿用现有脱敏。

## 6. 里程碑与任务

依赖关系：L0 → L2、L4；L1 → L3 → L4 → L5；L5 可在 L3 契约确定后先用 mock 开发；L6 可选；L7 收尾。L2 与 L3 可并行。

### L0 CLI 与成本探针（不合入产品代码）

- 产出：对 Claude Code / Codex / OpenCode（Grok 若额度允许）实测：模型参数名与报错行为；只读模式下长 JSON 输出的完整性；原生结构化输出能力是否可用；两个样例仓库（约 5 千行、约 5 万行）架构阶段的耗时、token 与引用有效率；续聊复用与全新调用的成本对比。
- 文件：`docs/dogfood/2026-10-xx-learn-probe.md`；探针脚本放 scratchpad，不提交。
- 验证：每项结论附命令与版本；未跑或失败的项如实标注。
- 出口：确定结构化输出方式、各适配器模型参数、默认超时与仓库上限。

### L1 文档与共享 Git 拉取

- 产出：更新 `docs/requirements.md` 当前范围（Learn 顶层入口与边界）、`docs/architecture.md` 模块表、`docs/security-policy.md` Learn 小节、新增 `docs/architecture/learn-contracts.md`；把 `skills/operations.rs` 中的 git 运行与浅克隆逻辑抽出为共享模块（如 `src-tauri/src/git_fetch.rs`），Skills 行为不变。
- 依赖：无。
- 验证：Skills 现有 Rust 测试全部通过；文档相对链接检查。

### L2 适配器模型选择

- 产出：`AgentCapabilities.modelSelection`、`AdapterInvocationRequest.model`、各适配器生成模型参数、模型名校验、执行记录包含模型；前端 `domain/agent.ts` 类型同步；Settings → Agents 可选设置每个 Agent 的默认模型（可选，若做则持久化到 AgentConfig 的新可选字段，旧配置缺省为空）。
- 文件：`agent_adapter.rs`、`models.rs`、`agents/config.rs`、`src/domain/agent.ts`、`contracts/tauri-contract.json`。
- 依赖：L0 的参数结论。
- 验证：适配器单测覆盖每个 CLI 的参数、非法模型名、不支持时报错；Chat 现有测试不变通过。

### L3 Learn 后端：研究存储、拉取、清单、源码读取

- 产出：`src-tauri/src/learn/{mod,models,store,fetch,inventory,source,commands}.rs`；`ProcessOwner::Learn { study_id, job_id }`；`learn_list_studies` / `learn_create_study`（只到 fetch + inventory）/ `learn_get_study` / `learn_list_tree` / `learn_read_source` / `learn_delete_study` / `learn_cancel_job`；作业状态与重启后 interrupted 恢复；`learn-event`。
- 依赖：L1。
- 验证：用注入的 GitRunner 和本地 fixture 仓库测试拉取成功、超限、取消；路径逃逸、symlink、二进制、超大文件的读取测试；重启恢复测试；契约测试。

### L4 生成流水线

- 产出：提示词模板与 `promptVersion`；`learn/generate.rs`（调用 `prepare_invocation`，`explore` 权限、cwd=checkout、可选 model）；把 `parse_chat_stream_line` 改为 `pub(crate)` 复用；结构化输出提取、schema 与路径校验、一次修复回合；Markdown 引用解析与 `refs.json`；覆盖率计算；生成前后 checkout 指纹比对；`learn_generate` / `learn_read_page` / `learn_read_job_logs`；“全部生成”串行队列。
- 依赖：L0、L2、L3。
- 验证：用 fixture CLI 子进程（参考 `pnpm smoke:chat` 的本地 fixture 方式）覆盖正常输出、坏 JSON 后修复、修复仍失败、引用越界、超时、取消、写 checkout 被检出；对 L0 的两个样例仓库各跑一次真实生成并记录指标。

### L5 前端 Learn 视图

- 产出：`src/features/learn/`（`LearnPage`、`StudyList`、`NewStudyForm`、`StudyProgress`、`WikiTree`、`WikiPage`、`ArchitectureDiagram`、`SourceViewer`、`learnClient.ts`、`mockStore.ts`）；`App.tsx` 与 `state/model.ts` 增加 `learn` 视图并保留 Chat 挂载；`MarkdownView` 增加可选 `sourceLinks`；i18n；可选：导出 Wiki 为本地 Markdown 目录。
- 依赖：L3 契约（可先用 mock），L4 完成后接真实数据。
- 验证：单测覆盖 `loom-src` 链接解析与无效引用、查看器定位与高亮、视图切换保留 Chat 草稿；`pnpm build`；记录新增 chunk 体积（Learn 懒加载，主包增量目标 < 5 KB gzip）。

### L6 深入追问（可选，按待确认问题 4 决定）

- 产出：在模块页发起追问，复用 Chat 运行链；需要让 checkout 以“隐藏项目”登记，Chat 项目树过滤 Learn 项目，会话数据写在 study 目录外的受控位置，避免改变 checkout 内容。
- 依赖：L5。
- 验证：追问不改变 checkout 指纹；Chat 项目树不显示 Learn 项目。

### L7 视觉回归、真实验收与资料

- 产出：`pnpm smoke:visual` 增加 Learn 矩阵（新建、进度、Wiki 目录、引用跳转、无效引用、深浅主题、中英文、窄窗口），截图放 `designs/exports/learn/`；真实桌面验收清单写入 `docs/testing.md`；验收记录 `docs/dogfood/2026-10-xx-learn-acceptance.md`；回填本计划状态与 `docs/PLANS.md`。
- 依赖：L5（L6 若做）。
- 验证：`pnpm check`、`pnpm tauri build --no-bundle`、`smoke:visual`；Claude Code 与 Codex 至少各一次完整真实研究。

## 7. 风险

| 风险 | 影响 | 缓解 |
|---|---|---|
| 大仓库耗时与额度消耗高 | 用户等待久、费用高 | 克隆前读大小并拒绝超限；深度分级；模块页按需生成；L0 记录实测成本并在新建表单显示预估 |
| 模型编造路径或行号 | 定位失效、误导学习 | Rust 逐条校验；无效引用不可点击；架构阶段校验失败触发修复回合；指标进入验收 |
| 仓库内容提示词注入 | Agent 越权或输出被操纵 | 只读权限 + cwd 固定 + 前后指纹比对；输出只按 schema 使用；不开放自定义 CLI |
| 各 CLI 模型参数与结构化输出差异 | 适配复杂、行为不一致 | L0 先实测；能力声明驱动 UI；不支持时明确报错 |
| CLI 只读策略并非 OS 沙箱 | 极端情况下仍可能写文件 | 指纹检测并让作业失败；文档如实说明保障来源 |
| 新顶层入口扩大产品范围 | 与现行需求冲突 | L1 先更新需求文档并说明是用户本次明确要求 |
| 磁盘占用累积 | 应用数据膨胀 | 研究列表显示占用；显式删除；浅克隆 |

回滚：Learn 为独立模块、独立数据目录。回滚时移除顶层入口与 `learn` 命令注册即可，Chat 与 Skills 数据不受影响；L1 的 Git 抽取是行为不变的重构，可单独回退。

## 8. 待确认问题

均已给出默认做法，不阻塞 L0–L3 开工：

1. **入口位置**：默认新增顶层 Learn（与 Chat、Settings 并列）。备选：放进 Chat 项目树作为特殊项目。
2. **源码存放位置**：默认应用数据目录。备选：让用户指定一个工作目录（便于用自己的编辑器打开）。
3. **私有仓库与其他主机**：默认首版仅公开 GitHub HTTPS。备选：允许 SSH（沿用用户自己的 git/ssh 配置，Skills 已有此通路）；后续支持本地目录研究。
4. **深入追问**：默认把 L6 放在首版之后。备选：首版即做。
5. **Chat 是否同步提供模型选择**：默认 L2 只在适配器层实现、仅 Learn 暴露；Chat 暴露另开小计划（涉及续聊指纹）。

## 9. 验证策略

- 每个里程碑按 [验证矩阵](../../testing.md#开发变更验证矩阵) 运行相关检查：L1/L2/L3/L4 为 Rust + 契约，L5 为前端单测与构建，L7 为里程碑完整门禁。
- 真实模型调用会消耗额度：未运行、登录失败、额度不足分别记录，不写成通过。
- 预览前后执行 `scripts/debug.sh stop`；网页预览用 `scripts/debug.sh web`，桌面用 `scripts/debug.sh desktop`。
