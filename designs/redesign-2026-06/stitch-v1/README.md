# Loom — 全面 UI 重构设计（2026-06）

生成时间：2026-06-04 · 工具：Claude + Stitch（Gemini 3.1 Pro 渲染）

本目录是基于 `docs/requirements.md` 对 Loom 的一次**全面重新设计**，不沿用旧 UI。
设计把产品定位为「多 Agent 本地开发指挥中心（Command Center）」，而不是聊天壳：
每一屏都回答 — **当前在哪个阶段 / 谁在执行 / 证据是什么 / 下一步动作是什么**。

所有图片为 2560×2048 高清 PNG，可直接用于实现参考、评审或切图。

## 设计系统（Design System）

| Token | 值 | 用途 |
|---|---|---|
| 主色 Teal | `#0D9488` | active / 主操作 / 当前阶段 |
| 成功 Green | `#16A34A` | 通过 / 已采纳 |
| 警示 Amber | `#D97706` | 风险 / 需确认 |
| 阻塞 Red | `#DC2626` | 失败 / 错误 / blocked |
| 背景 | 暖中性 off-white `#F7F6F3 / #FAF9F7` | 工作台底色，长时间使用护眼 |
| 文字 | 墨黑 `#1A1C1B` | 高可读 |
| 边框 | 发丝灰 `#E5E3DD` | 结构分隔，无重阴影 |
| 标题字体 | Space Grotesk / Geist | 技术感无衬线 |
| 正文字体 | Inter / Hanken Grotesk | 高密度可读 |
| 等宽字体 | JetBrains Mono | 日志 / 命令 / 路径 / 版本 / 时间戳 |
| 圆角 | 4–8px | 工程化、克制 |

设计原则：桌面优先（1440×900 基准）、高信息密度、卡片只用于独立信息块、
颜色只用于表达状态、命令与日志一律等宽字体、无营销式 hero。

## 六屏（对应需求信息架构）

| 文件 | 屏幕 | 对应需求 |
|---|---|---|
| `01-project-hub.png` | **Project Hub** | §6.2 项目管理：打开本地项目、技术栈识别、Git 状态、命令预设、安全策略（Ask/Allow/Block）、各阶段默认 Agent |
| `02-agent-registry.png` | **Agent Registry** | §6.1 Agent 管理：能力标签、适配器类型(CLI/JSON/MCP/Plugin)、文件写/命令执行权限、可用性检测、各阶段默认 Agent、Test connection |
| `03-planning-room.png` | **Planning Room** | §5.1 / §6.4 多 Agent 讨论：需求 composer、参与 Agent 选择、计划对比（共识 / 冲突 / 风险）、待确认问题、最终计划文档 |
| `04-execution-board.png` | **Execution Board** | §5.2 / §6.5 实施与 Review：todo 队列、主 Agent 实时输出、文件变更 diff、Review 分类（Blocker / Suggestion / Accepted risk） |
| `05-debug-cockpit.png` | **Debug & Repair Cockpit** | §5.3 / §6.6 / §6.7 / §6.8 调试验收：命令运行控制、实时日志流、错误摘要 → repair package、自动/人工修复、人工反馈 |
| `06-summary.png` | **Summary & History** | §5.4 / §6.3 修复验收循环 + 任务总结：交付内容、变更文件、验证证据、关键决策、剩余风险、全过程时间线 |

## 全局结构（每屏复用）

- **左侧导航栏（~72px）**：Loom logo + 6 个功能区图标（Project Hub / Agent Registry / Planning Room / Execution Board / Debug Cockpit / Summary）+ 底部设置。当前项 teal 左侧高亮。
- **顶部上下文栏**：当前项目名 + 等宽路径 · 四阶段 stepper（Discuss & Plan → Implement & Review → Debug & Verify → Fix loop）· 实时系统状态 pill · New Task。
- **主工作区**：多栏工作台（多为三栏），按「任务状态 + 证据流」组织，而非聊天消息流。

## Stitch 项目（可在线继续编辑 / 重新生成）

| 屏幕 | Project ID |
|---|---|
| 01 Project Hub | `13596400004914346865` |
| 02 Agent Registry | `17903477590937026530` |
| 03 Planning Room | `10461540817525728797` |
| 04 Execution Board | `7363527228236456146` |
| 05 Debug Cockpit | `9414377469360491296` |
| 06 Summary | `16019751654414896903` |

共享设计系统资产：`assets/6113542635377627623`（Loom Engineering Workbench）。

## 实现建议顺序

1. 先落地全局外壳（左栏 + 顶部阶段 stepper + 系统状态），它是所有屏复用的骨架。
2. `Project Hub` + `Agent Registry`：所有流程依赖项目与 Agent 能力识别。
3. `Planning Room` → 最终计划闭环。
4. `Execution Board`：保证每个 todo 有执行边界与 Review 分类。
5. `Debug Cockpit` + `Summary`：形成完整「调试 → 修复 → 验收」闭环。
