# Agent 调用层与左侧功能栏重构计划

- 状态：实施中；P0–P5 已完成，下一步 P6（进度见第 0 节）
- 基线：`main` @ `d5b8c4b` 之上的未提交 Learn 工作区（含 2026-10-26 Agent 启动前检查与错误详情折叠）
- 触发：用户要求参考 Codex 应用的布局，把 Chat 作为默认主页，Learn、Research、Discuss 作为并列功能；调用本机各种 Agent 做成独立的模块和层，上层模块只管调用，UI 只是交互入口
- 范围：
  - Rust 新增独立的 `agent` 层，Chat 与 Learn 改为只经它调用 Agent；
  - 前端新增左侧功能栏（Activity Rail）和功能注册表；
  - 新增 Research、Discuss 两个 MVP。
  - 不改 Chat v2 存储格式与现有 IPC 命令名，不启用冻结的 Workbench / SDK worker，不修改或删除旧 Task 数据。

## 0. 实施进度

- **P0 完成（2026-10-26）**：
  - Learn 工作已提交：`849d223 feat(learn): study GitHub repositories with a local agent`；本计划：`c616117`。
  - 真实 Codex Chat 基线通过：`chat::real_agent_tests::codex_three_turns_resume_after_repository_reopen`、`codex_stop_then_send`（78.5 s；全局 Codex 0.164.0 缺平台包，本次用本机缓存的 Codex 0.153.0 二进制前置 PATH 运行，未改全局安装）。Learn 真实基线沿用验收记录中的 hexyl 研究。
- **P1 完成（2026-10-26，`7049270`）**：
  - `src-tauri/src/chat/characterization_tests.rs`：
    - 用 `/bin/cat` 经真实 runner 回放 Claude 2.1.304、Codex 0.163.0、OpenCode 1.19.3、Grok 1.0.62 的录制流，事件序列、日志行数与 FNV-1a 摘要、`ChatTurnOutcome` 对比 `*.golden.json`（`LOOM_UPDATE_GOLDEN=1` 重新生成）；
    - 非零退出保留部分文本与最后 2,000 字符 stderr；静默成功与启动失败可区分；stdin 提示缺输入在启动前拒绝；非 Chat 所有者（Learn）登记的进程类型与清理。
  - `learn/jobs.rs`：`job_events_record_and_logs_are_stable` 固定伪 CLI 作业的事件、`JobRecord`、调用参数与日志文件内容。
  - 超时、取消、输出限额、进程组清理与 shutdown 已有 `chat::runtime` / `chat::shutdown` / `process_supervisor` 测试覆盖，未重复添加。
  - 与计划的偏差：测试数据复用已有的 `src-tauri/tests/fixtures/cli-streams/`，未新建 `agent-streams/`；runner 特征测试单独成文件，而非放进 `chat/mod.rs` tests，便于 P3 整体迁到 `agent/`。
  - 验证：`pnpm check` 通过（前端 130、Rust 296 通过 / 27 忽略）。首轮 `pnpm check` 中 `chat::shutdown::tests::shutdown_cases_in_isolated_processes` 失败一次（`killpg` 返回 EPERM，回合记为 Failed 而非 Cancelled），单独连跑 5 次与复跑全量均通过，判断为负载下的偶发问题，P3 迁移进程组清理时一并处理。

- **P2 完成（2026-10-27）**：
  - `agents.rs` + `agents/config.rs` → `agent/registry.rs`；`agent_adapter.rs` 拆为 `agent/capabilities.rs`（适配器 ID、能力类型与 `capabilities()`）和 `agent/invocation.rs`（`AgentStage`、参数准备、模型校验、续聊解析）；`agent_diagnostics.rs` → `agent/diagnostics.rs`；`agents/stream.rs` → `agent/stream.rs`。代码内容只改了 import 路径，函数体未动。
  - `agent/mod.rs` 是功能模块的唯一入口（`load_agents`、`capabilities`、`prepare_invocation`、`check_startable`、`redact_*` 等再导出）；只有 `lib.rs` 的 Tauri 命令注册直接引用 `agent::registry` / `agent::diagnostics`（`generate_handler!` 需要命令所在模块路径），契约测试中的 `AgentDiagnostic` 也走 `agent::diagnostics`。
  - Tauri 命令名与 `contracts/tauri-contract.json` 未变；`pnpm check` 通过（Rust 296 通过 / 27 忽略，与 P1 相同；P1 特征测试与契约测试通过）。纯移动，未重跑真实 Agent 测试，P3 改 runner 后重跑。
  - 与计划的偏差：`stream.rs` 暂只含原 `agents/stream.rs` 的日志脱敏，`parse_chat_stream_line` 随 P3 迁入；`registry.rs` 合并了 `config.rs`（518 行），原 `pub(super)` 辅助函数保留写法，合并后可见范围变为 `agent` 模块，行为不变。

