# Loom 个人 dogfood 到公开 Beta 发布准备 — Plan

- **Date**: 2026-08-07
- **Author**: Hermes
- **Status**: proposed
- **Scope**: 在不扩张核心产品功能的前提下，把 `docs/dogfood/project-optimization-2026-08-05.md` 中已完成的个人稳定版，拆成可以判断是否进入公开 Beta 的发布准备清单。

## 目标

把“本机可用、个人 dogfood 稳定”推进到“可以小范围发给外部用户试用”的可审查状态。重点不是新增功能，而是补齐发布边界、安装/升级、隐私披露、故障回收和反馈闭环。

## 非目标

- 不新增插件市场、云同步、团队账号、远程协作或浏览器自动化。
- 不承诺 Windows/Linux 同日公开发布；多平台只做决策门禁和后续矩阵。
- 不把真实付费 Agent canary 放入默认 CI；仍需手工触发，避免额度和账号风险。
- 不在未完成签名/公证/隐私说明前公开分发 DMG。

## 成功标准

- 发布决策能回答：谁可以试用、安装包来自哪里、用户会授权哪些本机能力、出错如何回收、反馈如何进入下一轮。
- DMG / app 产物有版本号、commit、checksum、构建环境和验证记录。
- macOS 安装、首次启动、选择项目、创建任务、跑一次无真实 Agent 的确定性 smoke、导出诊断包、卸载/清理缓存都有文档化步骤。
- 隐私和安全说明覆盖：本机项目读取、命令执行、日志存储、诊断包脱敏、外部 LLM/CLI 账号责任边界。
- Beta 反馈表能把问题归类为安装阻塞、权限/安全疑虑、Agent 调用失败、UI 理解困难、性能问题或需求扩张，并明确每类的证据字段。

## 当前状态

- `docs/plans/2026-08-05/11:14-project-optimization.md` 已完成 M0–M5，记录了稳定化分支、契约、进程监督、任务仓储、前端拆分、分层测试、中文默认 UI、诊断包和性能预算。
- `docs/dogfood/project-optimization-2026-08-05.md` 已记录 `pnpm check`、browser smoke、interaction smoke、visual smoke、bench、Tauri build 和 desktop smoke 均通过。
- 尚未完成 Apple Developer ID 签名、公证、公开分发页面、多平台矩阵和外部用户隐私/安全说明。

## 里程碑

### M0 — 发布对象与风险边界

**Outcome**: 明确这是“邀请制 Beta”还是“公开下载”，避免未准备好的支持成本。

| # | Task | Files / Output | Verification |
|---|---|---|---|
| 0.1 | 写一页 Beta 范围说明：目标用户、支持平台、已知限制、不会做的事 | `docs/release/beta-scope.md` | 说明中必须列出非目标和退出条件 |
| 0.2 | 明确外部用户试用协议：不要用于生产关键仓库、不要粘贴机密、真实 Agent 账号由用户自担 | `docs/release/beta-safety-notes.md` | 安全说明能被 README / 发布页链接到 |
| 0.3 | 建立反馈分类字段 | `docs/release/beta-feedback-template.md` | 每个问题必须能落到 blocker / bug / confusion / request / security 中一种 |

### M1 — macOS 产物可复现

**Outcome**: 当前 arm64 macOS 包可以从指定 commit 重新构建，并能让试用者校验来源。

| # | Task | Files / Output | Verification |
|---|---|---|---|
| 1.1 | 固定 release commit、版本号、Node/Rust/Tauri 环境 | release record | `git rev-parse HEAD`、`node -v`、`rustc -V`、`pnpm -v` 记录完整 |
| 1.2 | 从 clean checkout 运行 release build | DMG / app | `pnpm check`、`pnpm tauri build` 通过 |
| 1.3 | 生成 checksum 与桌面启动记录 | release record | `shasum -a 256`、`pnpm smoke:desktop` 或等价桌面启动脚本通过 |

