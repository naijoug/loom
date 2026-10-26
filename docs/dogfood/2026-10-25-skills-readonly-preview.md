# Skills 已完成切片提交与 M4-01 只读预览

日期：2026-10-25（America/Los_Angeles）。起点 `0ea881c` + 工作区已有改动；先提交 `46af4ba`，再实施 M4-01。用户授权范围为整理提交、同步 M5、只读发现/预览；账号实测继续 blocked，不接管真实旧数据。

## 提交边界

`46af4ba feat(skills): deliver translation workflows and migration evidence` 保存 M3 执行、历史容量修复、交互、测试及 G1/M3 分项证据，按既有证据勾选 M5 四项。使用独立 Git index 和导出树验证，原来已暂存的 `src/features/chat/markdown.ts` 删除字节保持一致；Chat GFM、消息分隔、package/lockfile、外链权限和图标没有混入提交。两份共享文档只提交 Skills 部分。

该专属树前端 119 项通过，build/fmt/Clippy/文档检查通过；Rust 初跑 263 通过、1 项 SSH 进程组清理出现系统 EPERM，定向复验 1/1 通过，25 ignored。此前完整工作区 125/266 是包含额外 Chat 测试的历史口径，不与专属树计数混用。日志 `/tmp/loom-skills-checkpoint/check.log`、`ssh-retry.log`。

## M4-01 实现范围

- Rust `migration.rs` 提供 `MigrationPreview`，schemaVersion=1、readOnly=true、canApply=false。主窗口 `skills_plan_migration` 和 CLI `migration-preview` 共用它；没有 apply/rollback 命令。
- 仅定位旧桌面已知 app data 或显式目录，旧 workspace/随包目录需显式提供。桌面用户级清单默认不选；项目需已登记。CLI `--scope project` 排除用户级清单，默认 global 的行为在帮助文档说明。
- 识别旧 library v1、Git/API/desktop-local 来源与 source ID aliases、声明的缓存路径、config.json 中四 provider 的 key/model 存在性、JSON v1 与 TSV 安装清单；指出缺失来源、重复 ID/alias、模式/链接不匹配、旧 bundle 链接、范围外引用和不支持的版本。
- 仍是观察报告：所有旧安装 legacy-unverified，不比较整个 copy 包内容，不重建指令/使用者，不扫描未登记缓存，不导入凭据。输出观察指纹供复核，不是可直接执行的事务规划。
- 绕开会初始化目录的 Repository::open / Coordinator / Operation；库校验复用纯读取/解析。单文件 1 MiB、累计 16 MiB、来源/安装记录 2000 项上限；超限或错误不按空状态成功。旧配置值不进 DTO、Debug 或日志。
- macOS 已授权根的路径别名按明确前缀归一化；内部 symlink 不扩大读取权限。实际 CLI 首次发现 /var 与 /private/var 被误判为范围外，修复后增加相应回归。

## 验证

| 检查 | 结果 | 证据边界 |
|---|---|---|
| 新增 Rust 9 项 | passed | 缺失/坏/未来版本、JSON+TSV、源码/安装链接边界、重复 alias、大小与条数限制、配置脱敏、默认排除 global、重复结果一致、文件/目录/权限不变、CLI 无库/锁初始化 |
| 新增前端 3 项 | passed | 不自动扫描、参数范围明确、无执行按钮；切项目旧结果不串入，失败可重试，编辑后旧预览清除 |
| 双端契约 | passed | 第 22 个 Skills 命令及 MigrationPreview 样本；初次命令排序不一致已修正后复验 |
| 完整工作区 `pnpm check` | passed | 前端 128、Rust 275；25 ignored；build、fmt、Clippy、文档检查包括在内。无真实模型调用 |
| release app bundle | passed | `pnpm tauri build --bundles app`；未签名/公证/发布 |
| `pnpm smoke:visual` | passed | 合成页面新增显式迁移预览、配置存在性及无 Apply 入口；包含既有 Chat/Skills 矩阵。截图 [网页预览](../../designs/exports/skills/migration-preview.png) |
| 实际随包 CLI | passed | 同一临时旧库与临时项目连跑两次，preview 相同；旧文件/权限不变，新的 Loom 数据目录不出现；fake key 不在输出；[结构化记录](samples/skills-migration/m4-readonly-cli.json) |
| 原生主窗口 | passed | 新 profile，显式输入临时旧库，global 未勾选；来源 alias/缓存被识别，仅显示 key/model 存在性，无执行按钮；扫描前后旧样本哈希/权限不变；没有注册来源或导入配置。[原生记录](samples/skills-migration/m4-readonly-native.json)、[截图](../../designs/exports/skills/migration-preview-native.png) |

测试只使用本轮合成旧数据，未读取真实私人旧库作为样本，不替换 HOME/CODEX_HOME，不调用任何账号。GUI 进入 Skills 时原有来源列表可能初始化隔离 profile 中的空库目录；迁移预览服务自身不创建任何文件，CLI 的零初始化由目录前后对比单独证明。

日志：`/tmp/loom-m4-check.log`、`/tmp/loom-m4-bundle.log`、`/tmp/loom-m4-visual.log`、`/tmp/loom-m4-preview-rust.log`、`/tmp/loom-m4-preview-front.log`、`/tmp/loom-m4-contract-check.log`、`/tmp/loom-m4-native.log`。关键结论保存为 JSON/本记录，不依赖临时日志长期保留。预览前后均使用 debug.sh stop。

M4-01 已完成本轮只读范围；M4-02–05 的备份、接管、共享归属核验、执行恢复与真实副本迁移尚未实施。M3-04 真实账号实测继续 blocked，G1 桌面源码隔离阻塞不变，G2/M7 未通过，不删除旧 Manager 或个人数据。

## 专属提交树复核

M4 待提交树单独导出验证，不包含 Chat GFM/消息分隔改动：前端 **122** 通过，build/fmt/Clippy/文档检查通过；Rust 初跑 **272** 通过，既有 Chat shutdown 的进程组清理再次出现系统 EPERM，定向复验 **1/1** 通过，合计 273 个用例，25 ignored。该差异仅来自排除了其他在途 Chat 测试；不把混合工作区计数套用于专属提交。日志 `/tmp/loom-m4-checkpoint/check.log`、`shutdown-retry.log`。相对链接/JSON/差异空白检查通过；原暂存 Chat 删除保持不变。