- **P3 完成（2026-10-27）**：
  - `agent/runner.rs`：从 `chat/runtime.rs` 抽出通用 runner `agent::run(RunSpec, cancelled, emit, observe)`，含启动登记、stdin、超时/取消/输出限额、进程组清理、停止原因与 `stop_outcome`；默认超时常量改名 `DEFAULT_TIMEOUT_MS`。`RunSpec` 由调用方给出 `ProcessOwner` / `ProcessKind` 和可选的行补充回调 `LineEnricher`。
  - `agent/event.rs`：`AgentEvent`（`Text` / `Part` / `Done`）、`AgentOutcome`，以及从 `chat/models.rs` 移入的 `MessagePart`（Chat 侧以 `ChatMessagePart` 别名再导出，序列化格式不变）、`DiffStats`、`ToolMetadata`。
  - `agent/stream.rs`：迁入 `parse_chat_stream_line` 及其辅助函数（改名 `parse_line`）；`chat/tools.rs` → `agent/tools.rs`。
  - `chat/runtime.rs` 只剩 Chat 薄封装：Chat 进程所有者、可写 Codex 的文件变更基线（作为 `LineEnricher` 传入）、`AgentEvent` → `ChatStreamEvent` 映射；`ChatTurnOutcome` 是 `AgentOutcome` 的别名。持久化字符串（`chat_abort`、`chat_timeout`、`app_shutdown` 等）不变。
  - `learn/jobs.rs` 改用 `agent::run`，不再引用 `crate::chat`；`ProcessMetadata::owned` 支持任意所有者。
  - `scripts/check-layers.mjs` 接入 `scripts/check.sh`：`agent/` 不引用功能模块；Learn 与 Chat 互不引用；chat/learn/chat_context 的生产代码不直接起进程（白名单 `learn/fetch.rs` 的 git）。已用临时违规改动验证脚本会失败。
  - 验证：
    - `pnpm check` 通过（Rust 296 通过 / 27 忽略，P1 golden 快照未变）；
    - 真实 Codex Chat：`codex_three_turns_resume_after_repository_reopen`、`codex_stop_then_send` 通过（63.4 s，同样用缓存的 Codex 0.153.0）；
    - 真实 Codex Learn：`learn::real_tests::codex_studies_hexyl` 通过（951 s），全部阶段 Done，11 页引用全部有效（invalid 0），源码覆盖 7/7；
    - 未跑 `smoke:visual`：本阶段未改前端代码，IPC 事件格式由特征快照与契约测试固定。
  - 与计划的偏差：
    - 未新增 `ProcessOwner::Feature`：当前没有调用方，`-D warnings` 下会成为死代码；随 P6 Research 一起加。
    - `RunSpec` 接收已准备好的 `PreparedAgentInvocation`，而不是 Agent 配置 + 权限 + 提示词；把参数准备并入 run 属于 P4 统一启动前检查。
    - `AgentEvent` 只有 `Text` / `Part` / `Done`；启动与日志行仍走单独的 `Observation` 回调（Chat 的进程登记与日志写入依赖它）。
    - 新增计划未列出的 `agent/event.rs`、`agent/tools.rs`。
    - 空输出占位文案“（Agent 没有返回可见文本）”与停止/超时中文提示仍在 runner 内，统一错误格式在 P4 处理。
    - runner 的超时/取消/限额测试仍在 `chat/runtime.rs`，经 Chat 薄封装调用 `agent::run`；停止原因测试已移到 `agent/runner.rs`。
    - S1 只部分落实：chat 的停止/关停（`chat/mod.rs`、`service.rs`、`shutdown.rs`、`repository.rs`）与 `learn/jobs.rs` 的 `is_shutting_down` 仍直接用 `process_supervisor`，脚本未检查这一项；P4/P8 由 `agent` 暴露停止与关停接口后收紧。
    - S2 前端规则未接入：`settings`→`chat`/`app-update`/`skills`、`learn`→`chat`、`skills`→`chat`、`chat`→`app-update` 目前存在跨 feature 导入（Markdown、AgentPicker 等），随 P5 抽共享组件后再加。
    - `session_capture.rs` 仍在 crate 顶层，由 `agent/` 引用。

