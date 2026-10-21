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

## 基础切片时的保证与限制（92507d3）

- 已验证 OS lifecycle lease/writer 锁、崩溃后 pending 仍可见、另一客户端阻止重叠写入、坏索引 fail closed、元数据权限/有界读取及 symlink 拒绝。
- 子进程用真实 OS 进程验证 kill 后解锁及无析构退出后恢复标记保留；不使用全局真实 HOME 或用户安装目标。
- pending 当前只能识别并阻止危险写入，完整恢复引擎、终态回执核对及 marker 清除未实现。不能据此宣称 V-17 的完整自动恢复通过。
- 生命周期锁尚未接到实际 updater/Skills 安装入口，共享注入使用者 ledger 尚未实现；V-16/V-18 仍未完成。
- 尚无 Skills UI、生产 IPC、Git 拉取/快照、安装服务、CLI/TUI、翻译迁移或旧 app data 接管；保留旧 Manager 及其应用数据。

## 第二切片：设置中的 Skills 来源管理

用户实施中进一步明确 Skills 应在设置侧边栏。实现改为 `SettingsPage` 懒加载 `SkillsPanel`，没有新增顶层 AppView 或 Chat 侧栏 Skills 按钮。来源、目录与后台操作服务不依赖 UI 位置，继续复用。

已实现本地目录登记/扫描/搜索/详情/相关文件、GitHub/GitLab URL 与受控 clone/checkout、固定 commit 快照、刷新保留旧版本、来源移除仅取消登记、持久化操作进度/取消/中断识别和脱敏 Git 执行记录。库默认 app data/skills；验收使用 `LOOM_SKILLS_DATA_DIR` 的临时隔离目录，协调域未改为临时库目录。

| 验证 | 结果与范围 |
|---|---|
| `cargo test ... skills::` | 22 项通过；新增 11 项覆盖目录/metadata/编码、逐包错误、相关文件、路径逃逸、持久化、旧 revision 保全/校验、真实本地 Git 导入刷新、恶意 smudge 禁用、取消和中断记录 |
| 双端 IPC/模型契约 | 八个命令、一个事件及 SkillsLibrary/SkillsOperation 在 canonical JSON、Rust 和 TypeScript 校验；首次全量检查发现 emitter 检查未包含新文件，补齐后通过 |
| `pnpm check`（最终 Settings 布局） | 前端 98 项、Rust 175 项通过，18 ignored 含真实模型 opt-in 和被父测试显式运行的子进程入口；fmt/Clippy/build/release 文档检查通过 |
| `pnpm tauri build --no-bundle` | 来源切片通过；其后 Settings 布局调整使用下列 app bundle 构建验证 |
| `pnpm tauri build --debug --bundles app` | 最终 Settings 布局成功构建 Loom.app；未做发布签名或公证声明 |
| 原生主流程 | 通过统一 debug 脚本启动准确构建路径：Chat → Settings → Skills，添加临时本地目录后列出 1 个技能，详情/相关文件可读；搜索无结果时旧详情清空，刷新记录显示 Complete |
| 原生重启与保全 | 停止并重新启动同一构建，来源和操作记录仍在；移除登记后技能库为空，原 `SKILL.md` 与 references 文件仍存在 |
| 原生 Chat 隔离 | 新聊天输入未发送验收草稿 → Settings/Skills → 返回 Chat，草稿仍在；结束时清空验收草稿，没有向模型发送 |
| 原生语言/主题 | 观察英文/系统深色和简体中文/浅色布局；结束前恢复 English/System，保留原用户偏好 |
| UI 回归 | 乱序详情、过滤选择、来源移除、旧操作事件、卸载订阅、搜索关键词及 frontmatter 概览投影通过；Chat Shell 测试确认 Skills 在设置侧栏且 Chat DOM 保持挂载 |

原生验收纠正了两个问题：表单 label 原先包含多个按钮导致 AX 名称混乱，改为独立 htmlFor/input；概览原先把 frontmatter 渲染成标题，改为只显示正文，Markdown 页保留原文。刷新同一技能保留当前详情页签。修正后再次构建并在原生窗口确认。

Git checkout 在 `.git/info/attributes` 禁用来源选择的 filter/encoding，实测恶意 smudge 未执行。快照明确拒绝未物化 submodule/LFS 和 symlink，不能把不完整包当成可安装内容。当前目录使用 scalar/list/folded-text YAML 子集；单文件、目录项、深度和快照大小均有限制。快照脚本保留 owner execute 位，同时保持私有权限。

辅助日志：`/tmp/loom-skills-settings-final-check.log`、`/tmp/loom-skills-settings-final-bundle.log`、`/tmp/loom-skills-native-final.log`。原生验证使用临时合成来源；没有导入个人真实技能库或操作用户已安装技能。截图已在本聊天的原生工具结果中观察；本轮未保存为仓库 PNG。

## 当前限制与后续执行入口

本次是来源/只读管理切片，未实现工具安装面板和完整安装事务，不能标记 G1 通过。仍需 M2a 的目标枚举、copy/symlink 安装/更新/卸载及恢复；随后在同一 Settings → Skills 板块补齐共享指令、CLI/TUI、翻译和迁移。外网 GitHub/GitLab、真实 provider、安装后 CLI 发现/调用及窄窗口原生验收仍待完成。

旧 library/JSON/TSV 合成兼容样本已加入 `src-tauri/tests/fixtures/skills/`，M4 尚未对它们执行实际接管。M5 的管理测试与过渡入口需等 M2c/M7 移除；保留旧 Manager，不提前退役。
