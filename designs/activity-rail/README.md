# Activity Rail — 布局稿（P5）

来源：[Agent 调用层与左侧功能栏计划](../../docs/plans/2026-10-26/22:42-agent-layer-and-activity-rail.md) §5.2、P5。
状态：按计划默认决策实施（用户指示"按默认决策执行"，未单独确认本稿）；截图由 `pnpm smoke:visual` 生成到 `designs/exports/activity-rail/`（不入库）。

## 整体结构

```text
┌──────┬────────────────────┬──────────────────────────────────────┐
│ rail │ 第二栏（功能自有）    │ 主区域（功能自有）                      │
│ 52px │ Chat: 项目/会话树     │ Chat: 转录 + Composer                 │
│      │ Learn: 研究列表       │ Learn: 新研究表单 / Wiki               │
│ 💬   │ Settings: 分区导航    │ Settings: 分区内容                     │
│ 🎓   │                    │                                      │
│      │                    │                                      │
│ ⚙    │                    │                                      │
└──────┴────────────────────┴──────────────────────────────────────┘
```

- 功能栏（`src/shell/ActivityRail.tsx`）只负责切换功能；第二栏和主区域完全由各功能渲染，功能之间不互相引用。
- 顶部：注册表里的功能，按注册顺序（Chat、Learn；Research、Discuss 交付时再加入，不放占位）。底部：Settings。
- 启动总是打开 Chat，不记忆上次视图（计划 §8-5）。

## 功能栏

| 项 | 规格 |
|---|---|
| 宽度 | 52px，`flex: 0 0 52px`，窄窗口也保留 |
| 背景 / 分隔 | `--chat-sidebar`，右侧 1px `--chat-border` |
| 按钮 | 36×36，圆角 9px，图标 18px（lucide，stroke 1.75） |
| 默认 / 悬停 | 图标 `--chat-muted`；悬停背景 `--chat-selected`，图标 `--chat-text` |
| 当前项 | 背景 `--chat-selected`，图标 `--chat-accent`，左侧 3×18 的 `--chat-accent` 竖条；`aria-current="page"` |
| 提示 | `title` 与 `aria-label`："Chat (⌘1)" / "Chat（⌘1）" 形式，快捷键写在提示里 |
| macOS 标题栏 | Tauri `titleBarStyle: Overlay`：功能栏顶部留 38px 拖拽区避开红绿灯；其它平台留 8px |
| 焦点 | `:focus-visible` 2px `--chat-accent` 外框 |

## 快捷键

- `⌘1…⌘4`（其它平台 `Ctrl`）：按注册顺序切换功能；超出已注册数量时忽略。
- `⌘,`：打开 Settings。
- 输入法组合输入中（`isComposing`）不响应；快捷键在任何视图都生效。

## 各功能在外壳中的变化

- **Chat**：侧栏底部移除 Learn 与 Settings 按钮，只保留"归档"切换；项目 / 会话树和折叠按钮不变。保持常驻挂载，切换功能不打断回合、草稿与选择。
- **Learn**：移除"返回 Chat"按钮，侧栏标题行留空作拖拽区；新研究表单的 Agent 选择改用共享 `AgentPicker`（菜单向下展开），模型输入仅在 Agent 诊断报告 `modelSelection` 时显示。首次访问后保持挂载。
- **Settings**：移除"返回"按钮；由功能栏底部进入，再点 Chat / Learn 离开。仅打开时挂载（无需保留的界面状态）。

## 窄窗口（< 760px）

- 功能栏保持 52px；Chat 侧栏按现有规则默认折叠、展开时覆盖在主区域上（相对 `.app-shell-content` 定位，不盖住功能栏）。窗口变宽时已折叠的侧栏不会自动展开。
- Learn 侧栏按现有规则堆叠到主区域上方；Settings 分区导航按现有规则收窄。
- 不出现横向滚动：`smoke:visual` 在 700px 宽检查 `scrollWidth <= clientWidth`。

## 深浅色

只用 `--chat-*` 令牌，深浅色由 `styles/theme.css` 切换；`smoke:visual` 矩阵覆盖 zh/en × dark/light × 700px。
