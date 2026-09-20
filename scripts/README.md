# Loom Scripts

本目录存放本地开发、预览和调试脚本。默认优先使用 `debug.sh`，它会在启动前清理上一轮 Loom 预览/桌面 dev 进程，避免 Vite 自动换端口后出现多个预览实例。

## 推荐启动方式

```bash
pnpm install
./scripts/debug.sh
```

`debug.sh` 默认启动 Tauri 桌面 App，使用固定端口 `1420`。常规任务不覆盖端口或主机。

可用命令：

```bash
./scripts/debug.sh desktop   # 启动 Tauri 桌面 App，默认命令
./scripts/debug.sh web       # 启动浏览器预览
./scripts/debug.sh status    # 查看当前预览/桌面 dev 状态
./scripts/debug.sh stop      # 停止本仓库启动的预览/桌面 dev 进程
```

## 浏览器预览

```bash
./scripts/debug.sh web
```

`debug.sh web` 统一清理旧桌面/预览进程，然后调用辅助脚本 `preview.sh`。

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

验证结束统一执行 `./scripts/debug.sh stop`。`preview.sh stop` 只清理 Web 辅助预览，不替代桌面进程清理。手工启动先确认本仓库无旧 Vite，使用 `pnpm dev --host 127.0.0.1 --port 1420 --strictPort`。

脚本保留端口/主机环境变量供专门排障使用；常规 Agent 验证固定 `1420`，不得通过随机换端口绕过冲突。

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

`docs:release:check` 检查必备发布资料、相对链接、路径泄漏、状态枚举与分发约束。状态可以随证据推进，不冻结历史 Wait/Hold；候选相关 Pass 必须关联同一 artifact 的复核记录，Invite-only 要求所有 gate 都有同候选证据。格式见 [发布证据约定](../docs/guides/release-evidence.md)。检查器不能证明签名命令真实执行，维护者仍需复核。

`docs:release:test` 在临时 fixture 中验证合法推进，以及无证据 Pass、候选不一致、记录越界/缺失、错误枚举、分发条件不足和安全约束丢失等拒绝路径。调整发布门禁时运行这两个命令。

日常变更按 [验证矩阵](../docs/testing.md#开发变更验证矩阵) 选择相关检查；里程碑和发布候选运行完整检查：

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
- 常规浏览器预览执行 `./scripts/debug.sh web`；`preview.sh` 是辅助入口。
- 验证结束后执行 `./scripts/debug.sh stop`。
