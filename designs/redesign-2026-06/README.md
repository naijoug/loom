# Loom — UI 重构设计（2026-06，v3 · 项目→任务工作流，暗黑/明亮双主题）

Loom 是「多 Agent 本地开发工作台」。v3 把产品从固定的四阶段向导，改成更贴近真实开发的
**项目 → 任务 → 按状态分化的工作区**模型，并采用参考图（Claude 桌面端）实测的精确色值与字号。

## 信息架构

- **左侧 = 项目列表**（Linear/Codex 风格）：搜索下方是 **＋ Add project**（打开本地目录新建项目，位于项目列表上方）；项目展开为任务，任务行带 Linear 状态图标；**每个项目 hover 时右侧显示 ＋**（在该项目下新建任务）；底部固定 Settings。
- **任务以「项目」为维度创建**：没有全局 New task。点项目 hover 出的 **＋**（或看板 **＋ New task**）打开新建任务弹窗——**不含项目下拉框**，项目由所点的 ＋ 决定（标题旁显示 “in <项目>”）→ 填标题/描述/邀请 Agent，弹窗还会给出 **「Suggested primary」推荐主 Agent**（如 Claude Code · strongest for Implement）→ Start discussion 进入计划聊天室。
- **每个项目有自己的看板**：点侧栏项目名打开该项目的 Board（`b-<pid>`），标题/任务/计数随项目切换（loom / speaker），侧栏任务列表同步高亮当前项目。
- **① 计划 = 聊天室**（Codex app 风格）：单条讨论流，可 `@` 邀请多个 Agent 参与；底部一个大输入框；
  右栏汇总「共识 / 冲突 / 风险」并提供 **Create tasks from plan**（讨论完成 → 生成任务）。
- **② 任务看板 = Linear 列表**：按状态分组（In Progress / Todo / Testing / Blocked / Done），
  含任务 ID、优先级、标签、执行 Agent，Todo 行有 **▶ Run** 手动触发执行。
- **③ 任务详情按状态分化：**
  - **In Progress** → Codex/Claude-Code 式 **Agent session**：可见的 agent loop（thinking → Read/Edit/Run 工具调用 → diff），底部 steer 输入框。
  - **Testing** → 按应用类型启动的**多终端调试**：全栈/Tauri 应用同时开 **Frontend** + **Backend** 两个终端实时输出日志；
    **Debug agent** 观察日志 → 摘要报错 → 提出修复 diff → 应用并重启 → 再次测试（test-cycle 循环）。
  - **Done** → 交付总结：完成项、变更文件、验证证据、时间线。
- **Settings**（六页均已设计内容）：**General**（工作区/启动/并发/语言/隐私）、**Appearance**（主题/取色/字体/字号/密度）、
  **Agents**（本地可用 Agent 配置）、**Commands & Safety**（命令预设 + 高风险策略 + 执行护栏）、**Notifications**、**About**。从侧边栏底部进入。

## 界面切换逻辑（见 `shots/00-navigation-flow.*`）

**核心原则：一个 Task 同时只有一个状态；状态决定打开哪个详情视图。Task Board 是路由中枢。**

向前推进（每屏主操作）：
`New task → ① Plan` →（**Create tasks from plan**）→ `② Task Board` →（**▶ Run** 某 Todo，状态 Todo→In Progress）→
`③ In Progress`（agent session）→（**Mark ready for testing**，状态→Testing）→ `④ Testing` →（全部通过，状态→Done）→
`⑤ Done` →（**Start follow-up**）→ 新的 ① Plan。Testing 严重失败 → Blocked（回 Board）。

随处跳转：
- **左侧项目列表**：点任务 → 按其状态打开对应详情；点项目名 → 该项目 Board。
- **面包屑 mini-stepper**（Plan ✓ · Implement ● · Test ○ · Done ○）：在同一任务的阶段间回看。
- **Board = 状态编辑器**：改任务状态即决定它打开哪个详情视图。
- Plan / In Progress / Testing 是同一任务的三种 session；Testing 内部还有 fix→restart→re-test 的 debug 循环（不切换主界面）。

