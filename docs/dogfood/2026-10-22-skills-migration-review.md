# Skills 迁移：API 来源与源测试等价验收

日期：2026-10-22。代码起点：Loom `0b63aad`、skills `6b6090e`；本记录对应随后提交的 M1b/SSH/等价补齐切片。Chat GFM 与图标等已有工作区改动不属于本切片。

## 源管理测试 8 项逐项对照

源文件为 skills 仓库 `apps/scripts/tests/test_managed_instructions.py`。目标测试分别位于 Loom 的 [安装测试](../../src-tauri/src/skills/installers_tests.rs)、[指令测试](../../src-tauri/src/skills/instructions_tests.rs)、[CLI 测试](../../src-tauri/src/skills/cli_tests.rs)。下表记录断言和有意差异，不以测试数量相同表示等价。

| 源测试 | Loom 目标测试 | 保留断言与明确差异 |
|---|---|---|
| `test_sync_update_remove_preserve_user_bytes_and_other_blocks` | `shared_instructions_keep_other_users_and_preserve_crlf_and_permissions`；`doctor_detects_local_live_injection_changes_without_writing`；`coordinated_shared_update_requires_all_users_and_changes_the_whole_group` | 同步、更新和移除只改受管块，用户正文/其他块保全；新增使用者集合与整组更新保护，部分卸载不会删除其他安装依赖的块。 |
| `test_malformed_block_leaves_file_unchanged` | `malformed_owned_block_refuses_update_and_uninstall_without_changes`；`malformed_duplicate_and_nested_markers_never_produce_a_span` | 异常、重复、嵌套 marker 拒绝修改；真实安装服务的更新/卸载拒绝后，指令、包状态、清单字节均不变。 |
| `test_crlf_and_symlink_are_preserved` | `shared_instructions_keep_other_users_and_preserve_crlf_and_permissions`；`scoped_instruction_symlink_is_preserved_but_escape_and_stale_preview_are_blocked` | CRLF、权限与合法指令链接保留。安全差异：只接受解析到授权 home/project 范围内的链接，逃逸和预览后变化拒绝。 |
| `test_status_does_not_create_files` | `doctor_missing_target_does_not_create_instruction_or_install_directories`；`missing_and_malformed_instruction_diagnoses_do_not_write_or_repair` | 缺失/异常诊断不创建 AGENTS、工具安装目录或来源登记，不自动修复指令。 |
| `test_unsupported_manifest_blocks_force_install_and_uninstall` | `legacy_manifests_are_not_implicitly_adopted`；`future_malformed_and_symlink_manifests_block_replacement_and_uninstall` | force 不绕过不支持/损坏/链接清单保护，拒绝后包、指令、清单原样保留。版本差异：旧客户端拒绝 v2，Loom 支持 v2；v1/TSV 待 M4 显式迁移，未来版本拒绝。 |
| `test_copy_refresh_requires_force_and_preserves_local_edits` | `cli_copy_force_and_orphan_uninstall_preserve_unmanaged_content`；`modified_copy_is_protected_until_explicit_replacement_and_backup_is_retained`；`copy_source_updates_are_visible_without_marking_installed_instructions_broken` | copy 重装默认跳过；显式替换才更新并保留备份；修改过的安装不能强制卸载。doctor 改为检查已安装版本健康，来源更新另由 `update_available` 表示：安装 A、来源 B 时 current 与可更新可以同时成立，不能用 doctor 退出码检测新版。 |
| `test_symlink_reinstall_syncs_injection_and_uninstall_retains_user_rules` | `cli_and_desktop_services_share_installs_and_live_instruction_refresh`；`scoped_instruction_symlink_is_preserved_but_escape_and_stale_preview_are_blocked`；`shared_instructions_keep_other_users_and_preserve_crlf_and_permissions` | 同一 local-live 来源重装同步注入，卸载保留用户规则与合法指令链接；共享块遵循最后使用者删除规则。远程 symlink 固定快照，刷新来源不隐式更新安装。 |
| `test_unmanaged_conflict_does_not_gain_instructions` | `unmanaged_auto_skill_conflict_never_adds_instructions` | 同名未托管内容在 skip 和 force 两种策略下均保全，不新增注入或 manifest；force 更严格地拒绝未托管替换。 |

