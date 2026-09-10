# Loom Scripts

本目录存放本地开发、预览和调试脚本。默认优先使用 `debug.sh`，它会在启动前清理上一轮 Loom 预览/桌面 dev 进程，避免 Vite 自动换端口后出现多个预览实例。

## 推荐启动方式

```bash
pnpm install
./scripts/debug.sh
```

`debug.sh` 默认启动 Tauri 桌面 App，固定使用 `http://localhost:1420` 作为 dev URL。支持通过 `PORT` 与 `HOST` 环境变量自定义端口与主机。

可用命令：

```bash
./scripts/debug.sh desktop   # 启动 Tauri 桌面 App，默认命令
./scripts/debug.sh web       # 启动浏览器预览
./scripts/debug.sh status    # 查看当前预览/桌面 dev 状态
./scripts/debug.sh stop      # 停止本仓库启动的预览/桌面 dev 进程
```

## 浏览器预览

```bash
./scripts/preview.sh start
```

默认地址：

```text
http://127.0.0.1:1420/preview/planning?step=review
```

可用命令：

```bash
./scripts/preview.sh start     # 启动预览，启动前会关闭旧预览
./scripts/preview.sh restart   # 等同于 start
./scripts/preview.sh status    # 查看预览状态
./scripts/preview.sh stop      # 停止预览
```

默认配置：

```text
端口: 1420
日志: /tmp/loom-preview-vite.log
PID:  /tmp/loom-preview-vite.pid
```

可以通过环境变量覆盖：

```bash
LOOM_PREVIEW_PORT=1421 ./scripts/preview.sh start
LOOM_PREVIEW_HOST=0.0.0.0 ./scripts/preview.sh start
LOOM_PREVIEW_LOG=/tmp/my-loom-preview.log ./scripts/preview.sh start
```

除非明确需要临时端口，否则不要改 `LOOM_PREVIEW_PORT`。常规开发和验证应统一使用 `1420`。

## 自定义端口调试

`debug.sh` 默认端口为 `1420`，如果需要隔离调试端口或避免端口冲突，可通过环境变量覆盖：

```bash
PORT=11420 HOST=127.0.0.1 ./scripts/debug.sh
```

## Release smoke 入口

无需启动桌面保存对话框的发布 smoke：

```bash
pnpm smoke:diagnostics
```

`smoke:diagnostics` 执行 `scripts/diagnostic-bundle-smoke.sh`，底层运行 `cargo test --manifest-path src-tauri/Cargo.toml diagnostic_bundle --lib`。它覆盖诊断包默认无日志、路径替换、fake secret 脱敏、项目外 / 不可读日志引用过滤、无效引用不挤占日志限额，以及 headless 文件写入 / 读回 harness。该命令只提供工程回归证据，不能替代 `docs/release/diagnostic-bundle-smoke.md` 要求的真实桌面导出、人工搜索和临时 JSON 删除记录。

需要验证 UI 入口但不操作真实保存对话框时运行：

```bash
pnpm smoke:interaction
```

`smoke:interaction` 会启动固定端口的 Web preview 并检查 Settings / Done pane 的关键交互和诊断入口。验证结束后脚本会清理预览进程；如中途被打断，手工运行 `./scripts/preview.sh stop`。

维护者本机 release app 启动 / 重启 smoke 使用：

```bash
pnpm smoke:desktop
```

`smoke:desktop` 只能证明当前机器上的 release app 可启动，不能替代 Developer ID notarization、Gatekeeper、安装 / 卸载或普通试用者分发 gate。

## 完整质量门禁

Release 文档门禁入口：

```bash
pnpm docs:release:check
pnpm docs:release:test
```

`docs:release:check` 检查 `docs/release/` 目录本身、必备发布资料、相对 Markdown 链接、反引号中的 release 文档引用、本机绝对路径泄漏，以及 `beta-release-review-checklist.md` 是否仍保留关键分发 gate 语句与完整 review 表字段（Gate / Required evidence / Current status / Stop rule）；每条 Stop rule 还必须保留对应 gate 的关键阻断证据，例如 credential 缺失时的 `No Keychain password item found`，避免被改成泛泛的“继续等待”。它也固定安装 / 卸载、首次启动和诊断包 smoke 记录模板中的 hold gate 语句，避免模板被误改成分发 pass。`docs:release:test` 会在临时 fixture 中验证 checker 的负向分支，确保缺少 `docs/release/` 目录、缺必备 release 文档、缺 evidence、错误 status、过短或泛化 stop rule、记录模板 hold gate 被误删、本机绝对路径、Markdown 断链和反引号 release 文档断链都会失败。新增或改动 Beta release 文档时先跑这两个命令，再根据改动范围决定是否继续运行 `pnpm check`。

提交前运行统一检查：

```bash
pnpm check
```

该命令先执行 `pnpm docs:release:check` 与 `pnpm docs:release:test`，再依次执行前端单元测试与生产构建，以及 Rust 格式检查、严格 Clippy 和测试套件。GitHub Actions 使用同一脚本，避免本地与 CI 规则漂移。

完整 UI 回归入口：

```bash
pnpm e2e:complete
LOOM_E2E_RELEASE=1 pnpm e2e:complete  # 同时生成发布包
```

该入口在 `pnpm check` 之后继续执行流程、交互和深浅主题视觉冒烟；视觉产物默认写入 `/tmp/loom-visual-smoke`。

## 使用约定

- 启动 UI 预览前先停止上一轮预览，避免多个 `vite` 进程占用不同端口。
- 常规本地使用优先执行 `./scripts/debug.sh`。
- 常规浏览器预览优先执行 `./scripts/debug.sh web` 或 `./scripts/preview.sh start`。
- 验证结束后执行 `./scripts/debug.sh stop`。
