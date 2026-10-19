# M2 工具权限决策核心阶段记录

- 日期：2026-10-09，Asia/Shanghai。
- 基线：`74916a2` 后本切片。源码哈希与验证范围见 [证据](m2-tool-policy-evidence.json)。
- 范围：计划第 20 节的模式、规则合并、路径、Source 分类与运行桥接线；不是完整 M2 或桌面交付。

## 已实现

- `PermissionPolicy` 实现生产 `ToolPolicy`，接入已验证的 Bash AST 分类，覆盖当前 SDK 原生读/写/edit/bash 工具的 Explore、Ask、Execute 判断。未知工具缺少宿主注册的权限元数据时明确拒绝。
- 默认、工作区、活动 Source 的规则累加；应用规则中的普通 MCP 单词按完整动词分类，工作区正则保持配置含义。Source 正则在独立 Source 身份约束下匹配，不能用正则 `|` 泄漏到其他 Source。未激活 Source 的工具不会执行；激活交互仍待 Source 服务接入。
- API 的 GET 以及显式 method/path 规则可自动允许，其他方法在 Explore 拒绝、Ask 请求批准。MCP API 工具走同一端点判断。
- 明确写路径支持 `~`、`*`、`**`、`?`。按执行 cwd 解析相对路径，检查兄弟目录、`..`、符号链接和最近存在祖先；允许规则须同时匹配字面和实际目标，防止符号链接绕出。
- Explore 对宿主指定的会话 plans/data 路径有写入例外；Ask 不继承这一例外，但遵守明确的 allowedWritePaths。
- Guarded 接口区分未启用与单次失败：未启用按 Ask，已启用时内部文件写入自动允许、外部写入固定请求批准，其他变更经风险检查；无结果/错误按上游回退 Execute。此处测试使用模拟风险检查器，尚未接真实决策模型。
- 编译策略的完整 SHA-256 包含规则、活动 Source、路径和宿主元数据。新增可选 `RuntimeBinding.policySnapshotDigest`，生产策略在启动 worker 前及每次工具请求校验；租约不匹配不能执行。旧记录缺少该字段时仍保持原序列化格式。
- `sdk_runtime_probe` 的合成范围限制外层委托生产策略，不再自行固定返回 Ask；`run_session` 在创建 SDK 进程前验证上下文策略。

## 验证

| 验证 | 结果 |
| --- | --- |
| `pnpm check:core` | 通过：103 个 workspace 测试入口、fmt、Clippy、双协议漂移、协议 TS 编译。 |
| 上游 MCP 差分 | 执行固定上游纯分类函数生成 132 个用例，Rust 在 Explore/Ask 下逐项对照通过；覆盖 Source 名、子字符串、camelCase、读写混合动词。 |
| 原生模式/配置/路径/Guarded/快照 | 7 个 policy 测试入口；增加 1 个生产桥接测试，缺少策略快照时 worker 未启动、无工具副作用且失败持久化。 |
| Pi SDK + 生产运行桥 | 9/9，见 [结果](m2-tool-policy-fetch-results.json)。保留之前 6 个审批/认证/冻结上下文场景，新增 Explore 拒绝写、Execute 写、Ask allowlist 写；新增场景没有生成用户审批记录，均有自动策略审计。 |
| 前端投影 | 7/7，生成契约的可选新字段没有破坏重放。 |
| Tauri Workbench | fmt 检查、11 项测试通过；1 项原生 Keychain 测试按其要求未重跑（ignored）。 |

日志位于 `.tmp/tool-policy-validation/`。SDK 测试使用注入 fetch 的实际 Pi SDK 与临时文件，不涉及公网模型、真实 HTTP 或原生桌面操作。

## 差异与剩余工作

- Source 规则按身份隔离、写路径同时检查 realpath，是对上游文本前缀/字面路径漏洞的明确修正。此前 Shell 差异仍见 [Shell 记录](m2-shell-policy-progress.md)。
- 权限配置目前是 Rust 调用方提供的不可变对象；配置存储/API、完整快照持久化、UI 编辑、热更新与正在运行授权的撤销仍待接入，不能声称工作区设置已交付。
- Shell 向会话 plans/data 输出的重定向/heredoc 例外、管理员授权元数据、记住授权、完整诊断提示、认证/计划等交互还未完成。文件 Write/Edit 路径例外已有上述测试，不以此代替 Shell 例外。
- Session 工具须由真实 catalog 注入 safe 元数据；Source 传输/认证/激活、浏览器等还未接入，仅分类 API/MCP 不能证明这些工具可实际调用。
- 真正的 Guarded provider 和风险提示详情、动态功能开关/模式变更仍需接线。测试不代表决策模型判断质量。
- Windows/PowerShell、正式桌面调度、真实模型仍待完成。先前 HTTP 许可与 Mac 解锁待回复，不把旧 PoC 或无网络测试算作新原生验收。
