# Loom Scripts

默认应用是本机 CLI Chat。开发预览统一通过 `debug.sh`：

```bash
scripts/debug.sh stop
scripts/debug.sh desktop   # 真实 Tauri 桌面
# 或 scripts/debug.sh web
scripts/debug.sh stop
```

固定端口 1420。Web 合成预览地址为 `http://127.0.0.1:1420/?fixture=simple-chat`；合成项目和消息只用于 Vite 开发态，不写入真实项目或调用模型。追加 `&cards=1` 会多出一个“Inline card preview”会话（一张有效卡片、一张回退卡片），需在设置“实验”中打开内联卡片才渲染为卡片；默认 fixture 不变，`smoke:visual` 不受影响。日志为 `/tmp/loom-preview-vite.log`，PID 为 `/tmp/loom-preview-vite.pid`；桌面 PID 提示文件为 `/tmp/loom-tauri-dev.pid`。

Skills 原生验收可先用 `pnpm tauri build --debug --bundles app` 构建，再通过 `LOOM_PREVIEW_BINARY=/absolute/repo/src-tauri/target/debug/bundle/macos/Loom.app/Contents/MacOS/loom scripts/debug.sh desktop` 启动准确的本地包；仅接受本仓库 target 下的 Loom.app，不覆盖系统安装版本。`LOOM_SKILLS_DATA_DIR=/absolute/temporary/data` 隔离技能库，`LOOM_PROFILE_DIR=/absolute/temporary/profile` 另行隔离设置、Agent 配置和最近项目；未显式设置技能库时使用该 profile 的 skills 子目录。两个变量均不改变 HOME、全局安装目标或用户固定协调锁域；原生安装验收仅选择临时项目目标。开始前及结束后照常 `scripts/debug.sh stop`。这个入口用于解决原生自动化按名称误选旧安装包的问题，不改变应用名或标识。该入口也接受复制到本仓库 target 下的 release Loom.app，并从其 Contents/MacOS 启动，便于验证不依赖源码工作目录。

| 命令 | 范围 |
|---|---|
| `pnpm check` | 分层检查（`scripts/check-layers.mjs`）、release 文档校验、OpenUI 提示词同步检查、前端测试/build、Rust fmt/clippy/test |
| `pnpm tauri build --no-bundle` | 编译桌面程序，不做发布或分发验收 |
| `pnpm smoke:chat` | 生产 Chat 仓储/runtime 的真实本地 fixture 进程，无模型请求 |
| `pnpm smoke:visual` | `simple-chat-smoke.sh`：按规定启动/停止预览，Chrome 检查新会话、分组、工具差异、主题、语言和窄窗口；功能栏矩阵（zh/en × 深浅 × 700px：52px 宽、当前项、⌘/Ctrl+2 与 ⌘/Ctrl+, 切换，以及切换后会话和草稿保持）截图在 `designs/exports/activity-rail/` |
| `pnpm smoke` / `pnpm smoke:interaction` | 同一简化 Chat 浏览器门禁的兼容别名，无需重复执行 |
| `pnpm smoke:diagnostics` | Chat 诊断包的离线 Rust 回归 |
| `pnpm e2e:complete` | 完整工程门禁和浏览器检查；可选发布构建仍需独立分发验证 |
| `node scripts/simple-chat-reachability.mjs` | 扫描 main.tsx / lib.rs 模块依赖，写入 `docs/reviews/simple-chat/reachability.json` |
| `pnpm openui:prompt` | 从 `src/features/chat/openui/library.tsx` 生成内联卡片提示词 `src-tauri/resources/openui-inline-prompt.md`；改组件库后必须重新生成，`--check` 只校验不写入 |
| `node scripts/check-layers.mjs` | 分层规则。Rust：`agent/` 不引用功能模块；Chat、Learn、Research 互不引用；chat/learn/research 生产代码不直接起进程（`#[cfg(test)]` 模块除外）。Git 获取经 `repository_sources` 调用 `tool_process`，Learn 不再单独放行 spawn。Agent CLI 只经 `agent::run` 启动。前端：`src/features/<x>` 不引用其它功能的代码或 CSS（白名单 `settings→skills`），`src/components`、`src/hooks`、`src/api` 不引用 `features/` |
| `pnpm check:core` | 显式选择冻结内核检查，不属于主门禁 |

网页截图与机器结果位于 `designs/exports/simple-chat/`。Chrome 可用 `CHROME` 覆盖可执行文件；截图是合成界面证据，不能替代真实 CLI 或 Tauri 交互。

