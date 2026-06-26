# Settings 页面真实化重构计划

- **日期**: 2026-06-25 19:35 CST
- **作者**: Codex
- **状态**: draft（2026-06-26 plan-review 后修订）

## 前置条件

> 开工闸门：本计划 M4–M5 要改的 `command_runner.rs`、`pty.rs`、`loopPolicy.ts`、`TestingPane.tsx`、`SessionPane.tsx` 当前都处于在途 loop-engine 重构（`10:21-loop-engineering-refactor`，标注"实施中"）的未提交改动之中。

- 必须先把 `10:21` loop-engine 的未提交改动落地/提交（或在其分支基线上拉取），得到干净工作树后再开 settings 分支，否则 M4–M5 会与在途工作直接冲突。
- 确认 `loopPolicy` 的 `LOOP_WALL_CLOCK_MS` 与本计划 M5.1 `commandTimeoutSeconds` 的关系（复用同一预算源还是独立超时），避免引入双策略源。

## 目标

把 Settings 页面从“高保真但大多不可用的设置展示页”重构为“只展示可兑现能力的配置入口”。本轮先移除或降级假功能，保留主题、Agent 管理、项目命令/验证入口、安全策略和版本信息等对 Loom MVP 开发闭环有直接价值的设置，并为保留项补齐真实持久化、调用链和验证。

## 非目标

- 不引入插件市场、云同步、团队账号、远程通知服务或自动更新系统。
- 不实现字体选择、密度、强调色、启动登录项、匿名遥测、声音提醒等当前没有产品闭环支撑的偏好项。
- 不重做 Settings 的整体视觉语言；只做必要的信息架构、控件状态和交互收敛。
- 不新增依赖；优先复用现有 Tauri command、`storage.rs`、`.loom/terminal-slots.json` 和 `agents.json`。

## 成功标准

- Settings 页不存在可点击但无效果、看似已启用但没有后端消费的控件。
- 保留的设置项刷新后能保留状态，并被实际业务链路读取或能明确展示只读来源。
- Agent 配置仍支持新增、编辑、启用/禁用、删除自定义 Agent，并保留 built-in 保护。
- Commands / Safety 区域能编辑项目验证/预览命令，或明确跳转到同一份终端槽位配置；执行策略至少覆盖命令超时、危险命令确认和日志脱敏状态。
- About 区域显示真实版本/后端健康状态；不可用的更新检查、更新通道、自动安装不再以可操作设置呈现。
- `pnpm test`、`pnpm build`、`cargo test --manifest-path src-tauri/Cargo.toml` 可通过；Settings 相关 smoke/visual 期望文本同步更新。

## 当前状态

### 已接线能力

- `src/components/Settings/SettingsPage.tsx:193` 加载 Settings 页面状态，并在打开时调用 `loadAgents()`。
- `src/components/Settings/SettingsPage.tsx:325` 的 Light / Dark 主题卡片和 `src/components/Settings/SettingsPage.tsx:641` 的顶栏切换会调用 `useTheme()`；`src/contexts/ThemeContext.tsx:23` 从 `localStorage(loom-theme)` 读取，`src/contexts/ThemeContext.tsx:65` 写回。
- `src/components/Settings/SettingsPage.tsx:371` 的 Agents 页已接入 `useAgentBridge()`；`src/hooks/useAgentBridge.ts:69` 调 `list_agents`，`:81` / `115` / `132` 分别调创建、更新、删除 Agent，`src-tauri/src/agents.rs:143`(list) / `148`(create) / `175`(update) / `202`(delete) 有对应 Tauri commands。
- Agent 配置持久化在全局 app data 下的 `agents.json`，路径由 `src-tauri/src/agents.rs:3079`（`storage::global_config_dir(app).join(AGENTS_FILE)`）管理；built-in Agent 只能启用/禁用，不能编辑/删除，见 `src-tauri/src/agents.rs:232` 和 `245`。
- 项目命令已有更真实的配置面：`src-tauri/src/terminals.rs:1` 说明终端槽位持久化到 `.loom/terminal-slots.json`，`src-tauri/src/terminals.rs:194` / `217` / `226` 支持建议、读取、保存；`src/hooks/useTerminalBridge.ts:5` 是前端桥接。
- 命令执行日志已经做脱敏：`src-tauri/src/command_runner.rs:89` 存储命令文本前脱敏，`src-tauri/src/command_runner.rs:303` 对输出行脱敏；具体规则在 `src-tauri/src/agents.rs:3212`。
- About 可以使用现有 `health_check` 获取真实 app/version/backend/timestamp，命令定义见 `src-tauri/src/lib.rs:34`，注册见 `:53`；外链可用 `tauri_plugin_opener`，初始化见 `:47`。

