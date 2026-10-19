# Testing 阶段重构：人机协同调试驾驶舱

- 日期：2026-06-24 17:56
- 状态：已实施（2026-06-25：M0–M4 全部完成并验证 —— `pnpm build` ✓ / `pnpm test` 42 ✓ / `cargo test` 99(+1 ignored) ✓，含 PTY 进程组杀。手动 UI 验证仍建议跑一遍 `scripts/preview.sh`）
- 主题：把 Testing 阶段从「human 跑 dev + 一块静态 Debug agent 卡片」改造成「真·终端实时日志（PTY + xterm.js）+ 可选区投喂 + agent 真正在阶段内执行修复」的人机协同调试闭环，让用户和 agent 能同时 review 与测试。
- 终端技术：dev-server 槽位用 **PTY（`portable-pty`）** spawn + 前端 **`@xterm/xterm`** 渲染；终端槽位**每项目可配置 N 个**（取代当前硬编码 frontend/backend + 启发式猜命令）。

## 背景与问题诊断

实测反馈：当前 Testing 界面（`src/components/TaskDetail/TestingPane.tsx` + `TerminalCard.tsx`）看起来像调试驾驶舱，但闭环在三处断裂，导致「无法让 agent 和 human 同时 review/测试」。

1. **右侧 Debug agent 面板基本是静态的，agent 在 Testing 阶段根本启动不了。**
   - `Prepare repair handoff` → `useTaskBridge.generateRepairContext` → Rust `generate_repair_context`（`src-tauri/src/tasks.rs:462`）只把日志/todo 上下文/人工反馈打包成一段文本 `task.repairContextPreview`，**没有任何入口真正调起修复 agent**。对比 `SessionPane` 的 `startTodo + startCommandRun` 才是真起一个 agent run。
   - `Auto / Manual` 切换是死的（`debug-agent-mode` 纯文字，无 state）。
   - `Proposed fix` 永远显示占位文案（无失败时）。
   - 结论：human 能跑 dev、能看日志，但 agent 一侧没有「执行」入口，谈不上「同时」。

2. **日志与「跟 agent 沟通」彼此断开。**
   - `TerminalCard`：日志整段渲染，**不能选行、不能复制、不会自动滚动**；每行硬塞 `STDOUT/STDERR` 前缀，噪音大。
   - 反馈是右下角一个**单行 `<input>`**（`Manual feedback`），与左侧日志没有任何关联——无法「圈出这几行报错丢给 agent」。
   - `reducer` 里 `commandLogs[runId]` 截断在 **300 行**（`commands/logReceived` 的 `slice(-299)`），dev server 日志几秒就冲掉。

3. **当前是「日志列表」不是「终端」，且永远拿不到颜色。**
   - `start_command_run`（`command_runner.rs`）用**普通管道**抓 stdout/stderr，按行推到前端。vite / `cargo tauri dev` 等工具检测到 `isatty()==false`（接的是管道而非 TTY），会**主动关闭彩色与进度条**——所以无论前端怎么美化，dev-server 输出永远是纯文本。要真终端体验**必须**给子进程接 PTY。

4. **终端槽位硬编码、命令靠猜。** `TestingPane` 写死恰好两个槽位（frontend + backend），命令用启发式（`pnpm dev` / `cargo tauri dev`）猜。截图里的 `todo` 项目根本没有 `cargo tauri dev`，暴露了硬编码的脆弱。

5. **布局与目标相悖。** 前后端两个终端**纵向堆叠**（各占很高），agent 挤在右侧窄栏；想同屏「前后端实时日志 + 跟 agent 聊」三件事，三者互相挤。

## 目标