- **P4 完成（2026-10-27）**：
  - `agent/error.rs`：`AgentError { summary, details }`，`Display` / `String` 输出“摘要 + 空行 + 详情”，空详情不输出空行。`check_startable` 返回 `AgentError`。
  - `check_startable` 加缓存：通过的结果按 Agent id 缓存（键为解析路径 + 修改时间），失败不缓存；`save_agents`、`diagnose_agents`（Settings 重新检测）、Chat 回合失败、Learn 作业失败时清除。自定义 CLI 只检查命令可解析，不执行 `--version`；Dummy 直接通过。
  - `chat_send` 在接受回合前调用 `check_startable`，安装损坏时直接返回错误，不创建回合。
  - `diagnose_agent`：能启动但 `--version` 非零退出标为 `broken`；前端 `AgentDiagnostic.status` 增加 `broken`，`AgentPicker` 新增 `isAgentUnusable` / `agentStatusKey`，Settings、Agent 选择器、Composer 发送判断、ChatPage 默认 Agent 与 `/agent` 切换统一使用，显示“安装已损坏 / Broken install”。
  - runner 非零退出的错误改为 `agent exited with code <n>` + 空行 + stderr 尾部 2,000 字符；`termination_reason`、错误码 `agent_exit_failed` 不变。
  - `ErrorText` 移到 `src/components/common/ErrorText.tsx`（样式 `ErrorText.css`，i18n 键 `learnErrorDetails` → `errorDetails`），兼容旧 `; stderr:` 格式；Chat 的错误片段、回退错误、回合详情的 `errorSummary` 与发送错误改用它，Learn 照常使用。
  - 契约：`docs/agent-adapter.md` 新增“启动前检查与错误格式”，`docs/architecture/chat-contracts.md` 记录 `chat_send` 的检查与新的退出错误格式。
  - 验证：
    - `pnpm check` 通过（前端 132，新增发送遇坏 CLI 时摘要与折叠详情、损坏 Agent 不可选两项；Rust 298 通过 / 27 忽略，新增 `AgentError` 格式、坏/好 CLI 与 `broken` 诊断、缓存命中 / 文件变化 / 清除三项）。
    - 首轮 `pnpm check` 中 `skills::operations::tests::ssh_configuration_is_preserved_and_default_is_only_used_when_unconfigured` 失败一次（`killpg` 返回 EPERM），单独连跑 3 次与复跑全量均通过；与 P1 记录的偶发问题同源（macOS 上进程组只剩未回收的僵尸进程时 `kill` 返回 EPERM 而非 ESRCH），P3 未处理，另立任务修复。
    - 真实二进制：临时测试对本机 Codex 调用 `check_startable` / `diagnose_agent`，全局 `~/.npm-global/bin/codex`（0.164.0）与缓存的 0.153.0 均为 Ok / `ready`；P0 时全局安装损坏，现已可用，因此损坏场景只由复现同一 stderr 的脚本单测覆盖。临时测试已删除。
    - 未跑真实 Chat / Learn 回合与 `smoke:visual`：本阶段只改了失败路径的文案和发送前检查，成功路径未变。
  - 与计划的偏差：
    - 未实现 `AgentErrorKind`：IPC 仍传字符串，当前没有按类型分支的调用方，`-D warnings` 下未使用的变体会报错；等 Research / Discuss 需要按类型展示时再加。
    - Skills 本地翻译未改用 `agent::run`，在 `docs/agent-adapter.md` 记为例外：它需要原始 stdout（Claude 的单个 JSON 结果），而 runner 解析前逐行脱敏，可能改写译文中形似密钥的示例；它也不走 Agent 配置。`check-layers.mjs` 暂不扫描 `skills/`。
    - Learn 新建研究表单仍只按 `available` 过滤 Agent，未读取诊断；损坏的 Agent 在开始时被 `check_startable` 拦下。
    - 新 Chat 回合的错误文本格式变化（旧：`agent exited with status exit status: N; stderr: …`），旧会话由 `ErrorText` 兼容读取。

- **P5 完成（2026-10-27）**：
  - 布局稿 `designs/activity-rail/README.md`（按“默认决策执行”的指示直接实施，未单独确认）。
  - `src/shell/`：
    - `features.ts` 功能注册表（Chat、Learn，懒加载），`AppView = FeatureId | "settings"`；
    - `ActivityRail.tsx` 左侧 52px 功能栏：注册表顺序在上、Settings 置底，当前项 `aria-current="page"`，提示含快捷键，按钮带 `data-view`；macOS 顶部 38px 拖拽区；
    - `AppShell.tsx`（由 `features/chat/ChatShell.tsx` 改写）：⌘1…4 / ⌘,（其它平台 Ctrl）切换，输入法组合中不响应；Chat 常驻挂载，其它功能首次访问后保持挂载，Settings 仅打开时挂载。
  - 共享层：
    - `features/chat/MarkdownView.tsx` → `components/Markdown.tsx`，代码块渲染改为调用方传入的 `renderCode`；Chat 的 OpenUI 内联卡片包装为 `features/chat/ChatMarkdown.tsx`；
    - `AgentPicker`、`externalLink` → `components/`；`AgentPicker` 新增 `placement`、`note`、可选 `model`（仅诊断报告 `modelSelection` 时显示模型输入）；
    - `features/app-update/` → `components/app-update/`，`inlineCardsSetting` → `hooks/`，删除 `components/Settings/` 转发壳；
    - `api/agents.ts` 封装 Agent 命令；Agent 诊断进入全局状态 `agentDiagnostics`（`agents/diagnosed` 按 id 合并），Chat、Learn、Settings 共用；
    - 共用样式从 `chat.css` 拆到 `styles/workspace.css`、`components/Markdown.css`、`components/AgentPicker.css`，功能之间不再互相引入 CSS。
  - 功能调整：Chat 侧栏底部移除 Learn / Settings，只留“归档”；Learn、Settings 移除返回按钮；Learn 新建表单改用共享 `AgentPicker`，同时排除诊断为不可用（含 `broken`）的 Agent，模型只在显示时提交（P4 偏差“Learn 只按 `available` 过滤”已解决）。
  - `check-layers.mjs` 规则 4：`features/<x>` 不引用其它功能的代码或 CSS，`components` / `hooks` / `api` 不引用 `features/`；已用临时违规导入验证脚本会失败。
  - 验证：
    - `pnpm check` 通过（前端 136，新增注册表 / 快捷键 / 切换保留 Chat 草稿、模型输入随能力显示共 4 项；Rust 298 通过 / 27 忽略）；
    - `pnpm smoke:visual` 通过：新增功能栏矩阵（zh/en × 深浅 × 700px：52px、当前项、⌘/Ctrl+2 与 ⌘/Ctrl+, 切换、无横向溢出）与 S6（1280px 打开会话写草稿，切到 Learn 再回来会话与草稿保持），截图在 `designs/exports/activity-rail/`（不入库）；Learn / Skills / 内联卡片 smoke 改由功能栏进入后通过。
    - 未做：桌面人工检查“切换功能时进行中的 Chat 回合不中断”。Chat 常驻挂载由单测与 S6 覆盖，但真实流式回合未在桌面上切换验证。
  - 与计划的偏差：
    - `AgentPicker` 未加权限参数：Chat 的权限仍由 `ModePicker` 负责，Learn 固定只读，没有调用方需要。
    - 注册表未加 `enabled` 字段：当前功能都启用，P6/P7 需要隐藏时再加。
    - Settings 不保持挂载：它没有需要保留的界面状态，关闭即卸载可减少常驻组件。
    - 规则 4 白名单 `settings→skills`：Skills 是设置的一个分区。
    - 共享组件的类名沿用 `chat-` 前缀，避免大面积改样式；新组件用 `activity-rail-*`、`app-shell-*`、`agent-model-input`。
    - 计划未列出的移动：`app-update` 放 `components/app-update/`，`inlineCardsSetting` 放 `hooks/`。

