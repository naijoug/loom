# Loom 完整版本验收报告

- 验收时间：2026-07-24 13:24 CST
- 最近复核：2026-07-27 CST
- 审计基线：`docs/requirements.md`
- 源码基线：`main` / `5d34892`，包含本轮及用户原有未提交改动
- 环境：macOS arm64、Node v22.22.3、pnpm 10.29.2、rustc/cargo 1.95.0
- 结论：完整本地开发闭环达到用户测试条件；2026-07-27 复核后代码级结论仍成立

## 强制门禁

| 门禁 | 结果 | 证据摘要 |
|---|---|---|
| `pnpm check` | 通过 | 前端/领域 85/85；生产构建；Rust fmt；严格 Clippy；Rust 166/166，另 1 项真实 Claude 凭据测试按设计忽略 |
| 生产前端包 | 通过 | 1823 modules；最大 chunk 390.18KB / gzip 101.42KB；无 500KB 警告 |
| `pnpm smoke` | 通过 | 8 个工作流页面 + 5 个 Settings 页面 |
| `pnpm smoke:interaction` | 通过 | 项目/任务弹窗、设置切换、结构化反馈、日志搜索、TaskSummary fixture 驱动的完成页，以及 960px 页面/终端控制区无横向溢出；真实 summary/export 由后端进程闭环测试覆盖 |
| `pnpm smoke:visual` | 通过 | 深浅主题各 13 屏，共 26 张截图；产物位于 `/tmp/loom-visual-smoke` |
| `pnpm dogfood:verify -- --strict` | 通过 | 0 失败、0 警告；真实 Agent action、失败验证、repair trace、成功验证、桌面回放证据齐全 |
| 后端真实进程闭环 | 通过 | Rust `command_runner_drives_two_repair_cycles_then_acceptance` 真实启动子进程，完成两轮失败/修复、最终通过、历史日志读取及 JSON/Markdown summary/export |
| Tauri release | 通过 | release binary、`Loom.app` 和 arm64 DMG 均生成；DMG 经 `hdiutil verify` 校验有效 |
| 发布版桌面 smoke | 通过 | `tauri://localhost` 启动；加载真实项目/3 个历史任务；旧任务无 summary 时安全降级；识别 Node/React/Rust/Tauri/Vite；About 后端健康为 OK；正常退出 |

## 2026-07-27 复核

- 重新执行 `pnpm check`、`pnpm smoke`、`pnpm smoke:interaction`、`pnpm dogfood:verify -- --strict` 和 `pnpm smoke:visual`，结果均通过；测试数量和生产包体积与 2026-07-24 记录一致。
- 复核既有 release binary 与 DMG 的字节数、DMG SHA-256 和 `hdiutil verify` 结果，均与下方发布产物记录一致；`codesign` 仍显示 linker ad-hoc 签名且无 Team ID。
- 本次没有重新执行 Tauri release 构建或发布版桌面 smoke；对应两项仍是 2026-07-24 的历史验收记录，不将产物完整性复核表述为新的桌面运行时覆盖。

## 发布产物

```text
src-tauri/target/release/bundle/macos/Loom.app
src-tauri/target/release/bundle/dmg/Loom_0.1.0_aarch64.dmg
```

- release 可执行文件：12,796,304 bytes
- DMG：4,950,701 bytes
- DMG SHA-256：`bb983f47e3095b39f328ac33eb40debae5610fdb279f1b6ab38946069f443b33`
- DMG `hdiutil verify`：VALID

首次 DMG 构建在受限沙箱内因 `hdiutil` 无法挂载镜像而失败；在获准的 macOS 发布环境中以相同源码重新执行后成功。这是构建环境权限差异，不是应用失败。

## 关键失败路径证据

- Rust 权威状态机拒绝跳过 Review、非法阶段转换和 terminal task 后续写入。
- 不同协作 Agent Review、失败 Review、缺失 Review 和 open blocker 均不能进入 Testing；接受风险需要持久化理由。
- 最新失败 validation 会覆盖较早成功证据；Agent action 不能冒充 validation 完成任务。
- 命令、PTY、Agent 与 Review 共用 cwd 边界、权限、危险审批和 secret 脱敏策略。
- 8 个并发命令 start/finish 保留全部 8 条 run 与 16 条事件；额外回归验证终态只合并到匹配 run。
- 反馈附件在复制前校验 run 归属，并执行扩展名、单文件/总大小和受控存储路径限制。
- Git baseline 测试覆盖 dirty tracked 与 untracked 文件，完成总结区分 `pre_existing` 与 `task_introduced`。

## 已知发布条件

- 当前没有 Apple Developer ID 证书与 Apple 公证；二进制为 linker ad-hoc 签名。产物适合本机开发测试，不应宣称为已签名/已公证的公开发行包。
- 默认测试不消耗真实 Claude 账户凭据；被忽略的真实 CLI canary 需要用户本机安装、登录后显式运行。确定性 adapter/进程/解析测试与现有严格 dogfood 证据均已通过。
- 仓库在实施前已有未提交改动，本轮按要求保留且没有 reset。测试任务的 Git baseline 与完成总结会显式区分原有脏改动。

## 用户测试入口

按 `docs/testing.md` 的十步路径测试真实项目。建议先使用可安全修改的临时 Git 仓库，并准备至少两个已登录、能力不同的本地 Agent，以完整体验规划、实施、独立 Review、失败修复和交付总结。
