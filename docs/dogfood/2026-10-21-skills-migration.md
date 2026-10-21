# Skills 迁移实施证据（2026-10-21）

状态：迁移进行中，G1/G2 均未通过。执行计划见 [Skills 迁移](../plans/2026-10-21/06:12-skills-module-migration.md)。本记录不把基础模块或合成测试当成完整功能验收。

## 基线与已提交切片

- Loom 起点：`codex/simple-cli-chat` @ `c8313dd60aa50831bce3d7a66ae3fbc20e72f273`，保留既有未提交 GFM/消息分隔等改动及已暂存的旧 Markdown 文件删除。
- skills 起点：`main` @ `a245966b15f75faf929b3cdfc6db7909d010e08c`；原未跟踪 `.agents/`、`skills-lock.json` 保留。
- skills `349d622`：修复 core/API 测试依赖已归档技能名称、UI 测试误把 UTC 零点当成本地同日的问题，不改产品行为。
- skills `fd7a4ec`：5 个内容质量工具与 4 个内容测试模块迁出 Manager，新增独立入口；原路径暂保留转发，安装管理测试仍随旧应用保留。

## 检查记录

| 范围 | 命令/场景 | 结果与边界 |
|---|---|---|
| Loom 初始基线 | `pnpm check` | 通过，前端 92 项、Rust 153 项；真实 CLI 测试未运行 |
| skills 初始基线 | `bash apps/scripts/skills-manager-check` | 内容 39 项通过；应用检查因已归档技能标题失败，随后还发现本地时区断言问题；已修复，不将初始运行写为通过 |
| skills 修复后基线 | 同上，`349d622` 对应工作区 | 完整通过：应用 96 项、Rust 24 项、内容/工具 39 项；typecheck/build/cargo check 通过 |
| 内容独立入口 | `bash scripts/skills-quality-check` | 32 项内容测试及 326 个 routing 管道样例通过；perfect 只验证评分工具，不代表模型路由准确率 |
| 隔离内容副本 | 仅 skills/archive/docs/scripts/quality/tests/quality，无 apps/Loom；node/pnpm/cargo/tauri 命令替身直接失败 | 内容检查通过；证明检查不依赖管理应用或这些 runtime |
| 保全核对 | 移动前后技能及 archive 的 SHA-256 映射 | 272 个文件哈希一致；没有改技能正文或归档 |
| 源应用兼容 | `bash apps/scripts/skills-quality-check` | 独立内容检查及额外 7 项旧安装/注入测试通过 |
| Loom 协调基础 | `cargo test --manifest-path src-tauri/Cargo.toml skills::` | 最终 11 项通过（8 个协调/文件场景＋3 个 URL 场景）；1 个 ignored 是被父测试显式启动的子进程入口，父测试实际执行了它 |
| Loom 静态检查 | `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings` | 最终增量代码通过；fmt 通过 |
| Loom 全量复验 | `pnpm check` | 协调基础版本通过：前端 92、Rust 161；之后新增 URL 规范化由上述 3 项增量测试和全目标 Clippy 覆盖 |
| Loom release 编译 | `pnpm tauri build --no-bundle` | 通过，产物 `src-tauri/target/release/loom`；仅编译，不是签名、原生 UI 或发布验收 |

受限环境的全量 Rust 检查曾在 Chat shutdown 用例失败；增强断言后记录为 `force_terminate_process_group(..., 9)` 返回 EPERM。定向复验曾通过，全量重复仍出现；在允许进程组信号的环境复验全量 check/no-bundle 通过。保留诊断断言，不为绕过沙箱而修改生产退出逻辑。

原始执行日志位于本机 `/tmp/loom-skills-*`、`/tmp/skills-manager-baseline-*`、`/tmp/skills-content-only-check.log`、`/tmp/skills-quality-compat-check.log`；它们是临时辅助信息，持久化结果以上表和实际测试源码为准。

## 当前代码保证与限制

- 已验证 OS lifecycle lease/writer 锁、崩溃后 pending 仍可见、另一客户端阻止重叠写入、坏索引 fail closed、元数据权限/有界读取及 symlink 拒绝。
- 子进程用真实 OS 进程验证 kill 后解锁及无析构退出后恢复标记保留；不使用全局真实 HOME 或用户安装目标。
- pending 当前只能识别并阻止危险写入，完整恢复引擎、终态回执核对及 marker 清除未实现。不能据此宣称 V-17 的完整自动恢复通过。
- 生命周期锁尚未接到实际 updater/Skills 安装入口，共享注入使用者 ledger 尚未实现；V-16/V-18 仍未完成。
- 尚无 Skills UI、生产 IPC、Git 拉取/快照、安装服务、CLI/TUI、翻译迁移或旧 app data 接管；保留旧 Manager 及其应用数据。

## 后续执行入口

补齐 M0 的旧格式 fixture 与快照/安装归属探针；推进 M1a 的 catalog/source service 和页面。M5 的内容工具独立化已验证，但管理测试及过渡入口需等 M2c/M7 才能移除。继续更新本记录和计划状态，不提前退役源应用。
