# Loom Scripts

默认应用是本机 CLI Chat。开发预览统一通过 `debug.sh`：

```bash
scripts/debug.sh stop
scripts/debug.sh desktop   # 真实 Tauri 桌面
# 或 scripts/debug.sh web
scripts/debug.sh stop
```

固定端口 1420。Web 合成预览地址为 `http://127.0.0.1:1420/?fixture=simple-chat`；合成项目和消息只用于 Vite 开发态，不写入真实项目或调用模型。追加 `&cards=1` 会多出一个“Inline card preview”会话（一张有效卡片、一张回退卡片），需在设置“实验”中打开内联卡片才渲染为卡片；默认 fixture 不变，`smoke:visual` 不受影响。日志为 `/tmp/loom-preview-vite.log`，PID 为 `/tmp/loom-preview-vite.pid`；桌面 PID 提示文件为 `/tmp/loom-tauri-dev.pid`。

| 命令 | 范围 |
|---|---|
| `pnpm check` | release 文档校验、OpenUI 提示词同步检查、前端测试/build、Rust fmt/clippy/test |
| `pnpm tauri build --no-bundle` | 编译桌面程序，不做发布或分发验收 |
| `pnpm smoke:chat` | 生产 Chat 仓储/runtime 的真实本地 fixture 进程，无模型请求 |
| `pnpm smoke:visual` | `simple-chat-smoke.sh`：按规定启动/停止预览，Chrome 检查新会话、分组、工具差异、主题、语言和窄窗口 |
| `pnpm smoke` / `pnpm smoke:interaction` | 同一简化 Chat 浏览器门禁的兼容别名，无需重复执行 |
| `pnpm smoke:diagnostics` | Chat 诊断包的离线 Rust 回归 |
| `pnpm e2e:complete` | 完整工程门禁和浏览器检查；可选发布构建仍需独立分发验证 |
| `node scripts/simple-chat-reachability.mjs` | 扫描 main.tsx / lib.rs 模块依赖，写入 `docs/reviews/simple-chat/reachability.json` |
| `pnpm openui:prompt` | 从 `src/features/chat/openui/library.tsx` 生成内联卡片提示词 `src-tauri/resources/openui-inline-prompt.md`；改组件库后必须重新生成，`--check` 只校验不写入 |
| `pnpm check:core` | 显式选择冻结内核检查，不属于主门禁 |

网页截图与机器结果位于 `designs/exports/simple-chat/`。Chrome 可用 `CHROME` 覆盖可执行文件；截图是合成界面证据，不能替代真实 CLI 或 Tauri 交互。

真实 CLI 测试通过 `LOOM_REAL_CHAT=1 cargo test --manifest-path src-tauri/Cargo.toml <test-name> -- --ignored --nocapture` 显式启用，会消耗模型额度。测试名和当前结果见 [测试指南](../docs/testing.md) 与 [本轮验收](../docs/dogfood/2026-10-20-simple-chat-acceptance.md)。

目录中保留的 Craft/SDK/REA/旧 Task 实验脚本仅为历史研究资料，不再由默认预览或 CI 执行；重新启用须先核对其冻结依赖与契约。当前预览已不提供 Workbench 启动开关。