### M2 — 安装、权限与卸载路径

**Outcome**: 试用者知道如何安装、首次授权、遇到 Gatekeeper/权限问题如何处理，以及如何清理本地数据。

| # | Task | Files / Output | Verification |
|---|---|---|---|
| 2.1 | 写 macOS 安装说明和 Gatekeeper 提示 | `docs/release/macos-install.md` | 从空白用户视角复核步骤可执行 |
| 2.2 | 写本地数据位置与卸载/清理说明 | `docs/release/local-data-and-uninstall.md` | 列出配置、任务历史、日志和诊断包位置 |
| 2.3 | 写首次启动 smoke：创建临时项目、跑无外部账号路径、导出诊断包 | `docs/release/beta-smoke.md` | 步骤能在无真实 Agent 凭据时完成 |

### M3 — 隐私、安全与诊断包披露

**Outcome**: 用户知道 Loom 会读取/写入什么，诊断包默认不包含什么，何时会调用外部 CLI/模型。

| # | Task | Files / Output | Verification |
|---|---|---|---|
| 3.1 | 写隐私说明初稿 | `docs/release/privacy-note.md` | 覆盖项目路径脱敏、日志默认不导出、用户选择日志时的限制 |
| 3.2 | 写 Agent/CLI 账号责任边界 | `docs/release/agent-account-boundary.md` | 明确 Codex/Claude/custom CLI 的账号、额度、数据发送由用户配置决定 |
| 3.3 | 用 secret fixture 复核诊断包脱敏 | release evidence | 运行现有诊断/测试命令并保存输出摘要 |

### M4 — 小范围试用与回收机制

**Outcome**: 外部试用不是“扔一个包出去”，而是有反馈入口、回滚策略和下一轮节奏。

| # | Task | Files / Output | Verification |
|---|---|---|---|
| 4.1 | 准备 3–5 人邀请制 dogfood 清单与任务脚本 | private release checklist | 每位试用者只要求完成同一个 15 分钟 smoke |
| 4.2 | 建立反馈 triage 表 | issue / table template | 每条反馈有复现步骤、诊断包、截图/日志和严重度 |
| 4.3 | 设定 Beta 停止线 | release gate | 出现权限误操作、未脱敏泄露、安装不可恢复、数据迁移损坏即停止分发 |

## 风险

| Risk | Impact | Mitigation |
|---|---|---|
| 未签名 app 造成安装摩擦 | 高 | 邀请制先说明 Gatekeeper 步骤；公开 Beta 前优先做 Developer ID 签名/公证 |
| 用户误把 Loom 用在生产关键仓库 | 高 | Beta scope 和安全说明中明确只建议临时/非关键项目试用 |
| 真实 Agent 调用消耗额度或泄露上下文 | 高 | 默认 smoke 使用无凭据确定性路径；真实 Agent canary 必须用户手工确认 |
| 诊断包包含敏感路径或日志 | 高 | 默认不导出日志；复跑 secret fixture；发布说明要求用户发送前自查 |
| 支持范围膨胀 | 中 | M0 明确不支持项；反馈分类中把需求扩张和 bug 分开 |

## 待确认问题

- Beta 是仅自己设备的“发布演练”，还是发给 3–5 个外部试用者？
- 是否已有 Apple Developer ID；如果没有，公开下载页必须先解释未签名限制或延后。
- 是否需要单独做 x64 macOS 包，还是首轮只支持 arm64。
- 反馈入口使用 GitHub issue、表单、邮件还是私聊收集。

## 下一步建议

下一轮优先完成 M0.1–M0.3：新增 `docs/release/` 下的 Beta scope、安全说明和反馈模板。它们是低风险文档切片，但会决定后续是否值得投入签名、公证和分发页面。验证时只需检查相对路径、链接和 `git diff --check`，不要先改发布构建脚本。