## 1. 目标

1. **Agent 层独立**：凡是调用本机 Agent CLI 的工作，都只通过 `src-tauri/src/agent/` 进行，包括：
   - 发现与配置、能力声明、启动前检查；
   - 参数生成、进程启动与监管、超时与取消；
   - 输出限额、流式事件解析、续聊句柄、脱敏执行记录。
2. **功能模块互不依赖**：Chat、Learn、Research、Discuss 是 Agent 层之上的并列模块，各自只管自己的数据、流程和提示词。功能模块之间不互相引用；共享的 UI 组件放到公共目录。
3. **UI 只是入口**：
   - 窗口最左侧是图标栏：Chat（默认主页）、Learn、Research、Discuss，底部是 Settings。
   - 每个功能自带第二栏（如 Chat 的项目会话树、Learn 的研究列表）和主区域。
4. **新增两个功能（MVP）**：
   - Research：针对一个问题，由单个 Agent 只读调研，产出带引用的报告，可以追问。
   - Discuss：2–4 个 Agent 先独立作答，再互评修订，最后汇总出共识、分歧、风险和建议。
5. **行为不变**：重构阶段 Chat 和 Learn 的用户可见行为、事件序列和存储内容不变。

## 2. 非目标

- 不做逐工具审批、远程 Agent、云端执行，不扩展 MCP / Browser / Pages / DAG（与[需求文档](../../requirements.md)当前范围一致）。
- 不恢复 Task / Board / Planning。Discuss 不推进实施、Review 或修复循环，只产出讨论结论。
- 不把 Chat 改成"每功能一个 Chat"；Research / Discuss 的追问在各自的数据里，不写入 Chat 会话。
- 不新增依赖（图标沿用 `lucide-react`）。
- 不改变 Agent 配置文件格式和 Settings → Agents 的使用方式。

## 3. 成功标准

| # | 标准 | 验证方式 |
|---|---|---|
| S1 | 在 Rust 中，只有 `agent/` 启动 Agent CLI 进程。`chat/`、`learn/`、`research/`、`discuss/` 不引用 `process_supervisor`、`tokio::process` 或 `agent::adapter` 的内部符号，只用 `agent::` 的公开接口 | `scripts/check-layers.mjs` 扫描 `use` 与路径，接入 `pnpm check` |
| S2 | Rust 功能模块之间不互相引用（Learn 不再 `use crate::chat::runtime`）。前端 `src/features/<x>/` 不 import 其它 `features/<y>/` | 同上，前端规则同样由脚本检查 |
| S3 | Chat 行为不变：现有 Rust / 前端测试、`smoke:visual` 通过；Codex 真实 Chat 回合（流式、停止、续聊）的事件序列与重构前的特征快照一致 | P1 补齐的特征测试、`real_agent_tests` 的 Codex 用例 |
| S4 | Learn 行为不变：`learn::` 单测通过；Codex 真实研究 hexyl 仍全部阶段 Done、引用全部有效 | `learn::real_tests::codex_studies_hexyl` |
| S5 | 任一功能遇到坏掉的 CLI 安装，都在启动前给出一行摘要，原始输出放在可展开详情里；Chat 发送前也会检查（带缓存） | 单测：伪造坏 CLI 与好 CLI；smoke 覆盖 |
| S6 | 应用启动进入 Chat；图标栏切换到 Learn / Research / Discuss / Settings 再切回，Chat 的会话、草稿和进行中的回合都保持；窄窗口（< 760px）不出现横向滚动 | `smoke:visual` 新增功能栏矩阵（中英 × 深浅 × 700px） |
| S7 | Research：选 Agent、模型和可选项目，提问后得到带源码 / 链接引用的报告；能追问、停止、重试；重启后可读 | 单测 + 合成 fixture + Codex 真实用例 |
| S8 | Discuss：2–4 个 Agent 完成 独立作答 → 互评修订 → 汇总；任一参与者失败时，其余结果保留并明确标出；开始前显示预计调用次数 | 单测（伪 CLI）+ 合成 fixture + 真实用例（至少 Codex + 一个其它 CLI，或标 blocked） |