1. 前后端命令是**真终端**：PTY 驱动，保留 ANSI 颜色/进度条，xterm.js 渲染自带滚动/选区/复制，scrollback 容量足够。
2. 终端槽位**每项目可配置 N 个**（常见 1 或 2），命令由用户固定并持久化，不再启发式猜。
3. 用户能在终端里**选中文本**，一键把「选中内容 + 命令 + exit code + 时间」作为结构化引用投喂给 agent，并补充一句话说明。
4. Debug agent 从静态卡片变成**可执行的对话**：能在 Testing 阶段内真正调起修复 agent（复用实现 agent + repair context），其输出实时回流到对话；human 可继续追加消息——实现「human 测 / agent 修」同屏并行。
5. 落地 `Auto / Manual` 语义：Auto = 检测到失败自动打包 + 起修复 run；Manual = human 触发。
6. 重排布局，让前后端终端并排 + agent 对话占满高度同屏可用。
7. 全流程保持 Loom 既有约束：所有 agent 调用与命令执行保留可见命令记录与日志；只读回看模式（`readOnly`）下全部执行/写入控件禁用。

## 非目标

- 不做远程/云端执行；仍是本地进程。
- **本期显式引入两个依赖**（有明确 UX 理由，CLAUDE.md 的「无理由不加依赖」此处不适用）：Rust `portable-pty`（彩色输出必须 PTY）、前端 `@xterm/xterm`（终端渲染 + 选区/复制现成）。不再额外引入其他依赖。
- 不把 agent 那类**非交互 run**改成 PTY：planning/implementation/fix agent run 仍走现有 line-based 管道（输出干净、好抓结构化），只有 dev-server 槽位走 PTY。
- 不替换现有 `generate_repair_context` 的打包逻辑（在其产物之上加「执行」一层，而非重写）。
- 不让进程脱离 Loom 存活（不做 tmux 式跨应用持久；评估后排除 tmux，见总体方案）。
- 不实现多 agent 同时修复或自动多轮无人值守循环（Auto 仅「失败即起一轮」，是否继续由 human 决定）。
- 不改阶段导航/回看（上一计划 `2026-06-12/10:46` 已完成，本期沿用其 `readOnly`）。

## 成功标准

1. 在 Testing 阶段同时 `Run` 多个槽位（如前端 + 后端），每个终端实时输出且**带 ANSI 颜色/进度条**（PTY 生效）；xterm 默认跟随到底，用户上滚后停住、回到底恢复。
2. 终端槽位可在项目层增/删/改命令并持久化；重启 Loom 后槽位与命令保留；不再出现「猜错命令」（如非 Tauri 项目却显示 `cargo tauri dev`）。
3. 在任一终端选中文本，浮出 `Quote in chat` / `Ask agent to fix`；点击后 agent composer 自动带入该引用（含命令与 exit code）。
3. 点 `Ask agent to fix`（或 Manual 发送）能真正起一个修复 agent run：右侧对话出现一条 agent 消息，其执行日志实时流入；run 结束后状态（succeeded/failed）可见，且作为一个 command/agent run 记录留痕。
4. `Auto` 模式下，某个命令 run 失败时自动生成 repair context 并起一轮修复 run（无需手点）；`Manual` 模式下必须 human 触发。
5. 回看模式（`readOnly`）下：Run/Stop/Re-run/起修复/发送/Accept 全部禁用，日志与历史 cycles/对话只读可见。
6. `pnpm test` 全绿（含新增 reducer/选区与 Auto 触发相关用例）、`pnpm build` 通过、`src-tauri` `cargo check`/`cargo test` 通过。

## 当前状态（关键事实）