### 主要问题

- General 页大量静态假功能：workspace name、default project location、restore session、reopen task、launch at login、max parallel agents、confirm commands、language、telemetry、clear cache 都只是静态展示，见 `src/components/Settings/SettingsPage.tsx:284` 到 `320`。
- Appearance 页除 Light / Dark 外均未实现：System、accent color、font、font size、density 都没有状态或业务消费，见 `src/components/Settings/SettingsPage.tsx:325` 到 `367`。
- 主题来源是 localStorage-only：`src/contexts/ThemeContext.tsx:23` 从 `localStorage(loom-theme)` 读取、`:25` 在无显式偏好时已默认跟随系统并监听系统切换，但没有接入任何全局 settings 模型。注意 `src/main.tsx:10` 的 `loom-theme=dark` 强写**只在 `if (previewMode)` 内**生效（预览/截图专用），并非真实 app 的启动行为——真实 app 已跟随系统，因此本轮要做的是把主题接到 settings，而不是改 `main.tsx`。
- Commands & Safety 当前是只读/假控件：命令 presets 仅展示 `state.projects.current?.suggestedCommands` 或硬编码 fallback；高风险策略、timeout、allowed cwd、redact secrets 没有配置模型或执行链消费，见 `src/components/Settings/SettingsPage.tsx:522` 到 `560`。
- Notifications 页全部是静态开关，且当前产品没有通知事件分发或桌面通知权限链路，见 `src/components/Settings/SettingsPage.tsx:565` 到 `586`。
- About 页版本号、build、更新状态、update channel、auto-install、资源链接全部硬编码或无点击处理，见 `src/components/Settings/SettingsPage.tsx:589` 到 `619`。
- 现有 visual smoke 仍以假功能文案作为断言：`scripts/visual-smoke.sh:76` 到 `81` 期待 `Accent color`、`High-risk action policy`、`Notify me when`、`Update channel` 等文本，重构后需要同步。

## 取舍原则

- Settings 只保留“当前版本能读取、写入或明确说明只读来源”的项目。
- 有后端消费链路的设置优先落地；没有消费链路的控件先删，不用 disabled 假装未来会做。
- 全局偏好和项目偏好分开：Agent、主题、少量 app behavior 是全局；终端槽位、验证命令是项目级。
- 安全设置必须能影响实际执行路径；如果本轮只完成审计/显示，则命名为“Current safeguards”而不是“Policy”。

## 里程碑

### M0 - 锁定当前行为和假功能清单

**结果**: 有测试或快照覆盖当前可保留能力，避免重构时误伤 Agent 与主题。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|---|---|---|---|
| 0.1 | 给 `ThemeContext` 补单元测试或最小 DOM 测试，覆盖 localStorage 读取、切换写回、forcedTheme 不写回。 | `src/contexts/ThemeContext.tsx`, `tests/unit/*` | - | `pnpm test` 中新增用例通过。 |
| 0.2 | 给 Agent 设置桥接/后端已有行为补回归：built-in 不能编辑删除，自定义 Agent 可更新删除。 | `src-tauri/src/agents.rs` tests | - | `cargo test --manifest-path src-tauri/Cargo.toml agents::` 相关用例通过。 |
| 0.3 | 记录 Settings 每个 Tab 的保留/删除决定，并在本计划或后续 PR 描述中引用。 | 本计划, PR 描述 | - | Review 时每个删除项都有理由和替代入口。 |

### M1 - 收敛信息架构，删除无用假功能

**结果**: Settings 导航只剩真实或本轮会实现的区域。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|---|---|---|---|
| 1.1 | 将 `settingsTabs` 收敛为 `general`、`appearance`、`agents`、`safety`(label `Commands & Safety`)、`about`，并从 `SettingsTab` 联合类型中移除 `notifications`、删除 `renderNotifications`。注意现有 tab id 是 `safety` 而非 `commands`，本轮保留 `safety` id（可改 label），不新建 `commands` tab，以免破坏 `tab=safety` 深链与 smoke。 | `src/components/Settings/SettingsPage.tsx:45`(`settingsTabs`)、`:30`(`SettingsTab` 类型)、`:565`(`renderNotifications`)、`:629`(renderer map) | M0 | `pnpm build` 无未使用 icon/import/type。 |
| 1.2 | General 只保留当前项目/工作区只读信息和少量真实 app 行为；删除 launch/login、language、telemetry、cache size 等无实现项。 | `renderGeneral()` | M0 | Settings general 页面没有无 handler 的 button/switch。 |
| 1.3 | Appearance 只保留 Light/Dark/System 中本轮可兑现的项；删除 accent/font/size/density，或改为只读“uses system fonts”。 | `renderAppearance()`, `ThemeContext` | M0 | 点击每个可点击主题选项都有状态变化和持久化。 |
| 1.4 | About 删除 update channel、auto-install、fake update check；资源链接只保留可真实打开的本地/外部链接，否则改为纯文本。 | `renderAbout()` | M0 | 无无效按钮；版本信息来自真实数据。 |

