# Loom 桌面端 UI 重设计：Command Center

生成时间：2026-05-21 11:20 CST

## 设计目标

Loom 的核心不是聊天，而是多 Agent 本地开发闭环。因此主界面应直接呈现“当前系统正在做什么、依据是什么、下一步由谁执行、如何验证”。本次重设计不沿用旧布局，按功能需求重新组织为三栏桌面工作台。

## 信息架构

### 1. 左栏：Mission setup

用途：定义任务边界、计划讨论、选择实施 Agent、推进 todo。

- 计划前：显示需求输入、多 Agent 选择、计划讨论流和最终计划确认。
- 计划后：切换为实施 todo 队列，明确每个 todo 的状态、执行 Agent 和完成入口。
- 设计原则：左栏只处理“要做什么”和“谁来做”，不混入日志和验收证据。

### 2. 中栏：Current operating state

用途：作为主判断面板，展示任务阶段、执行指标和 Agent 输出。

- 四阶段状态：Discuss + plan、Implement + review、Debug + repair、Verify + summarize。
- 遥测指标：Agent 成功调用数、todo 完成数、Review 风险、最近命令状态。
- 计划阶段显示 consensus brief 和最近 Agent signals。
- 实施阶段显示当前 todo brief、Review handoff checklist 和 Agent run 输出。
- 设计原则：中栏回答“现在在哪一步，为什么能继续或不能继续”。

### 3. 右栏：Evidence + repair

用途：承载命令、日志、错误摘要、人工反馈和修复 handoff。

- 支持运行命令、停止命令、过滤 stdout/stderr、搜索日志。
- 错误摘要和 repair context 与人工反馈放在同一栏，便于形成修复输入包。
- 设计原则：右栏回答“发生了什么，有哪些证据，如何进入下一轮修复”。

## 视觉策略

- 桌面端优先：稳定三栏，不做营销式首页。
- 使用低饱和中性工作台配色，减少纯蓝/深灰 IDE 感。
- 卡片仅用于阶段、指标、信号和 handoff 这类独立信息块，不把页面大区做成浮动卡片。
- 关键状态使用颜色区分：teal 表示 active，green 表示 success，amber 表示 attention，red 表示 blocked/error。
- 日志和命令继续使用等宽字体，流程和说明使用 sans 字体。

## 可落地组件

- `CommandOverview`：任务名、状态 pill、四阶段 card、遥测指标。
- `DecisionLedger`：计划汇总、计划文档路径、Agent 输出信号。
- `Mission setup column`：复用现有 planning / implementation 控件。
- `Evidence column`：复用现有 debug / feedback 控件。

## 后续 Stitch / Pencil 复刻要点

当前会话未暴露 Stitch MCP，Pencil MCP 也无法连接运行中的编辑器，因此本次落地为 React UI + 设计说明。后续在 Stitch 或 Pencil 里复刻时，建议直接按以下画板拆分：

1. `Command Center / Planning`
   - 左栏：需求输入 + Agent chips + plan discussion。
   - 中栏：四阶段状态 + consensus brief + Agent signals。
   - 右栏：disabled debug panel + evidence empty state。

2. `Command Center / Implementation`
   - 左栏：todo queue + primary Agent selector。
   - 中栏：current todo + review checklist + Agent run。
   - 右栏：live logs + error summary + manual feedback。

3. `Command Center / Blocked Repair`
   - 中栏状态 pill 为 `blocked`。
   - 右栏显示 error summary、repair context preview、人工反馈输入。

## 验收关注点

- 没有项目时应显示明确的项目启动空状态。
- 有项目但没有任务时，左栏可直接创建需求，右栏命令控件保持禁用。
- 任务进入实施后，左栏从计划讨论自然切换为 todo 执行队列。
- 调试、修复、验证相关证据始终可见，不需要用户切换页面寻找日志。