## 4. 当前状态（2026-10-26 核实）

### 4.1 Rust：Agent 调用分散在 7 处

| 职责 | 现在的位置 | 问题 |
|---|---|---|
| Agent 配置、发现、可用性 | `agents.rs`、`agents/config.rs`、`agents/stream.rs` | 与参数生成、执行分开放 |
| 能力与参数生成 | `agent_adapter.rs`（1068 行）：`capabilities`、`prepare_invocation`、`validate_model_name`、`AgentStage` | `AgentStage` 沿用旧 Task 阶段名（Planning 等），Chat / Learn 都要自己把权限映射过去 |
| 版本探测与启动前检查 | `agent_diagnostics.rs`：`diagnose_agent`、`check_startable` | `diagnose_agent` 把 `--version` 非零退出判为 `ready`；`check_startable` 目前只有 Learn 使用 |
| 执行：启动、stdin、限额、超时、取消、进程组 | `chat/runtime.rs`（1060 行）：`Limits`、`Request`、`Observation`、`run_owned` | 放在 Chat 模块里；`Request` 带有 `project_key` / `session_id` / `turn_id` 等 Chat 字段 |
| 流解析 | `chat/mod.rs:145` `parse_chat_stream_line`，结果是 `ChatTurnOutcome`（`chat/mod.rs:841`）与 `ChatStreamEvent`（`chat/models.rs:322`） | 通用的 CLI 输出解析挂在 Chat 的类型上 |
| Chat 上下文与续聊 | `chat_context.rs`（调用 `prepare_invocation`、`chat::resume`、`chat::inline_cards`） | 合理属于 Chat，但同时承担了 Agent 层的参数组装 |
| 进程监管 | `process_supervisor.rs`：`ProcessOwner { Unscoped, Skills, Task, Chat, Learn }`、`ProcessKind` | 每新增一个功能就要加一个枚举值 |

调用方：
- `learn/jobs.rs:11-17` 直接 `use crate::chat::{runtime::...}` 来运行 Agent，Learn 依赖了 Chat。
- `skills/translation/local.rs:231-242` 自己 `Command::new` 启动本地 CLI 做翻译，绕开了 adapter 和 runtime。
- `workbench/` 不在 `lib.rs` 的模块声明里，属于冻结源码、不参与编译；本计划不碰它，移动文件也不需要为它保留兼容路径。

### 4.2 前端

- `src/state/model.ts:2` 中 `AppView = "chat" | "learn" | "settings"`。`App.tsx` 用 `currentView` 切换视图：Chat 保持挂载，Learn 包在 `ChatShell` 里。
- 入口按钮在 Chat 侧边栏底部：`ProjectTree.tsx:74` 的 Learn 按钮，Settings 也在那里。
- 跨功能引用：
  - Learn → `chat/MarkdownView`、`chat/externalLink`；
  - Skills → `chat/MarkdownView`；
  - Settings → `chat/AgentPicker`（`agentHealth`）、`chat/openui/inlineCardsSetting`；
  - App → `chat/ChatShell`。
- Agent 选择：Chat 有 `chat/AgentPicker.tsx`，Learn 在 `NewStudyForm.tsx` 里另写了一套（`learnAgents` 过滤和模型输入），两份逻辑重复。

### 4.3 假设

- Research / Discuss 的具体范围由本计划给出默认决策（§8），用户确认后写入需求文档。
- 各 CLI 在 `explore`（只读）模式下能否联网搜索因 CLI 而异。Research 不额外提供联网能力，只用 CLI 自身的能力，并按真实结果记录。

## 5. 目标设计

### 5.1 Rust 模块结构

```text
src-tauri/src/
  agent/                  # Agent 调用层：唯一启动 Agent CLI 的地方
    mod.rs                # 公开接口（下列类型与函数）
    registry.rs           # 原 agents.rs + agents/config.rs：加载、发现、启用、可用性
    capabilities.rs       # 原 agent_adapter::capabilities 与能力类型
    invocation.rs         # 原 prepare_invocation / validate_model_name / 权限映射（内部）
    diagnostics.rs        # 原 agent_diagnostics.rs：诊断 + check_startable（带缓存）
    runner.rs             # 原 chat/runtime.rs 的通用部分：启动、stdin、限额、超时、取消、进程组
    stream.rs             # 原 parse_chat_stream_line + agents/stream.rs：CLI 输出 → AgentEvent
    record.rs             # 脱敏执行记录（调用、退出码、工具调用数、日志路径）
    error.rs              # AgentError
  chat/  learn/  research/  discuss/   # 功能模块，只 use crate::agent::{...}
  process_supervisor.rs   # 保留；ProcessOwner 新增通用 Feature 变体
```

`agent` 的公开接口（草案，以实现时的测试为准）：

