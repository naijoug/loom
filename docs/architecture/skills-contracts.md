# Skills 契约与实施状态

基线：Loom `c8313dd`；源 skills `a245966`。执行入口为 [迁移计划](../plans/2026-10-21/06:12-skills-module-migration.md)。本文件区分已落地基础与后续实现约定，不作为功能交付证明。

## 当前实现

- `src-tauri/src/skills/{error,files,coordination}.rs`：有类型错误、私有/有界/原子元数据、跨进程生命周期 lease 和 writer 锁、带版本 pending 索引。
- `sources.rs`：GitHub/GitLab URL 与 transport 标准化、旧 ID alias、输入校验；尚无网络拉取。3 项规范化测试已通过。
- 服务不依赖 AppHandle、窗口或 ChatSession，构造时必须传明确协调根目录。当前尚未接生产路径解析、UI、安装器、CLI binary 或 updater。
- 八项定向测试包含两个真实子进程场景：杀死持锁进程后锁释放；子进程不运行析构直接退出后，新客户端仍看到 pending。缺日志、坏 JSON、未知 pending schema、路径穿越和 symlink 也有覆盖。
- 当前只支持安全阻止未恢复资源的新写入，尚无提交/补偿引擎，因此不暴露随意清除 pending 的接口。不宣称 V-17 的自动恢复或完整 M0 已完成。

## 路径与锁

生产路径解析的目标契约：库默认位于 Loom app data 的 `skills/`；固定协调目录位于同一 OS 用户的 Loom app data 下 `skills-coordination/`，不受自定义库目录影响。显式测试目录只能经依赖注入提供。

`Coordinator::enter` 获取共享生命周期 lease；`Coordinator::writer(&lease)` 获取短时独占 writer。WriterGuard 借用 lease，禁止生命周期提前结束。`try_update` 非阻塞获取独占生命周期 lease，并检查 pending；存在活动操作/待恢复事务时失败，RAII 释放准入。

顺序：生命周期 lease → supervisor operation/update guard → writer。生产接线前必须验证真实更新入口不会绕开该顺序，纯文件工作也计入 supervisor。

## 元数据与事务

- 私有元数据最大 8 MiB；读坏/超限明确失败。原子写在相同目录 staging，先文件 `sync_all`，rename 后同步父目录（Unix）。这不是多文件原子提交。
- PendingTransaction v1：operationId、canonical writeSet、dataDir、journalPath、preparedDigest。journal 必须位于 dataDir 内；准备日志先持久化，发布前检查 digest 未变。
- 索引位于固定协调目录 `pending/<operationId>.json`；严格 schema/身份验证。重叠包括同一路径及父子目录；损坏索引阻止新写入，不相交且可判定的未完成资源不阻止无关写入。
- 后续事务引擎需生成不可变准备日志和另行终态回执，核验实际资源后才能清除 marker；不能因锁已释放或 PID 消失清除。
- 操作/日志不含凭据、完整环境或用户技能正文；必要备份仅存本机受限目录，错误输出不回显解析原文。

## 安装与共享指令（待实现）

目标目录的 `.skills-linker-manifest.json` 是安装归属权威记录。新写入采用 version 2，记录 installationId、sourceId、relativePath、installName、mode、revision/local-live、contentDigest 和 managedResources；v1/TSV 只读识别，M4 验证归属并备份后显式升级。旧 Manager 不能继续写 v2，回退必须恢复备份。

SharedInstruction v1 放在固定协调目录，键为 canonical 指令文件路径与 marker；保存内容 digest 和安装使用者集合（含清单定位）。相同内容可共享，最后一个已核实使用者移除才删块；不同内容明确冲突。旧 marker 无使用者集合不得默认归当前安装。

远程来源快照固定 commit；dirty legacy cache 以内容 digest 单独标识，不能冒充上游 commit。刷新目录只更新 latestRevision，symlink 固定指向不可变路径。本地 live symlink 明示行为，不把其变化伪装成版本化安装。

## 前后端（待实现）

按计划第 4.2 节提供 `skills_` 命令族。每个命令实现与 `contracts/tauri-contract.json` 同轮落地；当前不提前注册不存在的 IPC。输入只用已登记身份/经后端验证的用户选取路径；写操作捕获项目与目标，不随 UI 当前项目变化。

错误形态为 `{code,message}`；当前 code 为 invalid_input/conflict/lock_busy/updating/recovery_required/permission_denied/io，来源与操作服务新增业务错误时同步契约。前端按 code 判断，不解析消息文字。

operationId 关联持久化进度、目标、终态及幂等请求；事件只是投影，订阅后补查快照。切换 Skills 不卸载 Chat，不创建隐藏 ChatSession 来运行翻译。

## 源功能去向与兼容基线

| 源能力/目录 | 去向 | 首次交付 |
|---|---|---|
| core 解析/稳定 ID、桌面扫描 | Rust catalog/source 模块，保留旧 ID alias | M1a |
| GitHub/GitLab HTTPS/SSH、默认分支 | 固定 revision 的来源与快照 | M1a |
| Web GitHub API 只读来源 | Rust API 读取与完整包物化 | M1b |
| Node/Rust/CLI 安装实现 | 共用 Rust 安装事务服务 | M2a/b/c |
| auto/global 注入、manual wrapper、doctor | 共享资源归属与指令服务 | M2b |
| skills-linker、tui、ng | Loom tools + 无窗口 CLI | M2c |
| 4 个已注册翻译 provider、复制/分享文本 | Skills UI 与后端任务 | M3 |
| app data、旧 cache/bundle links、JSON/TSV | 预览/备份/接管/恢复 | M4 |
| metadata/routing/archive/who-am-i 内容检查 | 保留在 skills 的独立 scripts/tests | M5 |
| Web/API/桌面旧壳与专属依赖 | 完整替代验收后移除 | M7 |

CLI 默认 symlink、桌面默认 copy；Codex/ChatGPT 目标去重；`--force` 不绕过未知归属删除保护，安全收紧需在新 CLI help/迁移说明中明确。完整参数/JSON 对照与 provider 实测仍待实施，不能用此表声称所有兼容项通过。
