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

## 第三切片：安装到工具、更新/卸载与恢复（M2a，3448f5d）

Settings → Skills 新增详情安装页与 Installed 列表。后端派生 Codex/ChatGPT、Claude Code、Amp/Codex 的全局/项目目标，支持 copy/symlink、预览后执行、显式更新、来源移除后的按安装身份卸载。新目标默认不勾选；旧 v1/TSV 目标只读提示需要迁移。安装状态与实际 CLI 可发现/调用分别记录。

新增 v2 安装清单、完整包指纹、同文件系统逐项事务、不可变 prepare 日志、终态回执和跨 dataDir 恢复。备份放在工具目录 `.loom-skill-transactions/`，不在 skills 扫描目录，目录自带 `.gitignore`。用户修改、失效预览、未知清单及缺失备份拒绝覆盖。纯文件安装/恢复计入 supervisor，updater 先取得固定协调域的独占生命周期 lease。

| 验证 | 结果与边界 |
|---|---|
| 最终 `pnpm check` | 前端 106 项、Rust 192 项通过；19 ignored 含真实模型 opt-in 和父测试实际启动的子进程入口；fmt/Clippy/typecheck/build/契约门禁通过 |
| release 构建 | `pnpm tauri build --bundles app` 通过；原产物后续不在 target 中，重新构建再次成功，用重新生成的包完成下面原生验收；不代表发布签名/公证 |
| 安装/事务回归 | 多工具 copy、symlink、移除来源后卸载、修改保护、用户明确替换与备份、旧清单拒绝接管、目标逃逸、目录指纹结构、共用 manifest 批量卸载、预览失效、取消/更新互斥、备份 Git 忽略均覆盖 |
| 故障恢复 | 每个目标 rename 故障、真实子进程无析构退出、另一 dataDir 恢复、用户事后编辑保全、committed 标记残留收尾通过；不是共享指令资源的 M2b 验收 |
| 原生安装生命周期 | 临时项目 Codex + Claude copy → 修改合成来源后显式更新 → 批量卸载；再安装共享 `.agents/skills` symlink → 移除来源登记 → Installed 中卸载。5 个规划均成功，合成源保全、测试安装全部移除 |
| 真实 Codex 发现与调用 | codex-cli 0.163.0 的 `skills/list` 能发现本次 copy 和 symlink；Loom Read-only 回合显式调用 `$loom-acceptance`，返回仅存在于技能正文的 `LOOM_SKILL_ACCEPTED`，exit 0；用户提示未含该标记 |
| 卸载后发现 | `skills/list` 找不到合成技能，保留的备份没有被当成已安装技能；不是对所有 CLI 的通用发现证明 |
| Chat/UI 回归 | 合成流式回合在设置 Skills 可见时继续更新，返回 Chat 保留内容；安装请求绑定发起项目，切换技能/项目不污染新面板；并发刷新合并排队、临时读锁错误可重载 |
| 搬移后的原生启动 | release 包复制到独立 target 子目录，通过统一脚本从包内 Contents/MacOS 启动；Settings → Skills 来源/Installed 可用，历史操作保留，旧全局目标显示 Migration required；进程 cwd 已核验，结束后 stop |
| 源 Manager 兼容保护（`7dfd89d`） | Node、Rust、CLI/TUI 的写入口在任何目标变更前检查清单；v2、损坏 JSON、symlink（含悬空链接）拒绝写入；全选目标先预检，`--force` 不绕过。源完整 check：应用 99、Rust 25、内容 32、旧管理测试 8 项通过 |

原生测试使用 `LOOM_PROFILE_DIR` 与 `LOOM_SKILLS_DATA_DIR` 隔离设置/库，只向临时项目安装；没有修改全局安装或旧 Task 数据。Codex 的实际发现探针还确认本机版本同时识别 `.codex/skills` 和 `.agents/skills`，因此共享目标标注 Amp / Codex，提示避免重复安装。

原生验收发现并修复：操作事件和初始状态读取争用 writer 锁；UI 改为排队读取，后端提供有界等锁与独立取消控制；同批卸载多个技能时共用 manifest 的指纹需随每次已验证提交更新；备份必须置于技能发现目录外。新增回归覆盖这些行为。