真实 CLI 测试通过 `LOOM_REAL_CHAT=1 cargo test --manifest-path src-tauri/Cargo.toml <test-name> -- --ignored --nocapture` 显式启用，会消耗模型额度。测试名和当前结果见 [测试指南](../docs/testing.md) 与 [本轮验收](../docs/dogfood/2026-10-20-simple-chat-acceptance.md)。

目录中保留的 Craft/SDK/旧 REA 6.5.0／旧 Task 实验脚本仅为历史研究资料，不再由默认预览或 CI 执行；重新启用须先核对其冻结依赖与契约。当前预览已不提供 Workbench 启动开关。

Research 新 R0 使用 `research-target-probe.mjs --node <兼容Node绝对路径> --rea <固定包入口绝对路径> --output <全新临时目录>`，网站检查另传 `--chrome <Chrome绝对路径>`。探针创建自己的 JS App／本机网页，并验证 MCP、静态分析、Evidence 导出、错误／超限／取消；不安装依赖、不改用户 MCP，浏览器只使用专用 profile。`research-cli-probe.mjs <临时目录> <模型名>` 调用真实 Claude 无工具 profile，会使用当前账号额度；不提供模型名时沿用 CLI 配置。结果与实际限制见 [R0 验收](../docs/dogfood/2026-10-28-research-targets-r0.md)。这两项为显式验收脚本，不属于日常默认 CI。

Skills 视觉验收由 `pnpm smoke:visual` 同轮运行 `skills-smoke.mjs`。仅 DEV 的 `?fixture=simple-chat&skillsFixture=ready` 注入合成 API，可选 empty/loading/error/cancel；生产构建不包含该 fixture。截图和机器结果位于 `designs/exports/skills/`。

Learn 视觉验收同样由 `pnpm smoke:visual` 同轮运行 `learn-smoke.mjs`。仅 DEV 的 `?fixture=simple-chat&learnFixture=ready` 注入合成 Learn API（不调用 IPC、git 或模型），可选 empty/running/failed/error；截图和机器结果位于 `designs/exports/learn/`。

Research 视觉验收同样由 `pnpm smoke:visual` 同轮运行 `research-smoke.mjs`。仅 DEV 的 `?fixture=simple-chat&researchFixture=ready` 注入合成 Research API（不调用 IPC、git 或模型），可选 empty/running/failed/error；截图和机器结果位于 `designs/exports/research/`。

原生源码隔离探针可给 `LOOM_PREVIEW_BINARY` 搭配绝对 `LOOM_PREVIEW_SANDBOX_PROFILE`，debug.sh 在 macOS 用 sandbox-exec 启动准确 binary。2026-10-22 当前主机上即使 allow-default profile 也无法由原生自动化识别窗口，普通启动正常；因此此探针暂不能作为 G1 桌面通过证据，详见 Skills 当日验收。

### Discuss 合成矩阵

`discuss-smoke.mjs` 由 `pnpm smoke:visual` 调用，只操作 DEV `discussFixture`。覆盖两轮/汇总、失败/重试/取消、调用确认、引用/日志、中英文/深浅/窄窗口和 Chat 草稿保持。截图与结果在 `designs/exports/discuss/`；真实 CLI 入口见 [测试指南](../docs/testing.md#discussp7)。

### P8 原生验收边界

`LOOM_PROFILE_DIR` 的隔离范围是设置/Agent/最近项目与默认 Skills 库；Learn、Research、Discuss 使用 `app_data_dir()`，WebView 存储也不由该变量隔离。验收只创建明确命名的测试记录，不清理既有内容。当前原生证据与一次重启空白观察见 [P8 记录](../docs/dogfood/2026-10-28-p8-acceptance.md)。

### 原生启动恢复验收

`desktop-smoke.sh` 改为要求当前启动的 `interface_ready`，不把活着的空白进程计为通过。失败会输出启动阶段日志并清理本仓库测试进程。

`node scripts/webview-startup-fixture.mjs once|always` 从 dist 创建仅验收使用的 `.tmp/webview-fixtures/` 与 Tauri override config。`once` 跳过首次前端入口，验证自动一次重载后恢复；`always` 验证上限与原生提示。先 `pnpm build`，测试结束重新 `pnpm tauri build --bundles app`，不得交付注入包。见 [验收](../docs/dogfood/2026-10-28-webview-restart.md)。
