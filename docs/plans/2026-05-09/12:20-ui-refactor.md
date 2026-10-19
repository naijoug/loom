# UI 重构实施计划

- **日期**：2026-05-09
- **作者**：Gemini CLI
- **状态**：可执行 (Ready)
- **关联设计**：`designs/loom.pen` (Option 4: Themed Hybrid)

## 目标

基于最新确定的 Option 4 (Themed Hybrid) 设计方案，对 Loom MVP 的前端 UI 进行全面重构。
实现一套结构清晰的左右分栏界面，支持浅色（Light）与深色（Dark）主题无缝切换，并搭建出高保真的静态占位视图（Placeholder Views），为后续接入 Tauri 后端实际的 Agent 逻辑和日志流打好前端结构基础。

## 非目标

- 本计划**不包含**实现与 Rust/Tauri 后端的真实数据交互逻辑（如真实的终端日志流解析、真实的 Agent 调用），重点在于**视图层与布局**的构建。
- 本计划暂不实现复杂的动画过渡效果。
- 本计划不引入复杂的全局状态管理库（如 Redux），仅使用 React Context 或简单的 useState 进行视图状态和主题切换的管理。

## 成功标准

- [ ] 完成整体应用的基础布局结构：固定左侧边栏（Sidebar） + 右侧主工作区（Main）。
- [ ] 完成深浅模式（Light/Dark Mode）的主题变量定义及切换机制，且 UI 组件严格使用语义化颜色变量（CSS Variables 或 CSS-in-JS）。
- [ ] 按照设计稿完成侧边栏（项目列表、Agent 列表）的静态组件开发。
- [ ] 按照设计稿完成顶部任务头部（项目路径、任务执行步骤条、操作按钮）的组件开发。
- [ ] 按照设计稿完成右侧主工作区的左右切分布局：
  - 左半区：当前计划卡片（Current Plan）、Agent 消息输出区。
  - 右半区：深色调试终端（Debug/Logs）、手动反馈输入表单。
- [ ] 所有组件在亮/暗模式下均无视觉 Bug。
- [ ] 在基准视口 1200×800 下完整还原设计稿，最小宽度 1024 不出现横向滚动。
- [ ] 保留现有 Tauri bridge 能力：健康检查、spike 进程启动/停止、实时日志事件接收逻辑不得因 UI 重构回退。
- [ ] 通过 `pnpm build` 与 `cargo check`（在 `src-tauri/`）；可选 `pnpm tauri build` 冒烟。

## 当前状态

- 核心产品流程和状态机定义已经在 `docs/requirements.md` 和 MVP 计划中明确。
- 基础的 React + Vite + Tauri v2 框架已搭建完毕（见 `package.json`、`src-tauri/`）。
- 全新的设计原稿已就绪（`designs/loom.pen`，包含 Light/Dark 双主题与 9 个 token）。
- 当前 `src/App.tsx`（~234 行）与 `src/App.css`（~507 行）已包含 M1 spike UI：Tauri `health_check` 调用、spike runner 启停、`loom://spike-log` 实时日志监听和基础工作台布局。UI 视觉层需要重构为设计稿结构，但 Tauri bridge、日志流状态逻辑与 `src/domain/` 领域模型需要保留并迁移。
- 依赖现状：仅有 `react`、`react-dom`、`@tauri-apps/*`，**未安装** `lucide-react`。本计划批准在 M2.3 引入 `lucide-react`，理由：设计稿明确使用 Lucide 图标（`hexagon`、`folder`、`bot`、`check`、`loader`、`circle`、`chevron-right`、`send`），自绘 8+ 个 SVG 会增加维护成本且偏离前端设计约定。

## 里程碑

### M1: 基础设施与主题系统搭建
构建基础的 CSS 变量系统以支撑深浅模式的切换，搭建页面的顶层骨架。

