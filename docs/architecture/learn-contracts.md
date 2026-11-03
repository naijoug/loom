# Learn 契约

状态：2026-10-26 实现（L1–L5、L7；L6 深入追问未做）。计划见 [Learn 实施计划](../plans/2026-10-26/19:26-learn-repo-study-module.md)，安全边界见 [安全策略](../security-policy.md#learn)，公共运行/生命周期接口见 [Agent 层契约](agent-layer.md)。命令、事件与模型样本由 `contracts/tauri-contract.json` 两端共同校验。

## 存储

研究只写入应用数据目录，不写入用户项目：

```text
<appData>/learn/studies/<studyId>/
  study.json          # Study：仓库、固定 commit、Agent/模型、语言/深度、阶段、页面清单、覆盖率、checkout 指纹
  checkout/           # 浅克隆；生成全程只读
  inventory.json      # Rust 静态清单：语言行数、manifest、README 摘录、跟踪文件
  architecture.json   # 经校验的架构（schemaVersion 1）
  wiki/overview.md  wiki/architecture.md  wiki/glossary.md  wiki/modules/<moduleId>.md
  jobs/<jobId>/{job.json, prompt.md, stdout.log, stderr.log}
```

- `study.json` 原子写入；读取时把上个进程遗留的 `running` 阶段标为 `interrupted`，不自动重启。
- 同一研究同一时刻最多一个作业（进程内登记表）；删除研究前必须没有运行中的作业。
- `overview.md` / `architecture.md` 由 Rust 从 `architecture.json` 渲染；模块页与术语表由 Agent 逐页生成。

## 流水线

| 阶段 | 实现 | 超时 | 产出 |
|---|---|---|---|
| `fetch` | 加固 git 浅克隆（`--no-checkout`），`ls-tree` 校验 ≤ 20,000 文件、≤ 500 MiB 后再检出；记录 commit 与指纹 | git 600 s | `checkout/` |
| `inventory` | `git ls-files -z` + 有界扫描（跳过 > 1 MiB、二进制与依赖/构建目录） | — | `inventory.json` |
| `architecture` | Agent 只读调用；JSON 提取 → schema 与路径校验 → 失败时一次修复回合 | 30 min | `architecture.json`、渲染页、页面清单、覆盖率（< 80% 记警告） |
| `module:<id>` | 每页一次全新 Agent 调用；≥ 3 条有效源码引用，否则修复一次，保留较好结果并记警告 | 15 min | `wiki/modules/<id>.md` |
| `glossary` | 同上，≥ 1 条有效引用 | 10 min | `wiki/glossary.md` |

- 新建研究自动串联 `fetch → inventory → architecture → 全部 Level 1 模块页`；`all` 生成其余缺失模块页与术语表。
- Agent 调用经 `prepare_invocation`：`stage = Planning`、`chat_permission_mode = explore`、cwd = checkout、可选 `model`。进程归属 `ProcessOwner::Learn`，取消映射为中止；应用退出时阶段记为 `interrupted`。
- 每次 Agent 运行前后比对 checkout 指纹；发生变化则作业失败（“did not stay read-only”）。
- 执行记录保存脱敏后的调用、退出码、工具调用数；stdout/stderr 受 64 MiB / 单行 4 MiB / 200k 行限额。
- 提示词位于 `src-tauri/src/learn/prompts/`，`PROMPT_VERSION = 1` 写入研究。

## 源码引用

- Wiki Markdown 使用 `[label](loom-src:<path>#L<start>-L<end>)` 引用源码，`[title](loom-wiki:module/<id>)` 链接模块页。
- `learn_read_page` 返回逐条校验结果：`valid` / `missing`（文件不存在）/ `range`（行越界）/ `outside`（路径逃逸或 symlink）。前端把无效引用渲染为带警告样式的按钮，不静默丢弃。
- Chat 的 Markdown 不接受 `loom-src:` / `loom-wiki:` 链接；Learn、Research、Discuss 按各自契约传入链接处理器后才渲染为操作。
- `learn_read_source` 拒绝路径逃逸与任何 symlink；> 512 KiB 的文件按 2,000 行窗口读取，> 16 MiB 拒绝。引用校验与源码读取自 P6 起在共享的 `src-tauri/src/source_files.rs`（原 `learn/source.rs`），Research / Discuss 同用。

## IPC

所有命令仅限主窗口调用。

| 命令 | 参数 | 返回 |
|---|---|---|
| `learn_list_studies` | — | `{ studies: StudySummary[], storageErrors }` |
| `learn_create_study` | `{ input: { url, gitRef?, agentId, model?, language: "en" \| "zh-CN", depth: "quick" \| "standard" \| "deep" } }` | `StudyDetail`；随即启动 `fetch` |
| `learn_get_study` | `{ studyId }` | `{ study, architecture? }` |
| `learn_read_page` | `{ studyId, pageId }` | `{ pageId, markdown, refs }` |
| `learn_generate` | `{ studyId, stage }`（`fetch` / `inventory` / `architecture` / `module:<id>` / `glossary` / `all`） | `jobId` |
| `learn_cancel_job` | `{ studyId }` | 是否有作业被取消 |
| `learn_read_job_logs` | `{ studyId, jobId, stream: stdout \| stderr \| prompt, offset? }` | `{ text, nextOffset, hasMore }` |
| `learn_list_tree` | `{ studyId, dir? }` | `TreeEntry[]` |
| `learn_read_source` | `{ studyId, path, startLine?, endLine? }` | `{ path, text, firstLine, totalLines, truncated, language }` |
| `learn_delete_study` | `{ studyId }` | — |

Agent 解析：必须已启用、可用，且适配器声明 `modelSelection`（Claude Code / Codex / OpenCode / Grok 内建适配器）；模型名经 `validate_model_name` 校验后由适配器生成 `--model`。UI 不拼接 CLI 参数。

事件 `loom://learn-event`：`{ studyId, jobId?, stage, status, error? }`。`stage = "job"` 表示作业结束，`stage = "study", status = "deleted"` 表示研究已删除。前端收到任何事件后重新读取列表与当前研究。

## 前端

- 入口：左侧功能栏 Learn（⌘2，`src/shell/ActivityRail.tsx`，2026-10-27 P5 起）。Learn 首次访问后保持挂载，显示期间 Chat 容器保持挂载（隐藏），会话、草稿和活动回合不受影响。
- `src/features/learn/`：研究列表、新建表单、阶段与问题列表、作业日志、Wiki 目录（Overview / Architecture / Level 1–3 / Glossary）、架构图（SVG，节点可点击打开模块页）；源码查看器（目录浏览、行号、范围高亮）自 P6 起为共享 `components/SourceViewer`，Research / Discuss 同用。
- 非 Tauri 环境显示“仅桌面可用”。DEV 专用 `?fixture=simple-chat&learnFixture=ready|empty|running|failed|error` 注入合成 API，生产构建不包含。