以上目标用例均在本次完整检查中通过，M2b 等价记录出口关闭；旧归属接管依然属于 M4，不由这些测试证明。

## M1b 与 SRC-09

- 新增 10 项离线 API 测试：固定 commit 的完整包与安装、二进制/执行位/嵌套包、API/Git 独立来源、默认分支、旧模式字段、归档过滤、坏路径/模式/大小、truncated/缺 blob/损坏 blob、LFS、认证/限流/重定向、请求中取消。失败不发布新快照。
- `ssh_configuration_is_preserved_and_default_is_only_used_when_unconfigured` 使用真实本地 Git upload-pack 与 SSH 替身覆盖 7 个分支：环境命令、core.sshCommand、自定义 SSH 程序、variant、无配置默认、无效配置、命令失败。保留用户配置，只在缺省时补 BatchMode；日志没有替身秘密标记。这不是外网 SSH/私有仓库认证验收。
- 显式网络测试 `real_public_github_api_materializes_and_installs_without_git` 匿名读取 vercel-labs/agent-browser：10 个技能、45 个快照文件、31 个 HTTP 请求、0 个 Git 进程，临时项目 copy 安装/卸载通过。版本与指标见 [API 真实记录](samples/skills-migration/github-api-real.json)。
- 原生 release 应用在 Settings → Skills 选择 API 模式，匿名导入 tobi/qmd；2 个技能、9 个 HTTP 请求、0 个 Git 进程。临时项目 Codex copy 与包内全部 2 个文件哈希一致；删除来源后从已安装页卸载孤立安装成功，旧快照保留，HTTP 日志可见且无凭据字段。见 [原生记录](samples/skills-migration/github-api-native.json)。未运行下载技能的脚本或 Agent。
- 真实私有认证、外网 GitHub/GitLab clone/SSH 的剩余矩阵不能由上述匿名 API 或本地替身代替；继续按 G1/G2 门槛补验。

## M5 内容独立性

重新复制内容、归档、质量脚本与测试到临时目录，完全不复制 apps/Loom，并在子进程 PATH 前放 node/pnpm/cargo/tauri 的失败替身。`bash scripts/skills-quality-check` 退出 0：32 个内容单测、326 个 perfect routing 管道用例通过；未触发失败替身，不调用模型。根内容入口不发现或执行管理测试。

原始哈希文件记录 272 个路径，其中 `skills/.DS_Store` 不受 Git 管理；当前 271 个受管技能/归档文件全部与原基线哈希一致，无新增受管内容路径。此处纠正计数口径，不把 Finder 元数据误报为技能内容删除。结构化证据见 [内容隔离记录](samples/skills-migration/content-isolation-v4.json)。

M5-02 在 Loom 等价覆盖和根内容入口排除管理测试后关闭，M5 内容独立出口通过。源管理测试及旧转发入口留到 G2 后的 M7 删除；M5 不依赖 M7，源应用此时仍保留。README 新/v2 安装入口指向 Loom，并说明旧数据仍需显式接管。

## 最终检查与范围

- `pnpm check` 退出 0：前端 111 项、Rust 246 项通过，23 项默认忽略；包括构建、fmt、Clippy 和文档门禁。真实 API opt-in 检查另显式执行通过，未声称所有忽略项通过。
- `pnpm tauri build --bundles app` 退出 0，产物为 `src-tauri/target/release/bundle/macos/Loom.app`；未签名/公证/发布。原生 API 验收在 API 实现的 release 包完成，随后 SSH 与等价补齐用例经完整检查及最终 bundle 重建通过。
- 日志：`/tmp/loom-skills-api-v4-final-check.log`、`/tmp/loom-skills-api-v4-final-bundle.log`、`/tmp/loom-skills-github-api-real.log`、`/tmp/loom-skills-api-native.log`、`/tmp/skills-content-isolated-v4.log`；临时日志不是长期唯一证据，关键结果保存在本记录及 JSON。
- 后续：M3 四个翻译 provider、M4 旧数据/配置/缓存/安装迁移与回滚、G1 视觉/无源码/外网 Git 余项、M6/G2、M7 源应用退役。整体迁移未完成，不删除私人 app data、缓存或既有安装。

