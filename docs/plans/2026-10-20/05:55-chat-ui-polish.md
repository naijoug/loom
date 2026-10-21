# Chat UI 视觉与交互打磨计划

- 状态：代码完成；合成网页已验证；桌面已由用户确认两处提示条；已提交
- 基线：`codex/simple-cli-chat` @ `0acbb10`
- 设计依据：Claude Design 画布（Main / Composer / Empty / Settings 四块面板），令牌转写到 `src/styles/theme.css`
- 范围：只改前端展示与交互（`src/features/chat`、`src/features/settings`、`src/styles`、`src/i18n`），不改 Rust、存储格式和适配器协议

## 1. 背景

简化 Chat 功能已经可用，但界面是第一版骨架：侧栏、模式标记、工具卡片都用文本字符（`⌕`、`‹›`、`≡`）当图标；模式只靠文字区分；Auto-run 和切换 Agent 用 `window.confirm`，这个弹窗在 Tauri WKWebView 里能不能弹出并没有验证过（见验收记录“已知限制”）；设置页是平铺的表格和按钮。

## 2. 目标

1. 视觉层级：侧栏、转录、Composer、设置页在深色和浅色主题下都有清晰的层级，图标统一使用已有依赖 `lucide-react`。
2. 模式可辨识：Research（青色）和 Code（紫色）在侧栏图标、模式切换、新会话卡片、Composer 边框和提示中一致着色。
3. 应用内确认：Auto-run 首次开启和切换 Agent 的确认改成 Composer 上方的应用内提示条，不再依赖 `window.confirm`。
4. 信息更完整：
   - 新会话页显示项目路径。
   - 斜杠菜单显示命令说明和键盘提示。
   - Agent 菜单显示选中标记和“切换会清除原生续聊”的说明。
   - 工具卡片显示状态图标、目标、diff 统计和退出码。
5. 设置页：分区卡片；Agent 表格带状态点和等宽路径；语言使用单选卡片。

非目标：不新增依赖，不改动 Agent 能力和 CLI 参数，不改变已有的数据和偏好键。

## 3. 实施项

| 编号 | 内容 | 文件 |
|---|---|---|
| U1 | 令牌：深浅两套 `--chat-*`，新增模式色、提示条色、发送按钮色、圆角和阴影 | `src/styles/theme.css` |
| U2 | 侧栏：Logo、Projects 标题、chevron 图标、会话缩进线、模式图标、运行中指示、设置入口图标 | `ProjectTree.tsx`、`chat.css` |
| U3 | 会话头与转录：面包屑、图标化模式切换和导出、用户气泡、工具卡片的状态图标、等宽目标、diff 行着色 | `ChatSessionHeader.tsx`、`ModeSwitch.tsx`、`ChatTranscript.tsx`、`ToolCallCard.tsx` |
| U4 | Composer：模式着色边框、Agent 按钮（状态点和 chevron）、只读或 Auto-run 提示、浅色圆形发送按钮；斜杠菜单增加说明和页脚；Agent 菜单增加勾选和页脚；用应用内提示条替换两处 `window.confirm` | `ChatComposer.tsx`、`SlashMenu.tsx`、`AgentPicker.tsx`、`ChatPage.tsx` |
| U5 | 新会话页：项目路径、标题“New session in {project}”、带图标的模式卡片 | `EmptySession.tsx` |
| U6 | 设置页：返回入口、导航图标、分区卡片、Agent 表格、语言单选卡片 | `SettingsPage.tsx`、`settings.css` |
| U7 | 文案：新增键同时写入 `en.ts` 和 `zh-CN.ts` | `src/i18n/*` |

## 4. 验证

- 合成网页：`scripts/debug.sh stop` → `scripts/debug.sh web` → 打开 `?fixture=simple-chat`，分别截图检查深色、浅色、窄屏（小于 760px）、新会话页和设置页，结束后执行 `scripts/debug.sh stop`。
- `pnpm smoke:visual`：既有断言保持通过，截图更新到 `designs/exports/simple-chat/`。
- `pnpm test`、`pnpm build`。
- 桌面验收：不在本计划内，仍按验收记录的“下一步”执行；应用内提示条替换 `window.confirm` 后，桌面验收需补看这两处确认。

## 5. 实施记录

| 编号 | 状态 | 说明 |
|---|---|---|
| U1 | 完成 | `theme.css` 深浅两套 `--chat-*`，新增 raised/input/selected、模式色、提示条色、发送按钮色、阴影 |
| U2 | 完成 | `icons.tsx`（`LoomMark`、`ModeIcon`），侧栏搜索框、项目行悬停新建、会话缩进线和模式着色图标、运行中旋转图标、空项目提示 |
| U3 | 完成 | 会话头单行面包屑，模式切换着色，导出和更多操作改为图标按钮，菜单点击外部关闭；同一回合的工具卡片合并为一个列表，执行日志入口每回合只渲染一次（修复验收记录里的重复入口） |
| U4 | 完成 | `ConfirmBanner.tsx` 统一 Code 回合确认、Auto-run 首次确认和切换 Agent 确认；`ChatPage` 持有确认状态，切换会话或开始运行时清除；源码中 chat 部分已无 `window.confirm` |
| U5 | 完成 | 新会话页显示项目路径、“New chat in {project}”和带图标的模式卡片；无项目页改为品牌标记加主按钮 |
| U6 | 完成 | 设置页导航图标、页头说明与操作、带状态点的 Agent 表格、启用开关、工作模式空状态、语言和主题单选卡片 |
| U7 | 完成 | 新增文案同时写入 `en.ts` 和 `zh-CN.ts`；更新 `autoWarning`、`agentSwitchWarning` 措辞 |

验证（2026-10-20）：

- 内置浏览器手动检查 `?fixture=simple-chat`：深色/浅色会话页、新会话页、工具卡片展开、Auto-run 提示条（开启）、`/agent` 切换提示条（Esc 取消）、斜杠菜单、Agent 菜单、设置页 Agents/外观；700px 宽度下无横向溢出。
- `pnpm smoke:visual` 通过（`kind: synthetic-web-only`），截图已更新到 `designs/exports/simple-chat/`。
- `pnpm test` 82/82、`pnpm build`、`pnpm docs:release:check`/`docs:release:test` 通过。本轮未改 Rust，未重跑 cargo、clippy 和 tauri build。
- 桌面（2026-10-20，用户手动）：`scripts/debug.sh desktop` 启动 Tauri 开发版，用户确认 Auto-run 首次确认和切换 Agent 两个应用内提示条在 WKWebView 中正常显示和生效；结束后执行 `scripts/debug.sh stop`。其余界面的桌面检查未单独记录。
- 测试改动：`tests/unit/chatExport.test.cjs` 按 `aria-label` 定位导出按钮（导出按钮改为图标按钮）。

未完成与限制：

- 桌面只确认了两处提示条，简化 Chat 的整体桌面验收（验收记录 P9）仍未执行。
- 设置页“移除自定义 Agent”仍使用 `window.confirm`，不在本计划范围内。
- 合成预览没有诊断数据，Agent 状态点显示为灰色（unknown）；真实诊断下的绿/红状态只在代码中按 `agentHealth` 推导，未截图验证。