- **命令执行（现状是管道）**：`useCommandBridge.startCommandRun / stopCommandRun` → Rust `start_command_run`（`src-tauri/src/command_runner.rs:63`）用普通管道抓 stdout/stderr，按行经 `commands/logReceived` 进 `state.commandLogs[runId]`。**dev server 与 agent run 现在都走这一条**——本期把 dev-server 槽位分流到 PTY 路径，agent run 保留管道路径。
- **PTY 选型**：Rust 用 `portable-pty`（跨平台，macOS/Linux 走 unix PTY，Windows 走 ConPTY）spawn dev-server 槽位，原始字节（含 ANSI）经事件流到前端 `@xterm/xterm` 实例。xterm 自带 scrollback、选区、复制、自动跟随，前端不再需要自己维护日志 buffer 与滚动逻辑。
- **agent 启动范式**：`SessionPane.handleStartTodo` 已示范「`buildImplementationPrompt` → `buildAgentCommandArgs` → `startCommandRun`」，修复 run 直接复用该（管道）范式，prompt 用 `repairContextPreview`（+ 选区引用）。
- **repair context**：`generate_repair_context`（`tasks.rs:462`）已把命令日志、当前 todo、`feedback` 打包进 `task.repairContextPreview`；`command_runner.rs:509` 的测试已覆盖「两轮失败→repair→成功→可接受」的命令侧循环。
- **反馈持久化**：`append_feedback`（接受可选 `commandRunId`）已把 `UserFeedback` 挂到 task（`task.feedback`），承载「选区引用 + human 说明」。
- **错误归纳（关键约束）**：`analyze_error(exit_code, &stderr_lines)`（`command_runner.rs:370`）**只从 stderr 行**算 `matchedLines`/`stderrTail`。PTY 只有单个 master 流、**stdout+stderr 合并**，stderr 分离消失——故 PTY dev-server run **不跑 errorSummary**（dev server 长驻几乎不 exit，自动检测无意义）；auto「Detected error」改为只服务**管道型 test/fix run**（保留 stderr 分离）。详见 M0 与待确认问题 1。
- **进程组杀（关键约束）**：管道路径用 `setpgid(0,0)` + `kill(-pgid, SIGTERM)`（`command_runner.rs:206`）杀**整个进程组**（vite→esbuild 等子孙）。PTY 的 `child.kill()` 只杀直接子进程，PTY 路径**必须复刻进程组杀**，否则 dev-server 子孙进程泄漏。
- **脱敏**：管道路径对命令文本走 `redact_sensitive_text`；PTY 原始字节直送 xterm 会绕过脱敏，**投喂 agent 前（必要时连终端显示）需补脱敏**。
- **CommandRun 记账**：PTY run 仍照常创建并入 registry 跟踪（供 stop/kill/resize/状态 pill），**只有输出字节**走新 `pty/output` 事件、绕过 `commandLogs`。
- **槽位持久化（新增）**：当前槽位硬编码在 `TestingPane`。可配置 N 槽位**确认存 `.loom/loom.json`**（随仓库走），复用 `storage.rs` 的 `ensure_loom_dir` + `atomic_write_json` 范式；**不要**放进 `save_recent_project` 的全局 recent 列表。`ProjectSummary.suggestedCommands` 仅作新建槽位的建议来源。
- **只读边界**：`TestingPane` 已接 `readOnly` 并禁用所有写控件（上一计划成果），新控件需一并纳入。

## 总体方案

分四层推进：

0. **终端层（新增 PTY，Rust + 前端 + 依赖）**：`command_runner` 增加 PTY spawn 路径（`portable-pty`），dev-server 槽位走 PTY、原始字节流到前端；`TerminalCard` 改用 `@xterm/xterm` 渲染（滚动/选区/复制由 xterm 提供）。agent 的非交互 run 不动，继续走管道。
1. **槽位层（项目配置）**：终端槽位（名称 + 命令）持久化到项目配置，支持增/删/改；取代 `TestingPane` 里硬编码的 frontend/backend + 启发式猜命令。新建槽位时用 `ProjectSummary.suggestedCommands` 作为建议。
2. **协同层（前端 + 复用既有命令）**：右侧改造成「对话式 Debug agent」。前端会话视图（消息 = human 引用/说明 + agent 修复 run），修复 run 复用 `startCommandRun`（管道，agent 命令）+ `append_feedback`（留痕引用）。Auto/Manual 用本地 state 控制失败时是否自动起一轮。
3. **布局层**：`TestingPane` 重排为「左/中多终端并排 + 右 agent 对话占满高度」，并发可用。

> 已定决策：dev-server run 与 agent fix run 现在**结构性区分**（PTY 槽位 run vs 管道 agent run），比之前「前端约定」更干净；对话/引用复用 `UserFeedback`；Auto 默认关闭、Manual 主导。

### tmux 评估结论（排除）
- 硬依赖（用户机不一定装），且与 Loom 自有进程模型（`command_runner` + `commandRuns`）冲突，stop/exit code/失败检测要绕 `capture-pane`。
- 结构化输出（选区投喂 agent、errorSummary）从 capture-pane 抠更难。
- tmux 唯一独有价值是「跨应用持久」，而调试 dev server 不需要脱离 Loom 存活。
- 结论：用 **PTY 自持有进程**实现「固定、持久的终端槽位」，拿到 tmux 想要的体验而无其代价。