### M2 - 全局设置模型和真实读写

**结果**: Settings 页有一个最小全局设置模型，供主题和安全/行为配置复用。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|---|---|---|---|
| 2.1 | 新增 `AppSettings` Rust/TS 类型，字段先限于 `themeMode`, `confirmBeforeCommands`, `commandTimeoutSeconds`。 | `src-tauri/src/models.rs`, `src/domain/app.ts` 或新 `settings.ts` | M1 | TS/Rust 类型编译通过。 |
| 2.2 | 在后端新增 `load_app_settings` / `save_app_settings`，持久化到 app data `settings.json`，复用 `storage::atomic_write_json`。 | `src-tauri/src/storage.rs`, 新 `src-tauri/src/settings.rs`, `src-tauri/src/lib.rs` | 2.1 | Rust 单测覆盖缺省值、损坏文件报错/恢复策略、写入原子性。 |
| 2.3 | 新增 `useSettingsBridge`，Settings 页面加载、保存和错误展示走同一 hook。 | `src/hooks/useSettingsBridge.ts`, `src/state/reducer.ts` | 2.2 | 无 Tauri runtime 时 preview 不崩；Tauri runtime 下 invoke 参数正确。 |
| 2.4 | 把 `ThemeContext` 从 localStorage-only 收敛为 settings-backed：优先级 `forcedTheme` > 持久化 settings > 系统偏好；`toggleTheme` 写回 settings（保留 localStorage 作为无 Tauri runtime 的 fallback）。**不要改 `src/main.tsx:10`**——该行的 dark 强写只在 `previewMode` 内生效，是预览专用，删它会破坏 preview forcedTheme（见风险表）。 | `src/contexts/ThemeContext.tsx`, `useSettingsBridge` | 2.3 | 清空 localStorage/settings 后跟随系统；用户选择后刷新保持；preview 仍强制 dark。 |

### M3 - 保留并强化 Agent 设置

**结果**: Agents 页从“可用但粗糙”变成可信配置面，不被本轮删除误伤。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|---|---|---|---|
| 3.1 | 保留 Agents 页核心表格和表单，明确 built-in 行只允许启用/禁用；编辑按钮对 built-in 不展示或展示说明。 | `renderAgents()`, `isBuiltInAgent()` | M1 | built-in 无编辑误导；自定义仍可编辑删除。 |
| 3.2 | 把 `Default` 列改为真实含义，例如“Implementation capable”或删除；当前 `IMPLEMENT` 只是 capability 派生，不是默认 Agent。 | `src/components/Settings/SettingsPage.tsx:428` | M1 | UI 文案与行为一致。 |
| 3.3 | 增加 “Detect installed” 或 “Refresh availability” 真实按钮，调用 `loadAgents()` 重新检测 `available`。 | `renderAgents()`, `useAgentBridge.loadAgents` | M1 | 安装/卸载 CLI 后刷新状态变化可见。 |

### M4 - 项目命令设置接入终端槽位

