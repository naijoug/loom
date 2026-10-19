# macOS 应用内更新与 GitHub Releases

## 实现与分发边界

当前新增的更新服务面向 DMG / `.app` 安装的 macOS 客户端。默认本地配置没有公钥与更新源，界面明确显示不可用；不会请求占位地址。Web 预览不会请求原生更新。现有无 updater 的安装版需要先手动安装一次带更新能力的版本。

后台启动 30 秒后检查，此后每 6 小时检查。已有候选版本或已下载包时后台不覆盖它。发现新版后显示蓝色下载按钮，点击查看更新说明并下载；下载通过官方插件验签后才能安装。下载缓存仅在内存中，退出后需重新下载。用户点击“安装并重启”才安装，不自动停止任务、不自动强制重启。

安装前由 Rust 在同一把生命周期锁内判断全局空闲并关闭新执行入口。覆盖旧 Chat、规划/Review、命令/PTY、新 Workbench 的完整执行租约；运行中提示稍后重试。失败释放临时门禁。普通退出在安装期间被阻止；完成安装后的重启跳过不能异步等待的退出门。

新旧会话草稿和 Workbench 未保存笔记存于应用数据目录 `loom/updates/drafts.json`，由 Rust 原子写入。安装时再次同步保存，保存失败拒绝安装。设置表单和附件选择不属于此草稿存储。`loom/updates/history.json` 保留最近 100 条阶段记录，不记录包 URL、更新说明、凭据或原始网络错误。历史和草稿包含本地用户数据，不应直接公开。

本功能实现不改变 [Beta 发布门禁](beta-release-review-checklist.md) 的 Hold。没有两版真实签名/公证升级证据，不应标记分发通过。


## 已配置状态（2026-10-16）

用户明确授权复用旧 desktop-swift 项目的打包签名身份与配置。目标 GitHub 登录和仓库所有者均复核为 `naijoug`，写入目标仅为 `naijoug/loom`。

- 签名身份：`Developer ID Application: Honoululu Inc. (N7VU72TZB8)`。从本机钥匙串精确导出该身份，不批量导出其他身份；加密 PKCS12 已验证包含私钥，并匹配证书 SHA-1 指纹 `8FBAA8FA478BAA4F9DEFBBAA01844026CA43BE95`。
- 本机文件：`~/.tauri/loom-developer-id.p12` 和 `~/.tauri/loom-developer-id.password`，权限均为 0600。随机导出密码与旧项目无关；旧项目文件和钥匙串身份未修改。
- GitHub Variables：`APPLE_SIGNING_IDENTITY`、`APPLE_TEAM_ID`、`LOOM_RELEASE_REPOSITORY`、`LOOM_UPDATER_PUBLIC_KEY` 已配置。
- GitHub Secrets：`TAURI_SIGNING_PRIVATE_KEY`、`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`、`APPLE_CERTIFICATE`、`APPLE_CERTIFICATE_PASSWORD`、`APPLE_ID`、`APPLE_PASSWORD` 已配置。只验证名称存在，GitHub 不提供 Secret 值读回。
- 旧项目公证 profile 与源配置中的同团队 Apple ID / App 专用密码分别通过 Apple 认证。复用凭据建立并验证 `loom-beta-notary`；旧 profile 未更改，没有提交新的公证任务。
- 公证凭据与旧项目共用，后续撤销或轮换该 App 专用密码时需同步两个项目。证书包、密码和公证凭据均未写入代码仓库或聊天输出。

这些结果证明本地凭据可用和 CI 配置已写入，不能代替 GitHub runner 上的签名、公证和真实升级验收；当前没有触发 Release，分发 Hold 保持不变。

## 用户需要配置的 Variables

在 GitHub 源代码仓库 Settings → Secrets and variables → Actions 配置：

| 名称 | 内容 |
|---|---|
| `LOOM_UPDATER_PUBLIC_KEY` | Tauri signer 生成的 `.pub` 文件完整内容，不是路径。公开值，构建时写入客户端 |
| `LOOM_RELEASE_REPOSITORY` | 可选，公开发布仓库 `owner/repo`。默认当前仓库；代码仓库私有时必须指定独立公开发布仓库 |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: 姓名或组织 (TEAMID)` |
| `APPLE_TEAM_ID` | Apple Developer 团队 ID |

客户端固定通过 `https://github.com/<发布仓库>/releases/latest/download/latest.json` 获取 stable 更新。初次尚无公开 Release 时检查会失败，可稍后重试。草稿和预发布版本不作为 stable 更新发布。

## 用户需要配置的 Secrets