| # | 任务 | 涉及文件 | 验证 |
|---|---|---|---|
| 1.0 | 梳理并迁移现有 `App.tsx` / `App.css`：把健康检查、spike runner 启停、实时日志监听等 Tauri bridge 逻辑从旧 UI 中抽离到新组件结构；删除旧视觉布局和样式。 | `src/App.tsx`, `src/App.css`, 必要时 `src/hooks/*` 或 `src/components/Workspace/*` | `pnpm dev` 启动后进入新 App 骨架；后端探针与 spike 日志流逻辑仍可被后续组件接入，无旧 UI 残留。 |
| 1.1 | 建立浅色/深色主题 CSS 变量，**完整覆盖设计稿 9 个 token**：`--theme-bg-primary`, `--theme-bg-secondary`, `--theme-text-primary`, `--theme-text-secondary`, `--theme-text-muted`, `--theme-accent`, `--theme-border-subtle`, `--theme-border-strong`, `--theme-error`（取值见下方"主题 Token 表"）。 | `src/styles/theme.css` | 浏览器 DevTools `:root` 与 `[data-theme="dark"]` 下 9 个变量均存在且取值正确。 |
| 1.2 | 接入字体：`Geist`（标题/品牌 14/16/18）、`Inter`（正文/标签 11/12/13）、`Geist Mono`（终端 13）。优先使用 `@fontsource/*` 本地包；如不便引入新依赖则改用 system fallback (`ui-sans-serif`, `ui-monospace`)。 | `src/styles/theme.css`, 必要时 `package.json` | 渲染后 `computed font-family` 命中目标字体，终端使用等宽字体。 |
| 1.3 | 实现全局主题切换 Context 及 Toggle 按钮（写 `data-theme` 到 `<html>`，并持久化到 `localStorage`）。 | `src/contexts/ThemeContext.tsx`, `src/App.tsx` | 切换后所有色块同步翻转，刷新后保留选择。 |
| 1.4 | 构建 App 主布局（Sidebar 240 固定 + Main flex）。 | `src/layouts/AppLayout.tsx`, `src/App.tsx` | 1200×800 视口下完美还原；缩到 1024 仍无横向滚动。 |

#### 主题 Token 表（设计稿权威值）

| Token | Light | Dark |
|---|---|---|
| `--theme-bg-primary` | `#FFFFFF` | `#0A0A0A` |
| `--theme-bg-secondary` | `#F7F8FA` | `#141415` |
| `--theme-text-primary` | `#1A1A1A` | `#FFFFFF` |
| `--theme-text-secondary` | `#666666` | `#A1A1AA` |
| `--theme-text-muted` | `#888888` | `#71717A` |
| `--theme-accent` | `#A855F7` | `#A855F7` |
| `--theme-border-subtle` | `#EEF0F2` | `#27272A` |
| `--theme-border-strong` | `#1A1A1A` | `#3F3F46` |
| `--theme-error` | `#E11D48` | `#F43F5E` |

### M2: 左侧边栏 (Sidebar) 实现
完成左侧全局导航区域。

| # | 任务 | 涉及文件 | 验证 |
|---|---|---|---|
| 2.1 | 实现品牌 Logo 与标题区。 | `src/components/Sidebar/Brand.tsx` | 样式对齐设计稿。 |
| 2.2 | 实现项目列表 (Projects) 和 Agent 列表视图。 | `src/components/Sidebar/Navigation.tsx` | 选中态（如高亮项）与未选中态区分明显，支持深浅模式。 |
| 2.3 | 集成 Lucide React 图标库以满足设计稿所需 Icon。 | `package.json`, 涉及组件 | 图标正常显示。 |

### M3: 主工作区顶部 (Main Header) 实现
包含面包屑、任务进度条及关键操作按钮。