**结果**: Commands 页不再展示伪 presets，而是编辑 Testing cockpit 已经消费的项目终端槽位。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|---|---|---|---|
| 4.1 | 将 `safety` tab 的 Commands 区域（`renderSafety`）改为读取当前项目的 `listTerminalSlots`；为空时用 `suggestTerminalSlots` 生成建议。是改 `safety` tab 的 label/区段，不是新建 `commands` tab。 | `SettingsPage.tsx:522`(`renderSafety`), `useTerminalBridge` | M1 | 有项目时展示真实 slot；无项目时显示需要先打开项目。 |
| 4.2 | 支持添加/编辑/删除 slot 的 name、command、kind、cwd，并调用 `saveTerminalSlots`。 | `SettingsPage.tsx`, `src/hooks/useTerminalBridge.ts` | 4.1 | 保存后 `.loom/terminal-slots.json` 更新；TestingPane 重新进入读取同一配置。 |
| 4.3 | 删除 `suggestedCommands` fallback 展示，避免把 “MVP defaults” 误认为用户配置。 | `src/components/Settings/SettingsPage.tsx:211`, `renderSafety()` | 4.1 | 没有项目命令时显示空状态，不展示硬编码 `pnpm install`。 |
| 4.4 | 抽共享 helper，统一 TestingPane 的 slot 编辑/默认值（`buildDefaultSlots`、draft 校验）与 Settings 的逻辑，避免分叉。注意 `SessionPane.tsx:97` 也通过 `useTerminalBridge` 读取 slot，是第三个消费方——helper 与 `.loom/terminal-slots.json` 单一来源需覆盖这三处一致。 | `src/components/TaskDetail/TestingPane.tsx:286`, `src/components/TaskDetail/SessionPane.tsx:97`, 新 `src/utils/terminalSlots.ts` | 4.2 | 三处读取/新增/编辑同类 slot 结果一致。 |

### M5 - 安全策略只保留可执行项

**结果**: Safety 从假策略面板变成实际执行链能消费的护栏，或清晰展示当前固定护栏。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|---|---|---|---|
| 5.1 | 新增或接入 `commandTimeoutSeconds`，让 `start_command_run` / PTY 或前端 loop timeout 使用配置值；若 PTY 暂不支持，则 UI 明确限定适用范围。 | `src-tauri/src/command_runner.rs`, `src-tauri/src/pty.rs`, `src/utils/loopPolicy.ts` | M2 | 设置 1 分钟后长命令按预期终止或前端发起 stop。 |
| 5.2 | `confirmBeforeCommands` 接入前端运行入口；危险命令识别先覆盖 `rm -rf`, `git reset --hard`, dependency install, production-like URL。注意 `commandLine.ts` 当前只有 `parseCommandLine`，**危险识别是净新增函数 + 净新增单测**，没有可复用的既有逻辑。 | `src/utils/commandLine.ts`(新增 helper), `SessionPane`, `TestingPane` | M2 | 新增单元测试覆盖危险命令识别；开启后运行危险命令出现确认。 |
| 5.3 | 日志脱敏改为只读状态说明，不做假 toggle；说明当前始终启用，并引用实际日志行为。 | `renderSafety()`（`SettingsPage.tsx:522`） | M1 | UI 不允许关闭脱敏；文案与 `redact_sensitive_text`（`agents.rs:3212`）一致。 |
| 5.4 | 删除 Allowed working directories 假 chip；若要保留，只显示当前执行 cwd 规则：项目根 + slot cwd。 | `renderSafety()` | M4 | 无 `+ Add` 假入口。 |

### M6 - About 真实化

**结果**: About 展示真实版本/后端状态和真实可打开资源。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|---|---|---|---|
| 6.1 | 前端调用 `health_check`，显示 `app`, `version`, `backend`, `timestampMs`，替代硬编码 `v0.4.0 build 2026.06.05`。 | `src-tauri/src/lib.rs:34`, `SettingsPage.tsx`, 新 hook 可选 | M1 | package/Cargo 版本变化后 About 自动一致。 |
| 6.2 | 资源链接使用 `@tauri-apps/plugin-opener` 已有能力打开真实 URL/本地文档；无法确定 URL 的项删除。 | `renderAbout()`, `src-tauri/src/lib.rs:47` | 6.1 | 点击真实链接成功；preview 中不崩。 |

### M7 - 测试、预览和视觉烟测更新

**结果**: 重构后的 Settings 有自动化覆盖，截图和 smoke 不再依赖假功能文案。

| # | 任务 | 文件 / 符号 | 依赖 | 验证 |
|---|---|---|---|---|
| 7.1 | 更新 preview fixture 和 settings tab 深链类型，删除 notifications，改新 Commands/About 文案。 | `src/preview/PlanningPreviewApp.tsx` | M1-M6 | `pnpm smoke` settings URL 全通过。 |
| 7.2 | 更新 `scripts/visual-smoke.sh` 的 Settings 断言文本。 | `scripts/visual-smoke.sh:76` | M1-M6 | `pnpm smoke:visual` 通过。 |
| 7.3 | 跑完整验证：前端单测、构建、Rust 测试、基础 smoke。 | `package.json`, `src-tauri/Cargo.toml` | 全部 | `pnpm test`; `pnpm build`; `cargo test --manifest-path src-tauri/Cargo.toml`; `pnpm smoke`。 |

## 具体任务清单