```rust
pub struct AgentRunSpec {
    pub agent: AgentConfig,
    pub model: Option<String>,
    pub permission: Permission,          // Explore / Ask / Auto（与 Chat 现有取值一致）
    pub cwd: PathBuf,
    pub prompt: String,                  // 首轮输入或续聊新增输入
    pub resume: Option<ResumeHandle>,    // 原生续聊句柄，由功能模块保存
    pub system_append: Option<String>,   // Chat 内联卡片等功能附加内容
    pub limits: RunLimits,               // 超时、输出字节、行数
    pub owner: RunOwner,                 // { feature: "chat"|"learn"|"research"|"discuss", scope_id, run_id }
    pub log_dir: Option<PathBuf>,        // 由功能模块指定日志位置，内容由 agent 层脱敏写入
}

pub enum AgentEvent { Started { pid }, Text { delta, block_start }, Tool(ToolEvent), Log { stderr, line }, Usage(..) }
pub struct AgentOutcome { content, parts, resume: Option<ResumeHandle>, status, exit_code, termination_reason, tool_calls }
pub enum AgentErrorKind { NotFound, Disabled, Unsupported, BrokenInstall, InvalidModel, Timeout, Cancelled, Shutdown, Exited }
pub struct AgentError { kind, summary: String, details: Option<String> }

pub async fn run(spec: AgentRunSpec, cancel: CancelToken, on_event: impl FnMut(AgentEvent)) -> Result<AgentOutcome, AgentError>;
pub async fn check_startable(agent: &AgentConfig) -> Result<(), AgentError>;
pub fn list_agents(app: &AppHandle) -> Result<Vec<AgentConfig>, String>;
pub fn capabilities(adapter: &str) -> AgentCapabilities;
```

约定：

- **权限映射**：`AgentStage` 降为 `invocation.rs` 内部细节，功能模块只传 `Permission`。Learn、Research、Discuss 固定用 `Explore`。
- **续聊**：`ResumeHandle` 的生成与解析在 agent 层；保存在哪里由功能模块决定（Chat 仍写 v2 会话）。Chat 的续聊指纹逻辑（`chat::resume::binding`）留在 Chat，但只依赖 agent 层的公开类型。
- **错误**：IPC 仍然返回 `String`，格式统一为"摘要 + 空行 + 原始详情"。前端用共享的 `ErrorText` 解析，并兼容旧的 `; stderr:` 格式。这个约定写进契约文档。
- **启动前检查**：结果按 Agent 的 `(id, command 解析路径, mtime)` 缓存，应用会话内有效。Agent 配置变化、运行失败或 Settings 里手动重新检测时清除缓存。这样 Chat 每回合发送前检查也不会增加明显延迟。
- **进程归属**：新增 `ProcessOwner::Feature { feature, scope_id, run_id }`，新功能统一用它；Chat 与 Learn 保留现有变体，避免影响关闭流程（`chat/shutdown.rs`）和日志格式。`ProcessKind` 同理新增 `Feature`。

### 5.2 前端结构

```text
src/
  shell/                  # 应用外壳：ActivityRail、AppShell（原 ChatShell）、功能注册表
  components/             # 跨功能共享：Markdown（原 chat/MarkdownView）、externalLink、ErrorText、AgentPicker
  api/agents.ts           # 列出 Agent、诊断、能力；各功能 API 各自独立（chatClient / learn / research / discuss）
  features/{chat,learn,research,discuss,settings,skills}/
```

- **功能注册表**（`src/shell/features.ts`）：每项 `{ id, icon, labelKey, load: lazy(...), keepMounted }`。`AppView` 由注册表的 id 加上 `"settings"` 生成。
  - Chat 设 `keepMounted: true`（与现状一致），其它功能按需加载，切走后保持挂载以保留界面状态。
  - Research / Discuss 在各自阶段交付前不出现在注册表里，不放"即将推出"占位。
- **ActivityRail**：
  - 宽 52px，图标 + 提示（`title` 与 `aria-label`），当前项高亮，样式参考 Codex 截图；
  - 顶部依次是 Chat、Learn、Research、Discuss，底部是 Settings；
  - 快捷键：`Cmd+1…4` 切功能，`Cmd+,` 打开 Settings；
  - 窄窗口仍保留图标栏，第二栏按现有规则折叠或堆叠。
- **入口迁移**：Chat 侧边栏底部的 Learn 按钮移除，Settings 移到图标栏底部。Chat 侧边栏只保留项目和会话树（以及现有的新建会话等操作）。
- **共享 AgentPicker**：
  - 合并 `chat/AgentPicker.tsx` 与 Learn 表单里的选择逻辑：Agent（显示可用性与诊断状态）+ 可选模型（仅 `modelSelection` 为真的 Agent）+ 可选权限；
  - 不可用的 Agent 显示原因，不能提交；
  - Chat 继续用它（不显示模型输入，保持现状）；Learn、Research、Discuss 显示模型输入；
  - UI 只传 `agentId` / `model` / `permission`，不拼接 CLI 参数。
- **设计稿**：先在 `designs/` 画图标栏与四个功能的布局稿（深浅色、窄窗口），确认后再实现。

### 5.3 Research（MVP，默认决策见 §8）

- **输入**：问题、Agent、模型、可选项目（选了项目时 cwd 为项目根目录，否则为 Loom 管理的空工作目录）。
- **流程**：Agent 在 `Explore` 权限下调研，按提示词输出 Markdown 报告：结论、依据、引用、未解决问题。
  - 引用沿用 Learn 的 `loom-src:`，项目内路径在读取时校验；外部链接用 `http(s)`。
  - 追问在同一份报告里续写：优先原生续聊，没有原生续聊就重放有预算的历史。
