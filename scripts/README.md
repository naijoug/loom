# Loom Scripts

本目录存放本地开发、预览和调试脚本。默认优先使用 `start-local.sh`，它会在启动前清理上一轮 Loom 预览/桌面 dev 进程，避免 Vite 自动换端口后出现多个预览实例。

## 推荐启动方式

```bash
pnpm install
./scripts/start-local.sh
```

`start-local.sh` 默认启动 Tauri 桌面 App，固定使用 `http://localhost:1420` 作为 dev URL。

可用命令：

```bash
./scripts/start-local.sh desktop   # 启动 Tauri 桌面 App，默认命令
./scripts/start-local.sh web       # 启动浏览器预览
./scripts/start-local.sh status    # 查看当前预览/桌面 dev 状态
./scripts/start-local.sh stop      # 停止本仓库启动的预览/桌面 dev 进程
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

## 调试入口

```bash
./scripts/debug.sh
```

`debug.sh` 用于临时 Tauri 调试，默认端口为 `11420`，可通过环境变量覆盖：

```bash
PORT=11421 HOST=127.0.0.1 ./scripts/debug.sh
```

常规本地启动不要优先使用 `debug.sh`；只有在需要隔离调试端口或临时覆盖 Tauri dev URL 时使用它。

## 完整质量门禁

Release 文档门禁入口：

```bash
pnpm docs:release:check
```

该命令检查 `docs/release/` 中的必备发布资料、相对 Markdown 链接、反引号中的 release 文档引用、本机绝对路径泄漏，以及 `beta-release-review-checklist.md` 是否仍保留关键分发 gate 语句与完整 review 表字段（Gate / Required evidence / Current status / Stop rule）。新增或改动 Beta release 文档时先跑它，再根据改动范围决定是否继续运行 `pnpm check`。

提交前运行统一检查：

```bash
pnpm check
```

该命令先执行 `pnpm docs:release:check`，再依次执行前端单元测试与生产构建，以及 Rust 格式检查、严格 Clippy 和测试套件。GitHub Actions 使用同一脚本，避免本地与 CI 规则漂移。

完整 UI 回归入口：

```bash
pnpm e2e:complete
LOOM_E2E_RELEASE=1 pnpm e2e:complete  # 同时生成发布包
```

该入口在 `pnpm check` 之后继续执行流程、交互和深浅主题视觉冒烟；视觉产物默认写入 `/tmp/loom-visual-smoke`。

## 使用约定

- 启动 UI 预览前先停止上一轮预览，避免多个 `vite` 进程占用不同端口。
- 常规本地使用优先执行 `./scripts/start-local.sh`。
- 常规浏览器预览优先执行 `./scripts/start-local.sh web` 或 `./scripts/preview.sh start`。
- 验证结束后执行 `./scripts/start-local.sh stop`。
