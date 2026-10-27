# Research 契约

状态：2026-10-27 实现（[Agent 调用层与左侧功能栏计划](../plans/2026-10-26/22:42-agent-layer-and-activity-rail.md) P6，按 §8-2 默认决策）。范围见 [需求文档](../requirements.md) 的“Research 范围”，安全边界见 [安全策略](../security-policy.md#research)。命令、事件与模型样本由 `contracts/tauri-contract.json` 两端共同校验。

## 存储

研究只写入应用数据目录，不写入用户项目：

```text
<appData>/research/<researchId>/
  research.json       # Research：标题、Agent/模型/适配器、可选项目（规范路径与名称）、语言、提示词版本、问答列表、续聊句柄
  turns/<n>.md        # 第 n 个问题的报告（从 Agent 输出中提取）
  workspace/          # 未选项目时的 cwd；空目录
  jobs/<jobId>/{job.json, prompt.md, stdout.log, stderr.log}
```

- `research.json` 原子写入（进程内锁）；`schemaVersion = 1`。读取时把上个进程遗留的 `running` 问题标为 `interrupted`，不自动重启。
- 同一研究同一时刻最多一个作业（进程内登记表）；删除研究前必须没有运行中的作业。
- `Turn`：`index`、`question`、`status`（`running` / `done` / `failed` / `cancelled` / `interrupted`）、`jobId`、起止时间、`error?`、`warnings[]`、`refs?`（有效 / 无效引用数）、`context?`（`native` 原生续聊 / `replay` 回放历史；首问为空）。
- 列表摘要的 `status` 取最后一个问题：运行中 `running`，完成 `ready`，否则 `failed`。

## 作业

- 每个问题一次 `agent::run`：`prepare_invocation` 的 `stage = Planning`、`chat_permission_mode = explore`、可选 `model`；进程归属 `ProcessOwner::Feature { feature: "research", scope_id: researchId, run_id: jobId }`；超时 20 min，stdout/stderr 受 64 MiB / 单行 4 MiB / 200k 行限额。
- cwd：选了项目时为项目根目录（开始与追问前都重新确认它仍在 Loom 的项目列表中），否则为 `workspace/`。
- 提示词位于 `src-tauri/src/research/prompts/`（`PROMPT_VERSION = 1`）：只读规则、引用格式、报告结构（结论 / 依据 / 引用 / 未解决问题），报告写在 `<<<LOOM_REPORT` 与 `LOOM_REPORT>>>` 之间；缺标记时取整段输出。
- 追问：上一个问题完成、存在续聊句柄且 Agent 命令未变时，用原生续聊（`context = native`），提示词不含历史；否则回放此前已完成的问答（`context = replay`），预算 24,000 字符，保留最新的问答，超出部分标注省略，单个过长报告截断。原生续聊失败（非用户停止）时自动改用回放重跑一次，并记警告。
- 运行后：
  - 选了项目时比对运行前后的 `git status --porcelain` 与 HEAD，变化记警告（不判失败）；
  - 空报告判失败；
  - 校验报告中的 `loom-src:` 引用，无效引用记警告；选了项目但 Agent 没有任何工具调用时记警告（引用可能是猜的）。
- 停止映射为 `cancelled`（无错误信息）；应用退出时为 `interrupted`；其它失败为 `failed`，错误为 `AgentError` 的“摘要 + 空行 + 详情”格式。

## 源码引用

- 报告用 `[label](loom-src:<项目相对路径>#L<start>-L<end>)` 引用项目文件，外部来源用 `http(s)` 链接。
- `research_read_turn` 返回逐条校验结果（`valid` / `missing` / `range` / `outside`），读取时按当前文件重新校验；项目已移出 Loom 列表时不读取项目、`refs` 为空（引用显示为未校验）。前端把无效引用渲染为带警告样式的按钮。
- 引用校验与源码读取共用 `source_files.rs`（Learn 同用）：拒绝路径逃逸与任何 symlink；> 512 KiB 的文件按 2,000 行窗口读取，> 16 MiB 拒绝。项目不再登记时，源码读取被拒绝。

## IPC

所有命令仅限主窗口调用。

| 命令 | 参数 | 返回 |
|---|---|---|
| `research_list` | — | `{ items: ResearchSummary[], storageErrors }` |
| `research_create` | `{ input: { question, agentId, model?, projectPath?, language: "en" \| "zh-CN" } }` | `Research`；随即开始第 1 问 |
| `research_get` | `{ researchId }` | `Research` |
| `research_read_turn` | `{ researchId, turn }` | `{ index, markdown, refs }` |
| `research_ask` | `{ researchId, question }` | `Research`；已有运行中的问题时拒绝 |
| `research_retry` | `{ researchId }` | `Research`；只重跑最后一个 `failed` / `cancelled` / `interrupted` 问题 |
| `research_cancel` | `{ researchId }` | 是否有作业被取消 |
| `research_read_job_logs` | `{ researchId, jobId, stream: stdout \| stderr \| prompt, offset? }` | `{ text, nextOffset, hasMore }` |
| `research_list_tree` | `{ researchId, dir? }` | `TreeEntry[]` |
| `research_read_source` | `{ researchId, path, startLine?, endLine? }` | `{ path, text, firstLine, totalLines, truncated, language }` |
| `research_delete` | `{ researchId }` | — |

- 问题必填，≤ 8,000 字符。项目必须已在 Loom 打开过（`list_recent_projects` 中），否则返回“Open this project in Loom before researching it”。
- Agent 必须已启用、可用、适配器声明 `modelSelection`（Claude Code / Codex / OpenCode / Grok 内建适配器），并通过 `check_startable`；模型名经 `validate_model_name` 校验后由适配器生成 `--model`。UI 不拼接 CLI 参数。

事件 `loom://research-event`：`{ researchId, jobId?, turn?, status, error? }`。`status` 为问题状态，或 `deleted` 表示研究已删除。前端收到任何事件后重新读取列表与当前研究。

## 前端

- 入口：左侧功能栏 Research（⌘3）；首次访问后保持挂载，Chat 保持挂载（隐藏）。
- `src/features/research/`：研究列表、新建表单（共享 `AgentPicker`，模型输入随诊断能力显示）、问答列表（上下文标签、警告、错误摘要、报告）、追问框、停止 / 重试 / 删除、作业日志；源码查看器为共享 `components/SourceViewer`。
- 非 Tauri 环境显示“仅桌面可用”。DEV 专用 `?fixture=simple-chat&researchFixture=ready|empty|running|failed|error` 注入合成 API，生产构建不包含。