- **存储**：`<appData>/research/<id>/{research.json, turns/<n>.md, jobs/...}`，不写入用户项目。
- **只读保证**：选了项目时，与 Learn 一样在运行前后比对项目工作区状态（`git status --porcelain` + HEAD）。发生变化就标警告，并在报告顶部显示（用户项目可能本来就有并发修改，所以不判失败）。

### 5.4 Discuss（MVP，默认决策见 §8）

- **输入**：议题、2–4 个参与 Agent（各自可填模型）、可选项目、主持 Agent（默认第一个）。
- **流程**：
  1. 第一轮：各参与者独立作答，并行运行，并发上限 = 参与者数。
  2. 第二轮：把其他人的回答作为输入，各自评审并修订。
  3. 汇总：主持 Agent 产出共识、分歧（标明来源）、风险、建议。
  - 每一步都是一次 `agent::run`。
- **成本可见**：开始前显示"预计 N 次 Agent 调用"（2 人为 5 次，4 人为 9 次），用户确认后才开始。
- **失败处理**：单个参与者失败（含启动前检查失败）时，其余继续；汇总里注明谁缺席。可以单独重试某个参与者的某一轮。
- **存储**：`<appData>/discuss/<id>/{discussion.json, rounds/<n>/<agentId>.md, summary.md, jobs/...}`。

## 6. 实施阶段

各阶段可以独立 Review 和提交。P1–P4 只重构、不改行为；P5 是导航改版；P6、P7 是新功能。

### P0 前置：固定基线

- **产出**：Learn 当前改动单独提交（用户验收后，或按用户指示先提交），使重构的 diff 与 Learn 分开；记录 Chat / Learn 真实测试基线（Codex）。
- **依赖**：用户决定 Learn 提交时机（§8-1）。
- **验证**：`pnpm check`；`git status` 只剩本计划文档。

### P1 特征测试（重构保护网）

- **产出**：在移动代码前，把 Chat 与 Learn 目前的外部行为固定成测试：
  - 4 个 CLI 的流解析：取录制的 stdout 样例（已有测试数据就复用），断言 `parse_chat_stream_line` → `ChatStreamEvent` 序列与 `ChatTurnOutcome` 字段；
  - runner 行为：超时、取消、输出限额、进程组清理、shutdown，已有测试就补齐边界；
  - Learn：伪 CLI 作业的阶段、页面与日志文件内容。
- **路径**：`chat/mod.rs` tests、`chat/runtime.rs` tests、`learn/jobs.rs` tests；新增测试数据放在 `src-tauri/tests/fixtures/agent-streams/`。
- **验证**：新测试在当前代码上通过（作为基准）。

### P2 建立 `agent/` 并迁入配置、能力、参数、诊断（只移动，不改行为）

- **产出**：
  - `agents.rs`、`agents/*`、`agent_adapter.rs`、`agent_diagnostics.rs` 移入 `agent/{registry,capabilities,invocation,diagnostics,stream}.rs`；
  - `mod.rs` 给出公开接口，调用方改 import；
  - Tauri 命令名和 `contracts/tauri-contract.json` 不变。
- **依赖**：P1。
- **验证**：`pnpm check`；契约检查不变；P1 特征测试通过。

### P3 抽出 runner 与通用事件（核心步骤）

- **产出**：
  - `chat/runtime.rs` 的通用部分移到 `agent/runner.rs`，`parse_chat_stream_line` 的通用解析移到 `agent/stream.rs`，输出 `AgentEvent` / `AgentOutcome`；
  - `chat/runtime.rs` 只剩把 `AgentEvent` 映射为 `ChatStreamEvent` 的薄转换，以及 Chat 自有字段（message id、内联卡片、文件变更）；
  - Learn 改用 `agent::run`，删除 `learn/jobs.rs` 对 `crate::chat` 的引用；
  - 新增 `ProcessOwner::Feature`；
  - 新增 `scripts/check-layers.mjs` 并接入 `pnpm check`（S1、S2 的 Rust 部分）。
- **依赖**：P2。
- **验证**：
  - P1 特征测试全部通过，事件序列不变；
  - `pnpm check`、`smoke:visual`；
  - Codex 真实 Chat 回合（流式、停止、续聊）与 Codex 真实 Learn 研究。

### P4 统一启动前检查与错误格式

- **产出**：
  - `AgentError` 与"摘要 + 空行 + 详情"约定；
  - `check_startable` 加缓存，Chat 发送前也调用；
  - `diagnose_agent` 把 `--version` 非零退出标为 `broken` 并在 Settings → Agents 显示（前端新增状态文案）；
  - `ErrorText` 移到 `src/components/`，Chat 的错误展示也改用它；
  - Skills 本地翻译（`skills/translation/local.rs`）改用 `agent::run`；若它的非 Agent 用法不适合，在契约里记录为例外并说明原因。
- **依赖**：P3。
- **验证**：坏 CLI 与好 CLI 的单测；Chat 发送时遇到坏 CLI 的前端测试；`pnpm check`。

### P5 前端外壳与左侧功能栏