持久化脱敏证据：[原生安装生命周期](samples/skills-migration/native-install-lifecycle.json)、[真实 Codex 调用](samples/skills-migration/codex-native-invocation.json)、[release 搬移运行](samples/skills-migration/relocated-release.json)。辅助日志：`/tmp/loom-skills-m2a-gate-check.log`、`/tmp/loom-skills-m2a-relocation-rebuild.log`、`/tmp/skills-legacy-fence-check.log`。

## G1 补充回归：固定安装版本与更新失败准入

- `operations::tests::git_refresh_does_not_update_installed_copy_or_symlink_until_reviewed` 使用真实本地 Git 与 URL rewrite 贯通来源导入、copy/symlink 安装、刷新、显式更新、上游移除和孤立安装卸载：刷新到 B 时两种安装仍读 A，显式更新才读 B；旧 A 快照保留；上游不再包含该技能及移除来源登记后，B 链接仍有效并可按 installationId 卸载。没有外网或个人安装写入。
- 首次用例误把空来源刷新预期为成功，实际实现拒绝无技能快照。用例已分别验证“空来源刷新失败并保留最后成功版本”和“来源仍有其他技能时，被移除技能的旧安装有效”；不为通过测试降低快照完整性要求。
- 应用更新入口抽取 `with_install_admission`，实际 updater 与可控安装器替身共用生命周期 lease → supervisor 准入路径。测试验证纯文件工作阻止安装器执行、本地准入失败释放先取得的 OS lease、更新中拒绝新操作、安装失败释放两层准入、随后重试成功。未实际安装应用更新。
- 定向 Git 回归 1 项、updater 3 项及全目标 Clippy 通过；随后 `pnpm check` 前端 106/Rust 194 与 `pnpm tauri build --no-bundle` 均通过。辅助日志 `/tmp/loom-skills-g1-pinned-update.log`、`/tmp/loom-skills-g1-updater.log`、`/tmp/loom-skills-g1-extra-check.log`。

## 第四切片：共享指令、wrapper 与诊断（M2b 分项）

- `instructions.rs` 负责 category/activation、auto/global 的 inject.md、manual 显式 wrapper 和只读诊断；使用者 ledger 放固定协调目录，跨 dataDir 共用。Codex/Amp 项目目标复用 AGENTS.md，Claude 使用 CLAUDE.md；全局路径保持旧工具约定。源内容不执行。
- 安装规划包含指令/归属前指纹，UI 显示实际受影响文件；应用时复核并把技能、清单、指令和归属一起提交。未知旧 marker、缺失使用者清单、不同共享内容、用户编辑和目标逃逸均拒绝；cron 不注入，manual wrapper 默认不勾选。已安装 wrapper 的选项在重新打开时恢复。
- 状态与 Installed 列表显示 current/missing/stale/malformed，诊断不写文件。local-live 注入正文变化能标 stale；来源移除后仍可根据安装归属卸载指令与 wrapper。
- 52 项 Skills 服务测试通过；最终 `pnpm check` 前端 108/Rust 206、全目标 Clippy、fmt 和 `pnpm tauri build --no-bundle` 通过，20 ignored 含父测试实际运行的子进程入口与真实模型 opt-in。服务测试覆盖：含两个独立进程/不同 dataDir 并发写共享 AGENTS、同技能多个使用者部分/最后/批量卸载、重复安装、冲突、CRLF/权限保留、范围内符号链接、路径逃逸、陈旧预览和未知归属。并发测试首次因把正常 lock_busy 当异常而失败，改为有界重试/重新预览，仍严格拒绝其他错误；两块与原用户正文均保留。
- 跨文件恢复在指令、ledger、manifest 的每个 rename 点中断，另一 dataDir 通过真实恢复服务还原；无关项目文件即使带有效 journal 也拒绝恢复。已有事务测试继续覆盖真实子进程崩溃和事后用户编辑保全。跨目录 rename 已补源/目标父目录同步；测试不是断电或存储硬件故障证明。
- 前端定向 9 项通过，新增 wrapper 显式选择、受影响文件预览、诊断不触发写入；原测试改为只选择安装目标 checkbox，避免把新 wrapper 选项误当目标。
- 原生隔离项目完成：添加 auto fixture → 预览两个项目目标及 AGENTS.md → 安装后两项均报告 current → 卸载 Codex 仍保留一块及 Amp → 卸载 Amp 后精确恢复 `User acceptance rules\n` → 移除来源登记；源包保留，测试安装均已卸载，预览已停止。见 [共享指令原生证据](samples/skills-migration/shared-instructions-native.json)。未向模型发送请求，也未写全局安装。

