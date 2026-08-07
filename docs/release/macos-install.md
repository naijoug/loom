# Loom macOS Beta 安装说明

- **状态**: draft
- **适用阶段**: M2 安装、权限与卸载路径
- **关联计划**: `docs/plans/2026-08-07/10:00-beta-release-readiness.md`

## 适用对象

这份说明只面向邀请制 Beta 试用者。首轮默认平台是 macOS arm64，产物可能尚未完成 Apple Developer ID 签名和公证，因此不应把它当成公开稳定版下载包。

如果你不熟悉 Finder、Terminal、Gatekeeper 或 Git，请先不要独立试用，改由熟悉 macOS 开发环境的人陪同完成。

## 安装前检查

1. 确认试用任务只会使用临时项目、示例项目或低风险个人项目。
2. 确认本机已有至少一个可测试的安全命令，例如 `git status`、`npm test`、`pnpm test` 或一个只输出文本的脚本。
3. 如果要配置真实 Agent CLI，先确认对应账号、额度和数据发送边界；无凭据 smoke 请优先使用 `docs/release/beta-smoke.md`。
4. 下载或接收产物后，记录提供方给出的版本、commit 和 checksum；如果没有这些信息，不要继续扩大试用范围。

## 安装方式 A：DMG

1. 打开提供方给出的 `Loom_0.1.0_aarch64.dmg`。
2. 将 `Loom.app` 拖入 `Applications`。
3. 第一次打开时，如果 macOS 提示来源未知或无法验证开发者，先不要反复双击。
4. 在 Finder 中右键 `Loom.app`，选择 **Open / 打开**，阅读系统提示后再确认。
5. 如果仍被 Gatekeeper 阻止，打开 **System Settings → Privacy & Security**，在最近被阻止的应用提示中选择允许打开。

## 安装方式 B：直接运行 App bundle

仅在试用者明确知道产物来源时使用：

1. 解压或定位提供方给出的 `Loom.app`。
2. 将它放在临时目录或 `Applications` 中。
3. 右键选择 **Open / 打开**，不要用脚本批量移除安全属性。
4. 记录首次启动是否出现 Gatekeeper、文件访问、网络或辅助权限提示。

## 首次启动检查

1. 启动 Loom 后，先不要选择生产关键仓库。
2. 选择一个临时项目，确认 Loom 会在项目内创建 `.loom/` 并把 `/.loom/` 加入该项目 `.gitignore`。
3. 打开 Settings → Agents，仅配置你愿意用于本轮试用的命令。
4. 如果只做无凭据 smoke，可以暂时不配置真实 Agent，按 `docs/release/beta-smoke.md` 走确定性路径。
5. 完成试用后按 `docs/release/beta-feedback-template.md` 反馈版本、系统、安装方式、阻塞点和证据。

## Gatekeeper 处理原则

- 邀请制 Beta 可以接受一次性手工允许打开，但必须记录操作步骤和系统提示。
- 不建议试用者执行来源不明的 `xattr -dr com.apple.quarantine ...` 命令。
- 如果提供方要求使用 `xattr`，必须同时给出完整产物来源、checksum、版本和 commit；试用者仍应优先选择不含机密的低风险项目。
- 两名以上目标试用者遇到不可恢复的安装或启动失败时，应停止继续分发，先修复安装路径或补齐签名/公证。

## 安装完成标准

安装只算完成于以下条件同时满足：

- Loom 能启动到主界面。
- 试用者知道本轮只支持低风险项目。
- 试用者能指出本机项目数据写入 `.loom/`，全局最近项目记录写入 Tauri app data 目录。
- 试用者知道如何按 `docs/release/local-data-and-uninstall.md` 清理。
- 如果发生 Gatekeeper 或权限摩擦，反馈中已记录系统提示、处理方式和结果。