## M3-01：翻译设置与凭据契约

在 `2649372` 后接入 Rust `translation/config.rs`、两个主窗口 IPC 与 Settings → Skills 内的 Translation settings。保留 OpenAI/OpenRouter/Codex/Claude Code 四个 provider 身份和远程旧默认模型；环境模型与保存模型分别显示，密钥状态只读返回存在性和来源。保存使用 schema/revision、显式 keep/set/clear、原子私有文件、生命周期准入与共用 writer/pending 校验；旧配置自动导入和实际模型调用均未启用。

新增 5 项 Rust 回归覆盖：DTO/Debug 无密钥、环境优先/清除不动环境、独立 provider 和并发 revision 冲突、坏/未来/链接配置保全、输入校验、更新与 pending 阻止写入。新增 5 项前端回归覆盖：保留密钥/显式替换清除、失败释放密码、旧请求隔离、保存期间禁用、保存某 provider 保留其他草稿。

完整 `pnpm check` 退出 0：前端 115/Rust 251（23 项忽略）；随后修正 Reload settings 文案并补齐跨 provider 草稿保留，前端 116 项全部通过，最终 release app bundle 重新构建通过。日志为 `/tmp/loom-skills-translation-config-check.log`、`/tmp/loom-skills-translation-front-final.log` 与 `/tmp/loom-skills-translation-config-final-bundle.log`。

原生验收使用全新临时 profile/data，清除两个远程 key 与模型环境变量；用假 key 保存 OpenAI/gpt-5-mini，文件 `0600` 且值准确，界面仅显示 Using saved key。停止并重启最终构建后仍显示正确模型/存在状态，选择 Remove saved key 后保存、重新加载，显示 No API key configured，模型保留。四 provider 状态及窄窗口深色表单观察通过。运行日志不含假 key；没有模型请求。预览前后按 debug.sh stop 清理。结构化记录见 [翻译配置原生记录](samples/skills-migration/translation-config-native.json)。

首次窗口绑定曾返回空 AX/一次超时，核实只有一个进程后重新绑定明确 app 路径恢复；没有因观察超时重新启动未知进程。原生下拉菜单第一次 Return 提交了 keep，随后明确选择 clear 并点击 Save，最终文件与界面共同证明清除成功。

M3-01 配置切片通过；M3-02 四 provider 实际执行/超时/取消、M3-03 翻译结果归属与复制/命令面板、M3-04 逐 provider 真实调用仍待完成。此记录不能作为翻译功能完整替代或 G2 的通过证据。

## M3 执行与 G1 补验（本轮续记）

代码基线：Loom `0ea881c` + 已有未提交翻译、Chat GFM/消息分隔等改动；skills `0f864ee`。本轮修改未提交，未改源 skills 仓库、旧 Task 数据、旧 Manager app data 或真实全局安装。下列结果更新当前状态，不覆盖上文历史计数。

### 历史容量问题

- 翻译历史不再调用全库 `operations::list`，改为按当前技能关联的 operation ID 读取，沿用 owner 锁识别 interrupted 与身份核验。返回最近 20 条终态和全部运行中任务；移除目录 3000 项硬失败，历史文件不删除。
- 共用操作列表改为返回最近 1000 条终态及全部运行中任务，不再因磁盘记录超过 1000 条报错。取得已退出 owner 的锁后重新读取记录，避免把刚完成的操作误标 interrupted。
- 回归覆盖 1001 条无关操作、1601 条翻译、超过 3000 个文件、老运行中任务保留、身份不匹配、中断恢复、旧历史保全和重新打开入口。
- 最终 release 原生包使用隔离 profile：先完成一次真实 IPC → 合成 Codex 子进程 → 结果持久化；停止后加入 1601 条合成历史。库中共 1604 个操作记录、3204 个翻译文件，重启后显示旧结果且 Translate 可用，再次点击成功产生新的结果。源文档哈希不变，所有 1601 条历史保留。此处“真实 IPC/原生”不代表调用了真实模型。
- 证据：[大历史原生记录](samples/skills-migration/m3-large-history-native.json)、[原生截图](../../designs/exports/skills/native-large-history.png)。