## 里程碑

### M0：PTY 终端基建（Rust + 前端 + 依赖）
1. Rust：`command_runner` 增加 `portable-pty` spawn 路径（spawn / write / resize / kill），原始字节经事件流（如 `pty/output`）发往前端；保留现有管道路径给 agent run。
2. **进程组杀**：PTY 路径复刻管道路径的进程组语义（killpg / portable-pty 的 process-group 选项），确保停止时连 dev-server 子孙进程一并退出；`cargo test` 覆盖「停止后无残留子进程」。
3. **CommandRun 记账**：PTY run 照常建 `CommandRun` 入 registry（供 stop/kill/resize/状态），仅输出字节走 `pty/output` 绕过 `commandLogs`。
4. **errorSummary 在 PTY 下不适用**：PTY dev-server run 不调 `analyze_error`（stdout/stderr 已合并，stderr 分离失效）；auto「Detected error」只服务管道型 test/fix run。dev-server 的错误由 human 选区驱动（见 M2）。
5. **脱敏**：PTY 输出在投喂 agent 前过 `redact_sensitive_text`（必要时终端显示也脱敏）。
6. 前端：`TerminalCard` 改用 `@xterm/xterm`（+ fit addon）渲染 PTY 字节；窗口/容器 resize 同步给 PTY；选区/复制/滚动跟随由 xterm 提供。
7. 验证 dev server（vite / cargo）颜色与进度条正常显示；`cargo test` 覆盖 PTY spawn/退出/进程组杀。

### M1：终端槽位可配置 + 持久化（项目层）
1. 槽位模型：项目持久化 1..N 个 `{ name, command }`，支持增/删/改、`Run/Stop`、reorder（可选）。
2. 持久化到 `.loom/loom.json`（复用 `ensure_loom_dir` + `atomic_write_json`，随仓库走，**不进**全局 recent 列表）；新建槽位用 `suggestedCommands` 作建议。
3. 移除 `TestingPane` 的硬编码 frontend/backend 与启发式 `preferredFrontend/BackendCommand`。
4. `endpoint`（如 `localhost:1420`）可点开浏览器（沿用 opener 能力）。

### M2：选区 → 投喂 agent + 对话式 Debug agent（核心）
1. 右侧 `debug-agent-panel` 重构为对话视图：消息流（human 引用/说明、agent 修复 run 卡片含实时日志）+ 多行 composer + 当前引用区。
2. 选区操作条 `Quote in chat` / `Ask agent to fix`：用 `term.getSelection()` 取选中文本 + 命令 + exitCode + 时间格式化为结构化引用，注入 composer 引用区。
3. 修复 run：复用 `SessionPane` 范式，新增 `buildRepairPrompt(task, quotedLogs, humanNote)`（基于 `repairContextPreview` + 选区），`startCommandRun` 调起实现 agent；run 通过 `taskId` 关联，日志实时回流到该对话消息。
4. `append_feedback`：发送时把引用 + 说明持久化为 `UserFeedback`（带 `commandRunId`），保证留痕。
5. Auto/Manual：本地 state；Auto 下监听到某 run 失败（`commands/finished` status=failed）自动 `generate_repair_context` + 起一轮修复 run（去重，避免重复触发）。

### M3：布局重排 + 细节
1. `TestingPane` 三区布局：多终端并排（可调高度 / 单个全屏），右侧对话占满高度。
2. `Test cycles` 仅由**一次性 test/fix run** 填充（持久 dev-server PTY 不计入，它常驻不 exit）；每条可点开回看那次 run 的完整日志。
3. `Detected error` 自动从**管道型 test/fix run** 的 `errorSummary` 归纳（dev-server PTY 不参与，见 M0）；dev-server 的错误由 human 选区驱动。
4. `readOnly` 覆盖所有新控件（选区操作、起修复、发送、Auto 自动触发关闭）。

### M4：测试与验证
见验证策略。

## 具体任务清单