| # | 任务 | 涉及文件 | 验证 |
|---|---|---|---|
| 3.1 | 实现 Header 布局与面包屑文字。 | `src/components/Header/Header.tsx` | 布局对齐。 |
| 3.2 | 实现任务步骤流组件 (Planning -> Implementing -> Debugging)，节点之间使用 `chevron-right` 分隔。Step 接受 `status: 'done' \| 'active' \| 'pending'`，分别对应 `check`(muted)、`loader`(accent)、`circle`(muted) 三种 Icon + 文案颜色。 | `src/components/Header/TaskFlow.tsx`, `src/components/Header/TaskFlowStep.tsx` | 三种 status 各自截图对照设计稿无差异。 |
| 3.3 | 实现通用 `Button` 组件，至少支持 `variant: 'primary' \| 'danger' \| 'ghost'` 三种语义，并以 "Stop Task"（danger/primary 待最终确认）和 Submit（primary + send 图标）作为两个落地用例。 | `src/components/common/Button.tsx` | 三种 variant 在亮/暗模式下颜色、hover、focus-visible 状态符合主题；按钮可承载左侧/右侧 Icon。 |

### M4: 实施与调试视图 (Implementation & Debug Split Pane) 实现
应用核心交互区域，分为左右两个子面板。

| # | 任务 | 涉及文件 | 验证 |
|---|---|---|---|
| 4.1 | 搭建主内容区的左右 Split 布局。 | `src/components/Workspace/WorkspaceSplit.tsx` | 左右分栏正常显示，边界线清晰。 |
| 4.2 | 实现左半区：IMPLEMENTATION (Current Plan, Agent Message)。 | `src/components/Workspace/ImplementationPane.tsx` | 卡片样式、阴影/边框与设计稿匹配。 |
| 4.3 | 实现右半区：DEBUG / LOGS (Terminal, Manual Feedback)。 | `src/components/Workspace/DebugPane.tsx` | 终端底色跟随主题切换（使用 `theme-bg-secondary`），反馈框支持文本输入。 |

## 风险

| 风险 | 可能性 | 影响 | 缓解措施 |
|---|---|---|---|
| 主题变量覆盖不全导致部分组件在切换模式时不可见 | 中 | 中 | 在 M1 阶段严格规范 CSS Variable 命名表，避免组件内部写死十六进制颜色。 |
| Lucide React 与设计稿 Icon 存在偏差 | 低 | 低 | 采用相近语义的 Icon 替代，不阻塞开发。 |
| 终端视图及文本在特定主题下对比度不足 | 中 | 低 | 确保使用设计稿中定义的 `theme-bg-secondary` 与 `theme-text-primary` 搭配，必要时进行对比度微调。 |
| UI 重构误删现有 Tauri spike 能力 | 中 | 中 | M1.0 先抽离/迁移 bridge 逻辑，再替换视觉层；最终验证健康检查、进程启停和日志事件仍可用。 |

## 待确认问题

- 对于左右 Split 布局，MVP 阶段是否需要支持用户拖拽改变左右宽度？（建议：MVP 优先写死固定比例或响应式 flex 比例，后续再添加可拖拽 `Resizer`）。

## 验证策略

- **视觉回归**：将开发好的组件截图或并排对比 `designs/loom.pen`，确保 Paddings、Gaps、Colors 的严格还原；基准视口固定 1200×800。
- **状态测试**：通过 React DevTools 手动更改主题 Context 的值，确保全局所有界面组件在 `light` 和 `dark` 之间过渡自然，没有遗漏的硬编码颜色；用 `grep -rE "#[0-9A-Fa-f]{3,8}" src/components` 抽检无散落色值。
- **响应式测试**：在 1024 / 1200 / 1440 三档宽度下，确保分栏布局使用 flex 或 grid 正常收缩，不会出现严重的文字溢出或横向滚动。
- **Tauri bridge 回归**：确认健康检查按钮仍能调用 `health_check`；spike run 能启动/停止；`loom://spike-log` 事件能实时追加到 Debug/Logs 面板。
- **构建门禁（AGENTS.md 要求）**：
  - 前端：`pnpm build`（含 `tsc` 类型检查）。
  - Rust：在 `src-tauri/` 执行 `cargo check`。
  - 端到端：`pnpm tauri build` 至少执行一次冒烟，确认 Tauri 壳能加载新 UI。
