# Loom Beta Signing Gate Decision

- **状态**: decision recorded for current local candidate
- **适用阶段**: M1 产物完整性复核后的分发决策
- **关联记录**: `docs/release/artifact-integrity-check.md`

## 结论

当前 `Loom_0.1.0_aarch64.dmg` 不进入公开 Beta，也不作为默认邀请制 Beta 分发物。

原因不是 DMG 损坏：`docs/release/artifact-integrity-check.md` 已确认 checksum、只读挂载和 bundle 元数据通过。阻塞点是当前 app bundle 的签名状态不可信；本决策把它标记为 **Gatekeeper hold**：`codesign --verify --deep --strict --verbose=2` 与 `spctl --assess --type execute --verbose=4` 均返回 `code has no resources but signature indicates they must be present`。

因此本轮决策是：

1. **不把现有 DMG 当作公开下载包**。
2. **不要求试用者绕过 Gatekeeper 来验证产品价值**。
3. **允许维护者继续把它作为本机验证 artifact**，用于检查 checksum、bundle 元数据、文档路径和后续签名修复。
4. **邀请制 Beta 的下一份候选产物必须先解决签名 gate**，再进入首次启动 smoke 或更大范围分发。

## 为什么不接受 unsigned / invalid-signature 邀请制分发

邀请制 Beta 理论上可以接受一次性 Gatekeeper 手工允许打开，但当前状态不是清晰的“未签名但来源可信”，而是一个已有签名元数据却无法通过严格验证的状态。

这会带来三个问题：

- **试用者解释成本高**：很难让非维护者判断这是预期的 unsigned Beta，还是构建/打包过程损坏。
- **反馈噪声高**：安装失败、右键打开失败、系统设置允许失败会淹没真正的产品体验反馈。
- **安全边界不清**：Loom 会编排本地命令和 Agent CLI，安装入口本身必须比普通内容型 App 更保守。

## 当前证据

| Check | Result | Action |
| --- | --- | --- |
| DMG checksum | pass | 保留为本机候选构建证据 |
| Read-only mount | pass | 可继续用于 artifact inspection |
| Bundle identifier / version | pass | 与 `docs/release/release-build-record.md` 对齐 |
| `codesign --verify --deep --strict --verbose=2` | hold | 必须修复或重签 |
| `spctl --assess --type execute --verbose=4` | hold | 必须修复并重新评估 |
| Developer ID identity availability | observed | 本机 keychain 可见 `Developer ID Application` 身份，但尚未验证 Tauri 签名与公证流程 |
| Notarization credential | unknown | 不假设可用；需要单独验证 |

## 下一步修复路径

### Path A：修复签名并重新打包（优先）

1. 检查 `src-tauri/tauri.conf.json` 与 Tauri v2 macOS signing / notarization 配置。
2. 用明确的 Developer ID Application identity 重新构建或重签 app bundle。
3. 重新生成 DMG，而不是只修改已记录 checksum 的旧 artifact。
4. 重新记录：
   - commit
   - build command
   - signing identity label
   - notarization setting / result
   - DMG SHA-256
   - file size
5. 重跑：
   - `hdiutil verify src-tauri/target/release/bundle/dmg/Loom_0.1.0_aarch64.dmg`
   - `codesign --verify --deep --strict --verbose=2 <mounted Loom.app>`
   - `spctl --assess --type execute --verbose=4 <mounted Loom.app>`
6. 只有上述 gate 通过后，才执行 `docs/release/beta-smoke.md` 和 `docs/release/diagnostic-bundle-smoke.md`。

### Path B：只做维护者本机验证（当前可接受）

如果暂时不处理签名，则只能继续做不扩大分发的本机验证：

- 继续验证 release 文档链接、安装说明、反馈模板和诊断包 smoke 模板。
- 不把 DMG 发给新试用者。
- 不要求试用者运行 `xattr` 或绕过 Gatekeeper。
- 下一次构建前保留当前 `docs/release/artifact-integrity-check.md` 作为失败证据，而不是覆盖它。

### Path C：明确接受 unsigned 分发（本轮不选择）

如果未来确实要接受 unsigned 邀请制分发，必须另写一份试用者可读说明，并满足以下最低条件：

- `codesign --verify` 对当前 bundle 状态的解释足够清楚。
- `docs/release/macos-install.md` 明确列出 Gatekeeper 预期提示和不建议执行的命令。
- 提供 commit、checksum、文件大小、下载来源和撤回机制。
- 首批试用者必须是熟悉 macOS 安全提示的开发者。

在这些条件满足前，默认仍按 Path A / Path B 执行。

## Release README 更新规则

- `docs/release/README.md` 必须把签名 gate 放在首次启动 smoke 之前。
- `docs/release/macos-install.md` 必须提示当前候选 DMG 处于 signing hold，不应作为普通试用包安装。
- 下一次如果签名修复成功，应新增新的 artifact integrity record 或在现有文档中明确追加新的时间段记录，不要删除当前 hold 证据。