## 目录结构

- `html/app.css` — 设计 token + 组件样式（双主题 CSS 变量，权威来源，可直接搬进 Tauri/React）。
- `html/loom.html` — **可交互点击原型**：在浏览器打开后，侧栏任务/项目、面包屑、主操作按钮（▶ Run / Create tasks / Mark ready / Apply fix / Start follow-up）和主题切换都能真实跳转，可走通完整闭环。也支持直接按 `#<screen>/<theme>` 深链。
- `shots/` — 30 张渲染图（15 屏 × dark/light，2880×1880，headless Chrome @2x）。
- `stitch-v1/` — 最初的 Stitch 探索稿（teal、单主题、旧 IA），仅存档。

### 渲染图清单（`shots/`，每屏 `.dark.png` / `.light.png`）

| # | 屏幕 | 前缀 |
|---|---|---|
| ⓪ | 界面切换逻辑流程图（导航地图） | `00-navigation-flow` |
| ① | 计划聊天室 | `01-planning-chat` |
| ② | 任务看板（Linear） | `02-task-board` |
| ③ | 新建任务弹窗（项目内） | `03-new-task` |
| ④ | 添加项目弹窗（打开本地目录） | `04-add-project` |
| ⑤ | 任务详情 · 执行中（Agent session） | `05-task-inprogress` |
| ⑥ | 任务详情 · 测试（多终端 + Debug agent） | `06-task-testing` |
| ⑦ | 任务详情 · 完成（总结） | `07-task-done` |
| ⑧ | 设置 · General | `08-settings-general` |
| ⑨ | 设置 · Appearance | `09-settings-appearance` |
| ⑩ | 设置 · Agents | `10-settings-agents` |
| ⑪ | 设置 · Commands & Safety | `11-settings-commands-safety` |
| ⑫ | 设置 · Notifications | `12-settings-notifications` |
| ⑬ | 设置 · About | `13-settings-about` |
| ⑭ | 任务看板 · speaker 项目 | `14-board-speaker` |

> 路由 key：`planning · board · new-task · add-project · task-prog · task-test · task-done · settings · settings-appearance · settings-agents · settings-safety · settings-notif · settings-about`，以及每个项目 `b-<pid>` / `nt-<pid>`（`pid ∈ loom·speaker`）（深链 `#<key>/<theme>`）。

## 设计 Token（实测自参考图）

| Token | Dark | Light | 用途 |
|---|---|---|---|
| `--bg` | `#181818` | `#FFFFFF` | 应用 / 主内容 |
| `--sidebar` | `#272727` | `#D7D7D7` | 项目列表侧栏 |
| `--surface` | `#222222` | `#FFFFFF` | 卡片 |
| `--inset` | `#303030` | `#EFEFEF` | 终端 / 日志 / 代码 |
| `--selected` | `#3B3D41` | `#CECFD1` | 选中项 |
| `--border` | `#2F2F2F` | `#D4D4D5` | 边框 / 分隔线 |
| `--text` / `--text-2` | `#ECECEC` / `#A3A4A5` | `#1A1A1A` / `#6B6B6D` | 主 / 次文字 |
| `--accent` | `#97C9F8` | `#3485D1` | 链接 / active / 主操作 |
| 状态 | green/amber/red | green/amber/red | 通过 / 风险·测试 / 阻塞·错误 |

字号：标题14 · 正文14 · 表格/按钮12.5 · 标签11(caps) · 等宽日志13。
Agent 头像配色：Codex `#10A37F` · Claude `#D97757` · Hermes `#7C5CFC`。
切主题：`<html data-theme="dark|light">`。

## 复现 / 重新渲染

```bash
cd designs/redesign-2026-06/html
CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
"$CHROME" --headless=new --hide-scrollbars --force-device-scale-factor=2 \
  --window-size=1440,940 --screenshot=out.png \
  "file://$PWD/loom.html#task-test/dark"
# screen ∈ planning | board | task-prog | task-test | task-done | settings | settings-agents
# theme  ∈ dark | light
```
