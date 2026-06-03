# Loom 桌面端功能 UI 拆解与重新设计

生成时间：2026-05-21

设计稿：`designs/loom-functional-redesign.pen`

## 设计判断

Loom 的界面不应该从“聊天窗口”或“普通 IDE 侧栏”出发，而应该从一个可恢复、可审计的本地开发闭环出发。用户最关心的问题是：

- 当前任务处于哪个阶段？
- 哪个 Agent 正在负责？其它 Agent 在 Review 什么？
- 系统依据什么判断可以继续、需要修复或已经完成？
- 命令、日志、错误、人工反馈和验证证据在哪里？

因此新的 UI 按功能域拆成 6 个画板，而不是沿用旧布局。

## 功能拆解

### 00 Function map

目标：给出产品级信息架构。

包含 6 个核心域：

- Project hub：打开项目、识别技术栈、Git 状态、脚本命令。
- Agent registry：配置 Codex、Claude Code、OpenClaw、Hermes 等 Agent，并展示能力、协议和权限。
- Planning room：多 Agent 并行计划、汇总共识、冲突、风险和待确认问题。
- Execution board：选择主 Agent、执行 todo、展示文件变更、组织 Review。
- Debug cockpit：运行调试命令、实时日志、错误摘要、人工反馈、自动修复循环。
- Summary archive：输出任务总结、验证证据、剩余风险和历史时间线。

### 01 Project and Agent Setup

目标：在进入任务前把本地项目与 Agent 能力准备好。

关键 UI：

- Project analyzer：展示技术栈、推荐命令、Git 状态和项目配置状态。
- Command presets：保存启动、测试、构建、Lint 等命令。
- Safety policy：展示高风险行为策略，例如删除文件、重置 Git、安装依赖、访问生产服务。
- Agent registry table：每个 Agent 展示能力标签、权限、适配器类型、可用性和编辑入口。

### 02 Multi-Agent Planning Room

目标：让多个 Agent 先讨论，不让实现直接开始。

关键 UI：

- Requirement composer：输入需求并选择参与讨论的 Agent。
- Agent plan comparison：把各 Agent 输出拆成共识、冲突、风险。
- Open questions：记录仍需用户确认的问题。
- Final implementation plan：确认最终计划并导出 Markdown。

### 03 Implementation and Review

目标：把实施拆成可 Review 的小任务。

关键 UI：

- Todo queue：展示计划拆出的实施项、状态和当前执行边界。
- Primary Agent run：展示主 Agent、运行日志和执行范围。
- Review results：按 Blocker、Suggestion、Accepted risk 分类展示协作 Agent 意见。
- 控制动作：Request review、Switch Agent、Apply fixes。

### 04 Debug Repair Cockpit

目标：调试验收时始终让证据和修复入口可见。

关键 UI：

- Run controls：一键运行推荐命令，显示长任务控制。
- Live log stream：展示 stdout/stderr、时间、错误行。
- Error intelligence：把日志解析成问题摘要和 repair package。
- Human feedback：用户随时补充复现步骤、截图说明、期望行为或日志片段。

### 05 Summary and History

目标：完成任务后留下可审计的交付记录。

关键 UI：

- Completion summary：完成内容、变更文件、验证命令、剩余风险。
- Timeline：按时间展示需求、计划、实施、Review、调试、修复、验收事件。
- Export report：导出任务总结，作为后续维护依据。

## 布局原则

- 桌面端主工作区以 1440×900 为基准。
- 左侧保留全局导航，但不让它承载业务细节。
- 业务画板使用任务状态和证据流组织，而不是聊天消息顺序。
- Review 和 Debug 不做隐藏 Tab，因为它们是任务是否能继续的判断依据。
- 命令执行、日志、Agent 输出、人工反馈都必须能回溯到任务记录。

## 视觉原则

- 使用中性工程工作台配色：浅米灰背景、深墨文字、teal 表示 active、green 表示通过、amber 表示风险、red 表示阻塞。
- 控件密度偏高，适合开发者长期使用。
- 卡片只用于独立信息块，避免大面积营销式 hero 或装饰性卡片。
- 日志、路径、命令使用等宽字体；任务、Review、摘要使用正文无衬线字体。

## 后续实现建议

1. 先实现 `Project and Agent Setup`，因为其它所有流程依赖项目与 Agent 能力识别。
2. 再实现 `Planning Room` 到 `Final Plan` 的闭环。
3. 然后接 `Implementation and Review`，确保每个 todo 都有执行边界和 Review 分类。
4. 最后强化 `Debug Repair Cockpit` 和 `Summary Archive`，形成完整验收闭环。