### M3-02/03 功能与验证

- 四 provider 执行、summary/markdown、版本绑定、任务取消、结果历史和复制已接主窗口 IPC；21 个 Skills 命令。执行上下文独立于 Chat，原文不修改，持久日志不含请求正文或密钥。
- HTTP 通过 loopback 服务执行真实 reqwest 传输，覆盖 OpenAI/OpenRouter 成功响应、401/403、429、503、302 拒绝重定向、无效 JSON、输出上限、超时和请求中取消；断言不自动重试、日志不含合成 key/正文/上游错误文本。生产 URL 仍固定 HTTPS，测试端点只能由私有测试路径注入。
- 本地进程覆盖 stdin/stdout/stderr 上限、失败、超时、运行中取消和进程组清理。默认测试仅使用合成程序；四 provider 工作流 fixture 不冒充真实账号验证。
- 迁入命令面板（Cmd/Ctrl+K、搜索、上下键/Enter/Escape、无选中技能时禁用相关动作），保留来源、安装、相对路径、摘要翻译、分享文本和翻译设置导航，并增加 Markdown 复制。翻译命令只打开面板，不自动发送。
- 分享文本包含技能原文和可预览相关文件，动态选择围栏长度以保留嵌套代码块；二进制/不可预览资源保留路径与大小说明。没有创建 Gist 或网络发布。剪贴板不可用时显示只读可选文本供手动复制；旧应用自动下载回退改为这个显式本地回退，不承诺导出完整二进制包。
- 原生验证：来源登记/正文/相关文件、合成 Codex 翻译完成后切页恢复、命令面板筛选并复制分享文本、重启大历史结果恢复通过。全部模型调用为 0，远程 key 从验收进程环境移除，Codex 从临时 PATH 的受控脚本解析，并核对操作记录里的实际程序路径。

### M3-04 逐 provider 状态

用户明确选择“先完成本地与 G1，账号未核实项标 blocked”。Codex 当前账号不是此个人工作区身份；OpenAI/OpenRouter 环境 key 和 Claude 身份未核实。本轮不调用这些账号，也不代替用户切换登录。四个 provider 的真实调用均为 **blocked**；不是 failed，也不能由 fixture 置为 passed。[逐 provider 记录](samples/skills-migration/m3-provider-acceptance.json)。M3-02/03 本地工程切片已完成，M3 总出口仍等待 M3-04。

### G1 补验结果

| 项目 | 本轮结果 | 边界 |
|---|---|---|
| Skills 视觉矩阵 | passed | 显式 DEV fixture；中英文 × 深浅主题、700px 窄窗口、空库/加载/错误、搜索/文件、安装预览/卸载、翻译结果/取消/切技能、命令面板、返回 Chat 保留草稿；[机器结果](samples/skills-migration/g1-visual.json) |
| Markdown 布局 | passed | 截图发现 Settings 的 680px 表格最小宽度泄漏，已限定 Skills Markdown 样式；新增内部横向溢出断言 |
| GitHub HTTPS | passed | 匿名真实 tobi/qmd 导入与刷新，固定 revision、完整快照、详情读取、旧快照保留；未运行包内脚本 |
| GitLab HTTPS | passed | 匿名真实 gitlab-org/cli，多层/来源兼容沿用已有 fixture；导入与刷新、详情与旧快照保留通过 |
| 含 symlink 的 GitLab 样本 | 按安全边界拒绝 | gitlab-org/ai/skills clone 成功，但快照校验拒绝符号链接/特殊文件；换用支持范围内的官方 CLI 样本，未放宽校验 |
| 随包 CLI 无源码读取 | passed | macOS sandbox 明确拒绝两仓源码内容，随包 CLI copy 安装到临时项目成功；桌面 Installed 正确识别并按预览卸载；随后隔离 CLI status 成功，源保留 |
| 原生包搬移、启动、重启 | passed | 最终 `.app` 位于 target 下独立目录，从 Contents/MacOS 启动，临时 profile；来源、项目与大历史结果重启保留 |
| 禁止源码读取的桌面验收 | **blocked** | `sandbox-exec` 下进程存活但原生自动化无法识别窗口；额外拒绝源码、仅允许元数据、全部允许三种配置均未得到可观察窗口；相同包不经 sandbox-exec 则窗口与操作正常。不能据此判定源码依赖，也不能把 CLI 通过当成桌面通过 |

