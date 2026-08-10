# Loom Install / Uninstall Smoke Record

- **状态**: template; no notarized candidate pass yet
- **适用阶段**: 邀请制 Beta 前，同一候选 artifact 的安装、首次清理和卸载复核
- **关联流程**: `docs/release/macos-install.md`、`docs/release/local-data-and-uninstall.md`、`docs/release/beta-release-review-checklist.md`

## 目标

把安装说明和本地数据清理说明从“文档可读”推进到“同一候选 artifact 可复核”。这份记录不替代 `docs/release/notarized-dmg-gate.md`、`docs/release/beta-first-run-smoke-record.md` 或 `docs/release/diagnostic-bundle-smoke-record.md`；它只回答：试用者能否按文档安装、识别本地数据位置，并完成可解释的卸载 / 清理。

## 当前 2026-08-11 结论

- 当前没有针对已公证候选 DMG 的安装 / 卸载 pass 记录。
- `loom-beta-notary` credential 缺失期间，不要求试用者绕过 Gatekeeper，也不把维护者本机 release app smoke 当作普通试用者安装 pass。
- **Install / uninstall beta gate: Hold**

## 候选信息

```text
Record date:
Reviewer:
Candidate commit:
Candidate artifact:
Artifact checksum:
Artifact source:
macOS version:
Install path: DMG / app bundle / other
Gatekeeper state:
Related first-run smoke record:
Related diagnostic bundle smoke record:
```

## Path A：安装与首次打开

| Check | Expected evidence | Result | Notes |
| --- | --- | --- | --- |
| Artifact identity | Candidate commit、artifact 路径、checksum 与 release record 一致 | Wait | 不复用旧 hold artifact |
| Gatekeeper prompt | 记录首次打开是否有来源、签名、公证或权限提示 | Wait | 未公证候选不要求普通试用者绕过 |
| Install steps | 按 `docs/release/macos-install.md` 完成 DMG 拖拽或 app bundle 启动 | Wait | 记录是否需要 Finder 右键 Open |
| First project open | 只选择临时 / 低风险 Git 项目 | Wait | 不使用生产关键仓库 |
| Project-local data | 确认项目内 `.loom/` 与 `.gitignore` 行为符合文档 | Wait | 不提交 `.loom/` |
| Global data hint | 确认最近项目记录或 app data 说明可被试用者理解 | Wait | 不要求删除其他应用目录 |

## Path B：卸载与清理

| Check | Expected evidence | Result | Notes |
| --- | --- | --- | --- |
| App removed | `Loom.app` 已按安装方式删除或移入 Trash | Wait | 不删除其他 app |
| Project data decision | 对每个试用项目确认 `.loom/` 保留、备份或删除策略 | Wait | 若保留诊断证据，先人工脱敏 |
| `.gitignore` review | 确认 `/.loom/` 是否继续保留 | Wait | 只删除 Loom 唯一新增行 |
| Global data cleanup | 最近项目列表或全局 app data 按需清理 | Wait | 只处理属于 Loom 的目录 / 文件 |
| Git workspace check | 运行 `git status --short`，确认没有混入业务文件变化 | Wait | 记录异常 diff |
| Feedback evidence | 安装摩擦、Gatekeeper 提示和清理结果已写入反馈 | Wait | 不上传 token、`.env` 或私有日志 |

## Beta gate 判定

```text
Install path result: Pass / Blocked / Not checked
Uninstall cleanup result: Pass / Blocked / Not checked
Unexpected persistent data:
Unexpected git changes:
Tester-facing documentation gaps:
Install / uninstall beta gate: Hold / Pass
```

只有同一候选 artifact 的安装、首次打开、本地数据识别、卸载 / 清理和 git workspace 检查都为 `Pass`，且 notarized DMG gate、first-run smoke 与 diagnostic bundle smoke 已各自通过时，才允许把 `Install / uninstall beta gate` 从 `Hold` 改为 `Pass`。这份记录不能单独解锁邀请制 Beta 或公开分发。
