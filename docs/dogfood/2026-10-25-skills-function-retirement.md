# Skills 管理功能归属与源代码退役

用户最新范围：只迁移 Skills Manager 功能，原 skills 项目清理管理代码与残留；旧数据不迁移，M4 备份/暂存/恢复请求被替换。账号实测继续 blocked。Loom 起点 `b5eca66`，源 skills 起点 `0f864ee`。

## 已完成

- 源仓 `9167bb6 refactor: retire Skills Manager in favor of Loom` 删除 127 个 apps 文件及 9 个专属启动/预览/打包脚本，合计 136 个受管文件；包含 API/Web/Desktop/TUI、共享应用包、管理测试、pnpm workspace/lockfile、应用说明/历史计划及公共打包证书。
- 删除前逐文件核对消费者，保存并验证 Git bundle；剩余 15,842 个忽略文件均属于已确认的生成目录，整个生成应用目录移到仓库外可恢复位置。源仓不再有 apps 目录或 Manager 可执行入口。恢复目录及文件清单见 [结构化记录](samples/skills-migration/function-retirement.json)。Git 历史 `0f864ee` 仍是长期恢复基线；临时生成产物备份可能被系统清理。
- 原仓 README、AGENTS、scripts 说明改为内容仓库，质量入口保持 `scripts/skills-quality-check`；修复一处技能文档的旧应用计划参考链接。活动 Markdown 相对链接检查无失效项。
- Loom 撤下 M4-01 的模块、UI、CLI、IPC、专属测试和仅用于旧库导入的解析器；已提交 M4 计划/样本/截图保留为明确标记的历史证据。当前 Skills IPC 为 21 个，无 migration-preview / skills_plan_migration。
- Loom 保留来源、详情、copy/symlink 安装、显式更新/卸载、共享指令/wrapper、doctor、v2 事务备份恢复、CLI/TUI/ng、四 provider 翻译/设置、历史与命令面板。旧 JSON v1/TSV 与未知归属继续拒绝覆盖，文案改为只读而不是承诺未来数据迁移。
- 现行需求/契约/CLI 文档与计划索引同步范围。原 G1/G2 未通过项如实保留，不以源码退役冒充真实账号或桌面隔离验收。

## 验证

| 验证 | 结果与边界 |
|---|---|
| 源内容质量 | 原仓 32 项内容测试、326 项 routing perfect 管道用例通过；不调用模型 |
| 独立内容副本 | 不含 Manager 或 Loom；PATH 中 node/pnpm/cargo/tauri 为失败替身，内容检查仍通过 32/326 |
| 内容哈希 | 原 271 个受管技能/归档文件均保留，唯一变化是 inline-command-palette-minimum 的参考链接；全部归档字节不变 |
| 私人/未跟踪文件 | 旧 app data、`.skills-manager-data` 缓存、凭据、已安装技能不处理；原未跟踪 `.agents/` 与 skills-lock.json 哈希不变 |
| 源残留扫描 | 无活动 apps/、@skills-manager、原专属启动/打包入口引用；debug-loom 技能里的通用脚本示例是独立内容，未误删 |
| Loom 完整工作区 | `pnpm check` 前端 125/Rust 265 通过，25 ignored；包含 build/fmt/Clippy/文档检查 |
| 原生构建 | `pnpm tauri build --bundles app` 通过；源 Manager 已删除后构建，不依赖原 apps。未签名/公证/发布 |
| 视觉 | `pnpm smoke:visual` 通过：保留来源/安装/翻译/命令面板与 Chat 回归；没有迁移预览页 |
| 退出入口 | 实际随包 CLI help 不含 migration-preview/legacy-data-dir；调用旧命令失败为 Unknown command。canonical IPC 无旧迁移命令/样本 |
| 旧安装保护 | 原 JSON 拒绝接管测试扩展为 JSON v1 与 TSV 两个样本，显式 overwrite 也不改变原清单 |

本轮不调用真实 provider，也不重新尝试此前无法观察窗口的 sandbox-exec 桌面隔离；两项状态继续 blocked。独立 Web 产品按既定范围退役，管理功能由 Loom 桌面及 CLI 承载。

日志：`/tmp/skills-retirement-content-check.log`、`/tmp/skills-retirement-isolated-check.log`、`/tmp/loom-function-retirement-check.log`、`/tmp/loom-function-retirement-bundle.log`、`/tmp/loom-function-retirement-visual.log`。关键结果/删除清单另存 JSON。预览已按 debug.sh stop 清理。

本轮提交继续排除已有 Chat GFM、消息分隔、图标及原暂存的 markdown.ts 删除；不推送、不发布。撤回旧数据功能不影响已完成的 Loom v2 安装恢复能力。

## 提交树复核（2026-10-26 完成）

排除在途 Chat 改动的精确提交树：前端 119 项通过，build/fmt/Clippy/文档检查通过；Rust 初跑 262 项通过，1 个本地 Git fixture 出现 Failed/Committed 断言差异，定向复验 1/1 通过，合计 263 个用例，25 ignored。初次断言未打印具体错误，不能把它直接归因为此前的系统 EPERM。日志 `/tmp/loom-function-retirement-index/check.log` 与 `git-retry.log`。混合工作区的 125/265 完整检查独立通过，不与提交树计数混用。当前文档链接、源仓入口清理及原暂存状态核对通过。