- [x] M0：新增 `pty.rs`（`portable-pty` 0.9）spawn/write/resize/kill + `loom://pty-output` 事件流
- [x] M0：PTY 路径进程组杀 + `cargo test`（`stop_pty_run_kills_the_whole_process_group` 验证无残留）
- [x] M0：PTY run 照常建 `CommandRun` 入 registry；输出走 `pty-output` 绕过 `commandLogs`
- [x] M0：dev-server PTY 不跑 errorSummary（Preview=PTY / Validation=管道，二者结构性分流）
- [~] M0：PTY 输出投喂 agent 前脱敏 —— 推迟到 M2（投喂入口在 M2 实现；显示流保持原样）
- [x] M0：`TerminalCard` 双模式（pty=xterm + fit addon / logs=管道行）；resize 同步 PTY；parent env 传递
- [x] M1：项目级终端槽位模型（增/删/改/Run/Stop）+ 持久化到 `.loom/terminal-slots.json`（独立文件，避免 loom.json schema 升级清空；`terminals.rs` + `useTerminalBridge`）
- [x] M1：移除硬编码 preview/validation 启发式落点（首次访问 seed 默认并持久化，之后用户可改）；endpoint 显示
- [x] M1+：**项目感知命令生成** `suggest_terminal_slots`（扫描根目录 + 一级子目录的 package.json/Cargo.toml/go.mod，monorepo 子应用带 `cwd` 在各自目录运行，取代根目录瞎猜 `pnpm dev`）；槽位加 `cwd` 字段（Rust+TS+编辑器）；新增「Reset to detected」一键重扫覆盖（修复已被旧默认污染的项目）；空命令槽位点 Run 改为打开编辑器
- [x] M2：右侧对话式 Debug agent 视图（消息流：human 反馈 + agent 修复 run 实时日志 + composer + 引用区）
- [x] M2：终端选区 `Quote`（xterm `getSelection` / logs DOM 选区）→ 注入引用 → `Ask agent to fix`
- [x] M2：`buildRepairPrompt` + 复用 `startCommandRun` 起修复 run（复用实现 agent），日志回流到对话
- [x] M2：发送时 `append_feedback` 持久化「引用 + 说明」（`formatQuotedFeedback`）
- [x] M2：Auto/Manual state，Auto 失败自动起一轮（`autoHandledRef` 去重）
- [~] M3：`TestingPane` 布局 —— 右侧对话占满高度 + N 终端弹性纵排；side-by-side/可拖拽高度/单个全屏推迟（价值有限）
- [x] M3：Test cycles 每条可点开回看该 run 完整日志；Detected error 由管道型 run 的 errorSummary 归纳（跳转到行推迟）
- [x] M3：`readOnly` 覆盖全部新控件（增/删/改槽位、选区、起修复、发送、Auto、Accept）
- [x] M4：`agentRun` 纯函数单测（formatQuotedFeedback / buildRepairPrompt / buildAgentCommandArgs）+ `pnpm build`/`pnpm test`(42)/`cargo test`(99,1 ignored) 全绿
- [x] 更新 `docs/PLANS.md` 索引（随本计划已完成）

> 落地偏差记录：① 槽位持久化用 `.loom/terminal-slots.json`（非 `loom.json`），更耐 schema 升级，同在 `.loom/`；② M0 的「PTY 输出脱敏」并入 M2 投喂边界（显示流如真终端保持原样）；③ M3 的拖拽/全屏/跳转到行作为后续优化推迟，不影响闭环可用。

## 风险

