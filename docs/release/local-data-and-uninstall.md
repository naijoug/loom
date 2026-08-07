# Loom 本地数据与卸载清理说明

- **状态**: draft
- **适用阶段**: M2 安装、权限与卸载路径
- **关联计划**: `docs/plans/2026-08-07/10:00-beta-release-readiness.md`

## 一句话边界

Loom 是本地优先的桌面应用，但它会在被选择的项目内写入任务、日志、计划和附件副本，也会在系统 app data 目录记录最近项目列表。卸载 App bundle 不等于删除这些本地数据。

## 项目内数据

当你在 Loom 中打开一个项目时，Loom 会在该项目根目录下创建或使用：

| 路径 | 用途 | 清理建议 |
|---|---|---|
| `.loom/loom.json` | 项目元数据、schema 版本、项目 id 和项目名 | 删除后下次打开会重新生成 |
| `.loom/tasks/` | 任务状态、Planning / Review / Testing 记录、总结导出源 | 需要保留审计记录时先备份；完全卸载时可删除 |
| `.loom/logs/` | command runner / Agent run 的 stdout、stderr 或尾部日志 | 发送反馈前必须人工脱敏；完全卸载时可删除 |
| `.loom/planning/` | 多 Agent 计划、互评和综合计划的证据文件 | 若包含项目上下文，按敏感材料处理 |
| `.loom/terminal-slots.json` | 项目级测试/预览命令槽位 | 删除后需要重新配置 |
| `.gitignore` 中的 `/.loom/` | 避免把 Loom 本地记录误提交进项目仓库 | 卸载 Loom 后通常可以保留，也可以人工移除 |

## 全局 App 数据

macOS 上 Tauri 会为 `com.naijoug.loom` 分配应用数据目录。Loom 当前会在 app data 目录下的 `loom/` 子目录保存全局配置，例如：

| 路径模式 | 用途 |
|---|---|
| `~/Library/Application Support/com.naijoug.loom/loom/recent-projects.json` | 最近打开的项目列表 |

不同 macOS / Tauri 版本可能调整 app data 基础目录。若上述路径不存在，可在 Finder 中搜索 `com.naijoug.loom` 或 `recent-projects.json`，但不要删除其他应用目录。

## 卸载 App

1. 退出 Loom。
2. 如果是 DMG 安装，将 `Applications/Loom.app` 移到 Trash。
3. 如果是临时目录运行，删除对应的 `Loom.app`。
4. 清空 Trash 前确认没有误删其他应用。

## 清理项目内数据

对每个试用过的低风险项目执行人工检查：

1. 打开项目根目录。
2. 确认 `.loom/` 只包含 Loom 试用记录，没有你要保留的诊断证据。
3. 如需清理，删除 `.loom/`。
4. 检查 `.gitignore` 是否包含 `/.loom/`：
   - 如果该项目未来仍可能使用 Loom，保留这一行。
   - 如果要完全恢复试用前状态，并且这一行是 Loom 唯一新增内容，可以人工删除。
5. 运行 `git status --short`，确认没有把 `.loom/` 或 `.gitignore` 的意外变化混入业务提交。

## 清理全局数据

1. 退出 Loom。
2. 删除 `~/Library/Application Support/com.naijoug.loom/loom/` 中确认属于 Loom 的文件。
3. 如果只想清除最近项目列表，删除 `recent-projects.json` 即可。
4. 重新启动 Loom 后，如果最近项目列表为空，说明全局记录已清理。

## 诊断包处理

- 诊断包或日志片段可能包含项目路径、命令、文件名、测试输出、Agent 输出和人工反馈。
- 发送给维护者前，必须人工检查并移除 token、密钥、客户名称、私有路径、`.env` 内容和商业代码片段。
- 如果发现诊断包默认包含未脱敏敏感信息，应按 `docs/release/beta-feedback-template.md` 归类为 `security`，并停止继续外发该诊断包。

## 完全清理完成标准

- `Loom.app` 已删除。
- 所有参与试用的项目中，不再有需要清理的 `.loom/` 目录。
- 最近项目列表或全局 app data 已按需删除。
- `git status --short` 没有出现因清理导致的意外业务文件变化。
- 仍需保留的反馈证据已经复制到单独位置并人工脱敏。
