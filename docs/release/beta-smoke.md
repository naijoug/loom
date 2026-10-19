# Loom 邀请制 Beta 首次启动 Smoke

- **状态**: draft
- **适用阶段**: M2 安装、权限与卸载路径
- **关联计划**: `docs/plans/2026-08-07/10:00-beta-release-readiness.md`

## 目标

用一个 15 分钟、低风险、可复核的流程确认 Loom 在非作者设备上能完成基础启动、项目打开、命令验证、记录生成和反馈回收。这个 smoke 不要求真实 Agent 账号，也不要求修改生产项目。

## 非目标

- 不验证 Codex、Claude Code、OpenClaw、Hermes 等真实 Agent 的账号登录或模型质量。
- 不验证生产部署、数据库迁移、真实客户项目修复。
- 不要求公开签名、公证或自动更新。
- 不要求一次 smoke 覆盖完整端到端 dogfood；完整回归仍以 `docs/testing.md` 和 `pnpm check` 为准。

## 准备临时项目

在任意低风险目录创建一个临时 Git 项目，示例结构如下：

```bash
mkdir -p loom-beta-smoke
cd loom-beta-smoke
git init
printf '# Loom Beta Smoke\n' > README.md
printf 'console.log("loom beta smoke ok")\n' > smoke.js
printf '{"scripts":{"test":"node smoke.js"}}\n' > package.json
git add README.md smoke.js package.json
git commit -m 'seed loom beta smoke fixture'
```

如果本机没有 Node，可以把验证命令换成：

```bash
git status --short
```

## Smoke 步骤

1. 按 `docs/release/macos-install.md` 启动 Loom。
2. 打开刚创建的 `loom-beta-smoke` 临时项目。
3. 确认项目根目录出现 `.loom/`，且 `.gitignore` 包含 `/.loom/`。
4. 在 Settings → Agents 中先不要配置真实付费 Agent；如果 UI 要求命令，可配置一个只输出文本的本地命令作为占位，并在反馈中说明。
5. 创建一个任务，需求写为：

   ```text
   验证这个临时项目的测试命令能运行，并记录结果；不要修改 README.md、smoke.js 或 package.json。
   ```

6. 在 Testing / Validation 阶段配置低风险命令：
   - Node 可用时：`npm test`
   - Node 不可用时：`git status --short`
7. 运行验证命令，记录是否能看到实时输出、退出码和运行历史。
8. 如果 Loom 支持导出总结或诊断包，导出后先人工打开检查；不要发送包含私密路径、token、账号或其他项目内容的文件。
9. 退出并重新打开 Loom，确认最近项目或任务历史是否仍可查看。
10. 按 `docs/release/beta-feedback-template.md` 回传结果。

## 通过标准

一次 smoke 通过需要同时满足：

- Loom 能启动并打开临时项目。
- `.loom/` 被创建在临时项目内，且没有要求访问不相关目录。
- 至少一个低风险验证命令能运行并显示结果。
- 退出重启后，最近项目或任务记录没有明显损坏。
- 试用者能找到本地记录，并知道按 `docs/release/local-data-and-uninstall.md` 清理。
- 反馈中包含 Loom 版本、macOS 版本、芯片架构、安装方式、验证命令和结果。

## 失败分类

| 现象 | 反馈分类 | 需要证据 |
|---|---|---|
| App 无法安装或无法首次启动 | `blocker` | macOS 版本、芯片架构、Gatekeeper 提示截图、产物来源 |
| 打开临时项目失败或 `.loom/` 无法创建 | `blocker` / `bug` | 项目是否 Git repo、目录权限、错误提示 |
| 验证命令不显示输出或无法停止 | `bug` | 命令、退出码、日志截图、是否可复现 |
| 不理解某一步 UI 或安全提示 | `confusion` | 卡住位置、期望文案、实际文案截图 |
| 诊断包或日志疑似包含敏感信息 | `security` | 不要直接外发原文件；先描述字段名和出现位置 |
| 希望支持更多 Agent、平台或任务类型 | `request` | 使用场景、优先级、是否阻塞本轮 smoke |

## 回收与清理

Smoke 完成后：

1. 如果不再需要临时项目，可以删除整个 `loom-beta-smoke` 目录。
2. 如果要保留项目，至少确认 `.loom/` 不会被提交。
3. 如需完全卸载 Loom，按 `docs/release/local-data-and-uninstall.md` 清理 App、本地项目数据和全局最近项目记录。
