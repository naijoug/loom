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