真实来源记录：[HTTPS 来源](samples/skills-migration/g1-public-git-https.json)。隔离记录：[源码拒绝与对照](samples/skills-migration/g1-source-isolation.json)。真实 SSH/私有认证完整矩阵仍属于后续 G2，未由匿名 HTTPS 代替。之前真实 CLI 发现/调用及共享事务/更新准入证据继续引用 2026-10-21 记录，本轮不冒充重复调用。

### 工程门禁与下一步

- 完整 `pnpm check`：前端 125、Rust 265 通过，25 ignored；包含 build/fmt/Clippy/文档检查。其后新增本机取消用例独立复验，最终计数与退出码见本段后续补记。
- 最终 `pnpm tauri build --bundles app` 通过，包含 release 编译；没有签名、公证、发布。本轮未另称执行了 no-bundle 命令。
- `pnpm smoke:visual` 最终通过，包含原 Chat/OpenUI 检查和新的 Skills 矩阵。最初自动化因页面切换等待与按钮文案选择器失败，修正后重跑；不是生产安装失败。截图均在 `designs/exports/skills/`。
- 受限 sandbox 首次禁止 loopback/进程组检查，经运行环境批准后完整复验；未把受限运行失败隐去或记为通过。开发期间修正了 CommonJS 单测不能直接编译 `import.meta` 的 fixture 注入问题，最终仅由 main 的 DEV 分支注入，生产默认无 mock。
- 日志：`/tmp/loom-skills-m3-g1-check.log`、`/tmp/loom-skills-m3-g1-bundle.log`、`/tmp/loom-skills-g1-visual.log`、`/tmp/loom-skills-{github,gitlab}-git-acceptance.log`、`/tmp/loom-skills-local-cancel-final.log`。关键结果另存上述 JSON；临时日志不是唯一证据。
- 预览已按 debug.sh stop 停止，临时安装已卸载。继续保留临时库供复核；没有删除真实历史来解决容量问题。
- **G1 尚未关闭**：仅剩本轮未通过的桌面无源码读取验收；需要可观察的隔离执行环境补证。**M3 总出口 blocked**：按用户决定等待四 provider 个人账号核实与真实调用。M4/M6/G2/M7 未进入，本轮不删除旧管理器。

最终补记：`pnpm check` 再次退出 0，前端 **125**、Rust **266** 通过，**25 ignored**；包含新增本机运行中取消用例。单独本机进程 3 项亦通过。最终日志 `/tmp/loom-skills-m3-g1-final-check.log`。此后仅回填文档与入口代码缩进，未改运行行为。相对链接检查和 `git diff --check` 通过；生产 bundle 不含 Skills fixture 文本。

2026-10-25 提交整理复核：Skills 专属内容通过独立临时树验证，Chat GFM/消息分隔与图标不纳入。该树前端 119 项（不含工作区额外 Chat 测试）；Rust 初跑 263 通过、1 项 SSH 进程组清理 EPERM，失败项定向复验通过（1/1）；其余 Rust 263 项无需重复。历史截图保留其原工作区渲染版本，不宣称专属提交包含 GFM。M5 勾选状态已按既有证据同步。