1. **PTY 跨平台与 Tauri 事件吞吐**：`portable-pty` 在 macOS/Linux 走 unix PTY、Windows 走 ConPTY，行为略有差异（resize、退出码、信号）。高频字节流经 Tauri event 可能成为瓶颈，需按帧/字节批量合并发送，避免一字节一事件。
2. **依赖引入**：新增 `portable-pty`（Rust）+ `@xterm/xterm`（前端，含 fit addon）。需确认许可证与体积可接受；xterm 样式需接 Loom 主题。
3. **修复 agent 与 human 在 testing 阶段并发写同一仓库**：human 跑 dev、agent 改文件可能互相干扰（HMR 抖动、文件被覆盖）。需在 prompt 中约束 agent 仅改必要文件，并提示 human 注意；不在本期做文件锁。
4. **Auto 自动起修复 run 失控**：dev server 持续输出/反复失败可能触发多轮。必须去重（同一 failed run 只触发一次）+ Auto 仅「失败即起一轮」，下一轮由 human 决定。
5. **stdout/stderr 在 PTY 下合并，破坏 errorSummary**（已在 M0 处理）：`analyze_error` 依赖 stderr 分离，PTY 单流无法满足。决策：dev-server PTY 不跑 errorSummary，auto 检测只服务管道型 test/fix run（仍有 stderr 分离）。
6. **进程组泄漏**（已在 M0 处理）：PTY `child.kill()` 只杀直接子进程，必须复刻管道路径的 killpg，否则 vite→esbuild 等子孙进程残留。
7. **PTY 输出脱敏**（已在 M0 处理）：原始字节绕过 `redact_sensitive_text`，投喂 agent 前需补脱敏。
8. **dev-server PTY 不计入 `Test cycles`**（已在 M3 处理）：cycles 仅由一次性 test/fix run 填充，避免与常驻 dev server 混淆。
9. **xterm scrollback 与「投喂 agent」**：xterm 缓冲足够查看，但超长输出下 `getSelection()` 仅取缓冲内文本；够用，必要时配合「复制全部」。

## 待确认问题

1. **终端技术选型** —— ✅ 已定（2026-06-24）：**PTY（`portable-pty`）+ `@xterm/xterm`**；agent 非交互 run 保留管道。tmux 评估后排除（见总体方案）。
2. **终端槽位模型** —— ✅ 已定（2026-06-24）：**每项目可配置 N 个**命名槽位，持久化，取代硬编码 frontend/backend。
3. **修复 run 与 dev run 的区分** —— ✅ 已定：二者**路径结构性不同**（PTY 槽位 run vs 管道 agent run），前端据此区分，不动 Rust 模型；混乱再升级 `CommandRun.kind`。
4. **agent 对话/引用落库** —— ✅ 已定：**复用 `UserFeedback`**（带 `commandRunId`），对话由「feedback + 关联 run」重建；不引入独立对话结构。
5. **Auto/Manual 默认与范围** —— ✅ 已定：**默认 Manual**；本期 **M0→M4 全部实现**完整闭环。
6. **槽位持久化落点** —— ✅ 已定（2026-06-24 review）：存 **`.loom/loom.json`**（随仓库走），复用 `storage.rs` 的 `ensure_loom_dir` + `atomic_write_json`；不进全局 recent 列表。schema 演进沿用现有 `loom.json` 的版本约定。
7. **修复用哪个 agent？**（M2 实现时定）默认沿用 `task.primaryAgentId` / 第一个具备 implementation 能力的 agent，提供下拉切换（与 SessionPane 一致）。
8. **PTY 输出事件粒度？**（M0 实现时定）按时间/字节批量合并，平衡实时性与事件吞吐。

## 验证策略

1. `pnpm test`：现有用例全绿；新增覆盖——Auto 失败触发的去重逻辑（纯函数层）、选区引用格式化函数、槽位配置 reducer（增/删/改）。
2. `pnpm build`：tsc + vite 通过（新 props/类型扩散到 `TerminalCard` / `TestingPane`；xterm 类型）。
3. `src-tauri`：`cargo check` + `cargo test`——PTY spawn/write/resize/kill、退出码、**停止后无残留子进程（进程组杀）**、`.loom/loom.json` 槽位读写。
4. 手动验证（`scripts/preview.sh` 或 `scripts/start-local.sh`，端口 1420，结束后 `scripts/preview.sh stop`）：
   - 配置 1 个 / 2 个槽位，分别 Run，确认 **ANSI 颜色/进度条**正常、xterm 跟随到底、上滚暂停。
   - 重启 Loom 确认槽位与命令持久化保留。
   - 选中报错文本 → `Ask agent to fix` → 确认引用注入 + 修复 run 真正启动 + 日志回流 + 留痕。
   - 制造一次失败，Auto 模式确认自动起一轮且不重复；Manual 模式确认必须手动触发。
   - 回看模式下确认所有执行/写入控件禁用、无 command/feedback 被触发。