1. 先补测试锁 Agent 和 Theme 的既有真实行为。
2. 删除 Settings 中没有状态、没有 handler、没有后端消费的控件。
3. 新建最小 `AppSettings` 模型，只放本轮能被业务读取的字段。
4. 将 Theme 从 localStorage-only 收敛到 settings-backed；修复启动强制 dark。
5. 保留并清理 Agents 页，修正“Default”误导和 built-in 编辑入口。
6. Commands 页接入 `.loom/terminal-slots.json`，与 TestingPane 共用项目命令配置。
7. Safety 只实现命令确认/超时/脱敏状态，不展示没有消费链的策略矩阵。
8. About 改用真实 `health_check` 和真实资源链接。
9. 更新 preview、smoke、visual smoke 和相关文案断言。
10. 运行完整验证并在最终报告列出删除项、保留项、测试证据和剩余风险。

## 风险

| 风险 | 可能性 | 影响 | 缓解 |
|---|---:|---:|---|
| 删除假功能后视觉稿中的六页 Settings 覆盖不再匹配 | 高 | 中 | 同步 preview 和 visual smoke；把删除理由写进 PR/计划。 |
| Theme 从 localStorage 迁到 settings 破坏 preview forcedTheme | 中 | 中 | 保持 `forcedTheme` 优先级；无 Tauri runtime 时 fallback 到 localStorage。 |
| Commands 设置与 TestingPane slot 编辑出现双源状态 | 中 | 高 | 抽共享 helper；保存后以 `.loom/terminal-slots.json` 为唯一项目级来源。 |
| 安全策略如果只在前端确认，后端直接调用仍可能绕过 | 中 | 高 | 本轮至少在所有 UI 入口接入；后续可把策略检查下沉到 Tauri command。 |
| 命令超时涉及 long-running preview/PTY，错误终止会影响调试体验 | 中 | 中 | 区分 one-shot validation 与 preview；默认只对 validation 自动 kill。 |
| About 资源链接在 preview/browser 环境缺少 Tauri opener | 低 | 低 | hook 内做 runtime detection，preview 显示不可点击或普通链接。 |

## 待确认问题

- [ ] `confirmBeforeCommands` 是否只对危险命令生效，还是对所有手动运行命令生效？
- [ ] `commandTimeoutSeconds` 本轮是否覆盖 PTY preview，还是只覆盖 validation/one-shot 命令？
- [ ] 是否保留 `System` 主题选项？`ThemeContext` 当前是二值 `light | dark`，但**无显式偏好时已经跟随系统**（`ThemeContext.tsx:25`/`:41`）。若要显式 System 选项，需把 `themeMode` 设计为 `light | dark | system`，让"system"对应"清空持久化值、回落系统监听"。
- [ ] `safety` tab 的 Commands 区是否允许编辑 TestingPane 的全部 terminal slots，还是只提供跳转/只读入口，把编辑继续放在 Testing cockpit？（注意 SessionPane 也读取同一份 slot）
- [ ] About 的外部资源 URL 以 README/GitHub 为准，还是先只打开本地 `README.md` / `docs/requirements.md`？

## 验证策略

- **单元测试**: 覆盖 settings 默认值/持久化、Theme 模式、危险命令识别、terminal slot helper、Agent built-in 保护。
- **Rust 测试**: 覆盖 `settings.rs` 读写、`agents.rs` 既有 Agent 配置、`terminals.rs` slot 保存读取、必要时覆盖 command timeout 策略。
- **前端构建**: `pnpm test` 和 `pnpm build`，确保删除 tab/import 后没有类型与未使用问题。
- **桌面/预览 smoke**: `pnpm smoke` 覆盖 Settings 深链；必要时更新 `scripts/smoke-flow.sh` 的 tab 列表。
- **视觉 smoke**: `pnpm smoke:visual` 覆盖深浅主题下保留的 Settings 页面，断言新文案，不再断言删除的假功能。
- **手动验收**: 在 Tauri 预览中验证主题持久化、Agent 启用/禁用、自定义 Agent CRUD、项目命令保存后 TestingPane 读取、危险命令确认、About 真实版本。

## 回滚策略

- 信息架构删除是前端局部改动，可单独回滚 `SettingsPage.tsx` 和 smoke 文案。
- 新增 settings 后端以新文件和新 commands 为主，不迁移现有任务数据；若失败可移除 settings 调用并退回 localStorage 主题。
- Terminal slots 使用现有 `.loom/terminal-slots.json`，如 Settings 编辑出错，可保留 TestingPane 既有编辑路径作为应急入口。