| 名称 | 内容 |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | Tauri updater 私钥文件的完整内容。必须妥善离线备份 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 上述私钥密码；无密码密钥可留空 |
| `APPLE_CERTIFICATE` | 包含 Developer ID Application 证书及私钥的 `.p12` 文件的 base64 内容 |
| `APPLE_CERTIFICATE_PASSWORD` | `.p12` 导出密码 |
| `APPLE_ID` | 用于公证的 Apple ID 邮箱 |
| `APPLE_PASSWORD` | 此 Apple ID 的 App 专用密码，不能使用账户登录密码 |
| `LOOM_RELEASE_TOKEN` | 仅跨仓库发布时需要，对发布仓库有 Contents read/write 权限的细粒度 token；同仓库默认用 `GITHUB_TOKEN` |

Tauri 更新签名和 Apple 签名是独立机制。更新私钥不要提交到 Git，也不要复制到聊天。先在自己的终端生成并保管：

```bash
pnpm tauri signer generate -w ~/.tauri/loom-updater.key
```

将生成的公钥填入 Variable，私钥和密码填入 Secrets。代码不读取本机已有私钥。首次公开发布后不要随意换公钥，否则旧客户端无法验证新包。

Apple 公证本流程采用 Apple ID + App 专用密码方式。若改用 App Store Connect API，需一起修改 workflow 的凭据预检和构建环境，不能只填 API Key 后保留当前预检。

参考：[Tauri Updater](https://v2.tauri.app/plugin/updater/)、[macOS 签名与公证](https://v2.tauri.app/distribute/sign/macos/)。

## 发布步骤

1. 同步 `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json` 中的版本，更新 `src-tauri/Cargo.lock` 的应用条目。只接受 `vX.Y.Z` stable 标签。
2. 完成常规代码 Review 与发布门禁；提交后推送对应标签。也可手动运行 `macOS Release` 并输入已存在标签。workflow 按标签检出，不将当前分支静默当作版本内容。
3. `verify` 执行版本与公钥预检和完整 `pnpm check`。
4. `build` 在 macOS runner 分别构建 Apple Silicon 和 Intel。合并配置开启 `createUpdaterArtifacts`；签名、公证并检查 `.app` 的严格 codesign、Gatekeeper 和 staple。
5. 两个架构都完成后，合并生成 `latest.json` 与 `SHA256SUMS`，再创建草稿 Release。先上传包和签名，最后上传清单。重复运行只允许覆盖草稿；已公开同标签版本拒绝覆盖。
6. 在草稿 Release 中补充实际更新说明；将相同说明写入 `latest.json` 的 `notes`。重新计算 `SHA256SUMS` 后替换清单和 checksum 文件，包与签名不变。
7. 下载草稿候选完成安装验证。需从旧版端到端验证更新时，使用独立公开测试发布仓库、同一公钥和相同构建流程；测试通过后才公开 stable Release。草稿本身无法供普通客户端匿名更新。
8. 完成当前候选的发布门禁后，人工发布 Release。旧客户端下次检查即发现新版。

发布产物：每个架构的 `.dmg`、`.app.tar.gz`、`.app.tar.gz.sig`，以及合并的 `latest.json`、`SHA256SUMS`。清单中的 `signature` 是签名文件内容，不是 URL。GitHub 发布 token 永远不会写入客户端。

本轮未启用新 Workbench 的生产 SDK 打包。Release 沿用现有默认产品入口；不能把更新流程当作新工作台完整可发布的证明。

## 本地构建配置

本地调试不需要任何签名配置。构建可更新的候选时，先设置上述公开变量和 `RELEASE_TAG`，执行：

```bash
node scripts/prepare-app-release.mjs
pnpm tauri build --config "$PWD/.tmp/tauri.updater.conf.json"
```

签名私钥、公证配置通过构建进程环境传入；`.tmp` 不入库。发布 workflow 自动完成这两步。默认 `tauri.conf.json` 保持无更新源，因此常规无签名开发构建仍能运行。

## 验证与回退

真实验收必须记录两个版本、commit、产物 SHA256、macOS 版本、架构及结果：无更新、新版提示、无 Content-Length 进度、断网重试、坏签名拒绝、重复点击、其他工作区运行任务、PTY 开启、失败后可继续执行任务、只读 DMG / App Translocation、安装目录权限不足、磁盘不足、安装重启、草稿/笔记/会话历史保留。浏览器或单元测试结果不能代替安装验收。

发行错误时先停止公开错误的更新清单；不要复用相同版本号或覆盖已发布签名包。恢复旧代码时发布一个更高版本号的修复版，保持默认不降级的版本比较。已升级用户的数据兼容性仍需按对应迁移规则验证。
