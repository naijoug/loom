# Research 契约

状态：2026-10-27 实现（[Agent 调用层与左侧功能栏计划](../plans/2026-10-26/22:42-agent-layer-and-activity-rail.md) P6，按 §8-2 默认决策）。范围见 [需求文档](../requirements.md) 的“Research 范围”，安全边界见 [安全策略](../security-policy.md#research)。命令、事件与模型样本由 `contracts/tauri-contract.json` 两端共同校验。

2026-11-06 扩展状态：普通 Research 保留原问答行为，存储已支持 v1 读取与首次写入备份升级到 v2。目标 Research 的存储、跨进程租约、材料分页和独立 IPC 已接入；App 与 GitHub 后端链路已有验证。网站目标在 R4 使用独立 Chromium profile 和本次 loopback 调试端口，保存页面/脚本证据与不进入文本提示的截图。R5 在 macOS 且 `LOOM_RESEARCH_TARGETS=1` 时打开单次目标界面，默认关闭。R6 的连续模式还要 `LOOM_RESEARCH_ITERATION=1`。E0 的 `LOOM_RESEARCH_EXPERIMENTS` 默认关闭，且没有实验界面。原生三目标真实链路仍按 [多目标计划](../plans/2026-10-28/04:41-research-targets-and-experiments.md) 留到后续验收。

## 存储

研究只写入应用数据目录，不写入用户项目：

```text
<appData>/research/<researchId>/
  research.json       # Research：标题、Agent/模型/适配器、可选项目（规范路径与名称）、语言、提示词版本、问答列表、续聊句柄
  turns/<n>.md        # 第 n 个问题的报告（从 Agent 输出中提取）
  workspace/          # 未选项目时的 cwd；空目录
  jobs/<jobId>/{job.json, prompt.md, stdout.log, stderr.log}
```

- `research.json` 原子写入；新记录 `schemaVersion = 2`。v1 记录读取时保持磁盘不变，首次写入前保存并校验 `research.v1.backup.json`。未来 schema 拒绝读取与写回。无活动跨进程租约时，读取返回 `interrupted` 投影，不在读路径写入恢复状态。
- 同一研究同一时刻最多一个 writer（OS 文件锁，普通 Job 与目标 run 共用）。进程内登记表只是本机投影；删除取得同一租约。停止请求绑定 run/job 与本次 lease token，旧 attempt 的迟到取消不会误停重试。
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
- 非 Tauri 环境显示“仅桌面可用”。DEV 专用 `?fixture=simple-chat&researchFixture=ready|empty|running|failed|error|targets` 注入合成 API，生产构建不包含。`targets` 只模拟单次目标界面，不启动浏览器、Git 或模型。


## 目标 Research（单次界面已接入，连续调研与实验仍关闭）

- `workflow = legacy | target` 不可原地切换；旧 ask/retry/cancel、turn、日志及源码入口对 target 返回 `workflow_mismatch`。普通创建输入拒绝未知的 target 字段。界面在读到 `workflow = target` 后改用目标命令，不调用上述旧执行入口。
- `research_target_capabilities` 只报告开关：`singleShot` 在 macOS 且 `LOOM_RESEARCH_TARGETS=1` 时为真；`iteration` 与 `experiments` 固定为假。关闭时仍可阅读已保存的目标报告、证据和快照，但不能预检、开始、追问、重试或恢复。停止活动 run 不依赖该开关。
- 目标命令：`research_target_capabilities/preflight/start_run/ask_run/retry_run/resume_run/cancel_run/get_run/list_runs/read_report/list_report_tree/read_report_source/list_evidence/read_evidence/export`。全部检查主窗口。新运行须开启开发开关；已保存资料读取不依赖开关。`research_export` 取得同一 writer lease，在应用受管目录导出 Markdown 和脱敏材料；显式 includeRaw 才包含经 hash 校验的原始 Evidence，并返回分享前审查提示。
- 单次表单默认提交 `mode = single`。只有能力接口同时报告 `singleShot` 和 `iteration`，且 `experiments` 为假时，表单才多一个连续调研选项，并提交 `mode = iterative`。网站在预检前和预检结果里都写明会加载页面并运行脚本。证据区只显示脱敏文本；截图不作为图片渲染。源码查看器只读取当前 reportId 的快照。
- 实验开关 `LOOM_RESEARCH_EXPERIMENTS` 默认关闭，能力字段 `experiments` 只在 macOS 且该变量为 `1` 时为真。E0 不注册实验命令，也不提供界面入口。实验代码在 `src-tauri/src/experiments/`，不引用 Research。快照只复制显式相对路径上的普通文本；`package.json`、`node_modules`、符号链接和含 NUL 的文件被拒绝。脏工作区返回 `workspace_dirty`，不清理。`node-cpu-v1` 的批次选择、基线 spread 和 keep/discard/inconclusive 判定是纯函数；基线测量在受管副本里跑 Node，cwd 不是原项目。
- 连续循环只接受 `next_action`：`inspect_app`、`analyze_js`、`capture_page`、`inspect_web_scripts`、`read_evidence`、`finish`。动作必须匹配已冻结目标，证据和假设里的 ID 必须已经发布。带 shell、任意路径、另一条 URL 或预算字段的 JSON 无效。错误目标动作停在 `unsupported`，不执行。重复读取同一证据不计新进展；连续两轮没有新覆盖后停在 `no_progress`。轮数不能超过预检预算，超出后写 `budget_exhausted` 部分报告。假设裁决只有 `support`、`refute`、`unknown`。假设和进展写在 `runs/<runId>/iteration.json`，重试沿用同一 run 的轮数和这份记录。追问仍强制单次且采集额度为 0。
- 预检冻结规范路径、Agent 配置、工具身份、预算和 5 分钟期限；开始只接受 preflightId/requestId。重复 requestId 不重启进程；失败请求也保存，换请求体报冲突。已消费的预检不能通过新 requestId 再启动。
- target 的 `runs/<runId>/events.jsonl` 为权威记录，`state.json` 为可重建投影。读者忽略半条尾记录；恢复持有 writer 后修复尾部、保守扣除未知调用预留。retry/resume 不清零用量；模型补读上限同样持久化。
- 报告 manifest 绑定 run 与已发布 snapshot/evidence 集合，report ID 包含 manifest 摘要；快照按清单及源码内容 hash 验证，缺失或篡改不会回退 workspace。只读追问复制 parentReport 的固定材料 ID，采集预算为 0，不访问当前目标。
- 模型仅可返回严格的 `answer` / `need_material` JSON。每交换最多 4 个读取、源码窗口最多 200 行、序列化材料最多 48,000 字符；读取范围超过文件末尾时与共享源码查看器一样裁到 EOF，起点越界仍拒绝。每报告 3 次补读、1 次纠错、最多 5 次模型调用。
- `loom://research-event` 的 target 事件包含 researchId/runId、status/phase、progress、stopReason/error；读取磁盘状态仍是恢复依据。旧事件形状保持兼容。
- 当前 App 链路使用固定 REA 6.8.0 私有 MCP、固定工具枚举和 schema 校验，先静态识别，再 JS 分析、保存 Evidence 与源码快照，最后通过 evidenceOnly CLI 生成报告。原始 Evidence 与模型视图分开 hash；默认材料读取只提供派生视图。App 不执行，原生引擎只提供诊断。
- GitHub 目标不调用 REA。预检只接受公开 `https://github.com/<owner>/<repo>` 和受校验 ref，并清空 tools。采集经 `repository_sources` 浅克隆并固定 commit，再把文本路径发布为不可变快照；符号链接、二进制、vendored 目录和超过 1 MiB 的文件不进入快照。模型用已有 `tree` / `source` 材料分页读取，`nextCursor` 表示未读完的路径。`research_list_report_tree` / `research_read_report_source` 只接受 researchId 与 reportId，从该报告的 `defaultSourceSnapshotId` 解析，不接受调用方传入的根目录或 snapshotId。
- 网站目标需要 REA 6.8.0 和本机 Chrome/Chromium。预检规范化 `http(s)` URL，拒绝 `file:`、`javascript:`、凭据和私网字面地址，并记录将要启动的浏览器路径。采集使用新的 user-data-dir 和自动分配的 loopback 调试端口，确认监听套接字属于本次进程树后才把该端口交给 `list_browser_targets` / `inspect_web_page`。根页面落到其他 origin 时返回 `cross_origin_redirect` 或 `cross_origin_navigation`。截图 PNG 只进原始证据。模型视图去掉 cookie、Authorization 和 URL 查询串。
- 尚待完成的约束与验证见 [R1/R2 记录](../dogfood/2026-10-28-research-targets-r1-backend.md)、[R3 记录](../dogfood/2026-11-06-research-targets-r3.md) 和 [R4 记录](../dogfood/2026-11-06-research-targets-r4.md)。不能将默认关闭的后端切片描述为完整目标研究产品交付。


### R1 持久化与恢复补充

- `runs/<runId>/disk.json` 保存每次文件/目录写入的容量预留、已确认字节与发布位置；metadata/journal 和最终报告保留独立额度。usage 区分 `diskBytes` 与 `diskReservedBytes`。工具 cwd 在运行中与退出后检测额度；这不是 OS 文件系统配额。
- 完成事件丢失时，受锁恢复验证已 rename 的 artifact，再关联 snapshot/evidence/report；未知写入保留预留，不通过重启清空预算。首次研究与 initial run 一起原子发布，pending 请求回放不重新执行。
- 每次补读后的完整有界 context 另存不可变文件，由 run 的 `materialContext` 引用并验证 scope/hash。manifest 始终保留；旧页被容量淘汰时明示，证据目录支持 `evidence_index`，材料分页返回 `nextCursor`。
- `research_get/list` 的 `activeOwner?` 为当前租约投影，不作为持久化活动事实。空闲锁重新取得时清理旧 owner 文件；读取不写入恢复状态。
- 版本检查在统一 supervisor 下运行；start/continue 先登记 request/run，再由该运行验证 CLI。新模式只在 macOS 开放开发开关，其他平台明确 unsupported。


### 静态 App 采集与测试边界

- 目录、独立 `.js/.mjs/.cjs`、ASAR 的合法源码成员统一捕获到受管目录，记录原始路径/容器 SHA/文件 SHA 与 omissions；REA 分析该捕获而不执行原 App。ASAR companion 只允许声明成员路径，链接、越界、完整性错误均拒绝。
- 已完成的源码捕获在 REA 调用前发布，因此取消或 provider_timeout 也可保留源码。provider tool error 会在剩余额度内读取 bundle，宿主部分报告不冒充完整模型结论。
- `LOOM_PROFILE_DIR` 同时隔离 Research 的验收数据；普通安装的原存储路径不变。读取旧报告不创建 workspace。
- `ipc_tests.rs` 使用 Tauri MockRuntime 的真实 invoke handler，覆盖序列化、窗口、幂等与控制链；这不是原生 WebView/视觉证据。测试专用上下文仅存在于 cfg(test)，生产构建不包含。