源用例迁移对应：`test_sync_update_remove_preserve_user_bytes_and_other_blocks`、`test_crlf_and_symlink_are_preserved` 对应 Loom 安装级 CRLF/权限/共享使用者与受限链接测试；malformed 对应 marker 拒绝与原文保全；copy 强制更新/用户修改、symlink 重装/卸载、未托管冲突由安装与新诊断测试覆盖。旧脚本测试暂保留到 CLI 兼容及 G2，不能先删除源功能。

辅助日志：`/tmp/loom-skills-m2b-final-check.log`、`/tmp/loom-skills-m2b-final-build.log`、`/tmp/loom-skills-m2b-native.log`。原生操作发生在最后一轮目录同步与 wrapper 选项恢复小修正之前；这些修正已通过完整回归与 release 构建，未重复原生操作。

该切片交付时多使用者共享内容一致更新尚未实现；下面的补充交付关闭此项。旧指令归属重建仍属于 M4，CLI/TUI/翻译/旧数据接管及 G1 视觉/无源码访问验收继续推进。

## M2b 收尾：多使用者一致更新

- 后端检测共享指令路径，预览增加 coordinated 标记。更改共享内容须选齐所有已核实使用者；每个使用者请求的最终内容必须一致。单目标和统一更新共用 prepare/stage 实现，统一更新合并重复资源，保留一个组级 journal 与终态回执。
- 新增服务回归覆盖：A 安装给 Codex/Amp → 仅更新一个被拒绝 → 选齐后两份包、清单、指令一起更新 B → 重试幂等 → 卸载恢复；预览后一个目标被编辑，另一个目标与共享块都不更新；两个包/两个清单/正文/ledger 共六项资源的 12 个 rename 中断点，另一 dataDir 恢复后全部精确匹配原状态。
- 临时 HOME 路径分别验证全局 Codex 的 `.codex/AGENTS.md`、Claude 的 `.claude/CLAUDE.md`、Amp 的 `.config/amp/AGENTS.md`，卸载恢复原文；未改真实 HOME 环境。缺失和损坏的诊断验证未创建/修复文件。
- 最终 `pnpm check` 前端 109、Rust 211 项通过，20 ignored 的范围不变；`pnpm tauri build --no-bundle` 和 debug app bundle 构建通过。日志：`/tmp/loom-skills-m2b-closeout-check.log`、`/tmp/loom-skills-group-release-build.log`。
- 原生临时项目验证：预览明确“共用指令，一起应用”；安装 A 后只修改来源到 B，界面显示有更新；显式替换两个目标后两份 inject.md 均为 B，共享块为 B，两个 manifest 指纹一致，检查持久化日志只有一份组级 prepare、六项资源；批量卸载后原用户规则恢复，来源文件保留，登记移除，预览停止。见 [一致更新原生证据](samples/skills-migration/coordinated-update-native.json)。没有模型调用或全局安装变更。

M2b 对当前托管安装的出口已通过，允许继续 M2c；旧来源/指令归属重建及实际 CLI 入口仍分别受 M4/M2c 门槛约束。统一恢复不等于跨文件系统的瞬时原子提交，中断后发现用户修改会保留待恢复状态。

## 当前限制与后续执行入口

M2a 安装闭环已有上述代码和原生证据，G1 仍待综合关闭：完整 Skills 视觉矩阵与无源码访问条件；远程 A → B 安装更新和更新器准入/失败的服务入口已按上一节补验。搬移运行只证明独立包路径及工作目录；源 checkout 仍在磁盘，不冒充无法访问源码的验收。

M1b GitHub API、M2c CLI/TUI、M3 翻译与分享、M4 旧数据接管尚未完成；M2b 当前托管安装出口已通过，共享指令/wrapper/只读诊断/一致更新与恢复均有上述证据。外网 GitHub/GitLab 与真实翻译 provider 验收仍待后续，G2 未通过。

旧 library/JSON/TSV 合成兼容样本已加入 `src-tauri/tests/fixtures/skills/`，尚未执行真实旧数据接管。M5 的管理测试与过渡入口需等 M2c/M7 移除。旧 Manager 新源码的版本保护不能修补已安装历史二进制，也不能让旧客户端参加 Loom 锁协议；切换期间仍须停止旧 Manager 写入。旧应用与数据保留，不提前退役。