- **产出**：
  - `designs/` 布局稿；
  - `src/shell/`（`ActivityRail`、`AppShell`、`features.ts`）；
  - `src/components/`（`Markdown`、`externalLink`、`ErrorText`、`AgentPicker`），并把 `features/*` 之间的引用全部改为引用 `components/`；
  - `api/agents.ts`；
  - `AppView` 改由注册表生成，移除 Chat 侧边栏里的 Learn / Settings 按钮；
  - 快捷键、中英文案、窄窗口布局；
  - `check-layers.mjs` 增加前端规则；
  - `smoke:visual` 增加功能栏矩阵，Learn smoke 的入口改为点击图标栏。
- **依赖**：P4（`ErrorText` / `AgentPicker` 的接口）；与 P3 无代码冲突，但按顺序做可以减少返工。
- **验证**：`pnpm check`、`smoke:visual`（S6）；桌面人工检查切换时 Chat 回合不中断。

### P6 Research MVP

- **产出**：
  - Rust `research/`（存储、提示词、作业、命令 `research_*`、事件 `loom://research-event`）；
  - 前端 `features/research/`（列表、新建、报告页、追问、日志）；
  - DEV fixture；契约文档；需求文档的 Research 范围（用户确认后写入）。
- **依赖**：P5；§8 的 Research 决策。
- **验证**：单测（存储、提示词、引用校验、追问、取消）；合成 smoke；Codex 真实用例并写入验收记录。

### P7 Discuss MVP

- **产出**：
  - Rust `discuss/`（轮次编排、并行运行、部分失败、单项重试、汇总）；
  - 前端 `features/discuss/`（参与者选择、调用次数确认、按轮次对照查看、汇总页）；
  - fixture、契约、需求文档。
- **依赖**：P5；P6 的报告渲染与引用组件可以复用（通过 `components/`）。
- **验证**：
  - 伪 CLI 单测：并行、一个失败、取消、重试；
  - 合成 smoke；
  - 真实用例至少 Codex 加一个其它 CLI；若受本机认证限制，标 blocked 并写明原因。

### P8 文档与验收

- **产出**：
  - 新增 `docs/architecture/agent-layer.md`（接口、错误约定、进程归属、缓存、例外清单）；
  - 更新 `architecture.md`、`agent-adapter.md`、`security-policy.md`、`testing.md`（矩阵与人工验收步骤）、`learn-contracts.md`（改为引用 agent 层）、`requirements.md`；
  - `docs/dogfood/` 验收记录；回填本计划第 0 节"实施结果与偏差"；同步更新 `docs/PLANS.md`。
- **验证**：发布文档检查；完整门禁：`pnpm check`、`pnpm tauri build --no-bundle`、`smoke:visual`、真实用例。

## 7. 风险与回滚

| 风险 | 影响 | 应对 |
|---|---|---|
| P3 移动 runner 引起 Chat 回归（流式、停止、续聊、关闭流程） | 高 | 先做 P1 特征测试；先原样移动再泛化，分两个提交；每步跑 Codex 真实回合；出现问题就回退该提交 |
| 启动前检查给 Chat 每回合增加延迟（Node 包装的 CLI `--version` 约 0.1–0.5 s） | 中 | 按路径和 mtime 缓存，只在首次和失败后检查；在验收记录里记下实测延迟 |
| 并行 Discuss 的资源与额度消耗 | 中 | 最多 4 人；开始前显示调用次数并要求确认；超时与 Learn 同级（单次 15 min） |
| 各 CLI 只读模式下的联网能力不一致，Research 质量参差 | 中 | 不承诺联网；报告标注使用的 Agent 和模型；真实结果按 CLI 分别记录 |
| 前端大范围移动文件与 Learn 未提交改动冲突 | 中 | P0 先提交 Learn |
| `ProcessOwner` 序列化变化影响日志或关闭流程 | 低 | 保留现有变体，只新增 `Feature` |
| 冻结的 Workbench 源码引用旧路径 | 低 | 它不在编译范围内；不改它，在契约里注明 |

回滚：每个阶段单独提交；P2–P4 是纯重构，可以逐个 revert；P5 之后的功能通过注册表的 `enabled` 隐藏，不需要回退存储，因为新功能的数据都在各自独立的应用数据子目录里。

## 8. 待确认问题（已给默认决策，不阻塞 P0–P5）

1. **Learn 何时提交**：默认等你完成桌面验收后提交，再开始 P1。
2. **Research 范围**：默认"可选项目 + 单 Agent + 报告 + 追问"，数据放应用数据目录，不写入项目。
3. **Discuss 形式**：默认 2–4 人、两轮 + 汇总、主持人为第一个参与者、开始前确认调用次数。
4. **图标栏是否显示未交付的功能**：默认不显示，各功能交付时再加入。
5. **启动时打开的视图**：默认总是打开 Chat（主页），不记忆上次的视图。

## 9. 验证策略汇总

- **每阶段**：`pnpm check`（含 `check-layers.mjs`）；涉及 UI 的阶段加 `smoke:visual`；预览前后执行 `scripts/debug.sh stop`。
- **P3 / P6 / P7 结束**：Codex 真实用例（Chat 回合、Learn 研究、Research 报告、Discuss 讨论）。Claude 等其它 CLI 视本机认证情况，如实标 passed 或 blocked。
- **P8**：完整门禁加用户桌面验收，记录到 `docs/dogfood/`。
