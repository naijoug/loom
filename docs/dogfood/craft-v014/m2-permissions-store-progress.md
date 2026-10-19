# M2 权限配置持久化与策略审计记录

- 日期：2026-10-09，Asia/Shanghai。
- 基线：`7512157` 后本切片；源码/日志哈希见 [证据](m2-permissions-store-evidence.json)。
- 范围：计划第 22 节。配置保存、下回合应用和策略审计已接线；外部文件热更新/授权重评及完整 UI 对齐仍未完成。

## 已实现

- Core catalog 追加保存应用、工作区和 Source 权限文档，带 revision、operationId、作用域与更新时间。重复请求幂等，旧版本编辑冲突失败，null 表示恢复默认；存储失败不发布成功。
- `workbench_save_permissions` 经可信主窗口与生产 PermissionPolicy 编译校验后保存；Core wire 仅提供读取，不暴露绕过校验的权限写入。命令契约、生成 TS 与前端客户端已同步。
- SDK 宿主按作用域读取配置：默认/工作区叠加，只有已启用 Source 的文档参与；下一回合自动读取最新版本。运行中回合保持冻结，待批准请求不会被新增 allowlist 静默改为允许。
- 每个 SDK 回合启动前保存完整规则、实际 bundled defaults、路径与宿主元数据的策略快照；摘要绑定 RuntimeBinding。`read_policy_snapshot` 只能按真实会话/回合读取其绑定快照，并校验 hash/大小/文件类型/链接边界。不能只凭摘要声称可以回溯策略内容。
- 新权限设置入口支持读取应用默认/工作区/已有 Source 文档、JSON 编辑、校验错误、版本冲突后重新加载、幂等重试、重置。保存后明确说明下一回合生效。

## 验证证据

| 检查 | 结果 |
| --- | --- |
| `pnpm check:core` | 109 项 workspace 测试入口、fmt、Clippy、协议漂移和协议 TS 编译通过。 |
| 新 Core 存储测试 | 5 项覆盖作用域、CAS、幂等/重置/重启、非法/超大配置、写失败不发布、快照篡改/隔离/链接替换。 |
| Tauri Workbench | 19 项通过，含宿主拒绝非法正则、活动 Source 过滤、当前回合冻结与下一回合规则实际改变工具副作用、快照重启读取。 |
| Tauri 契约 | 6 项通过；修正初次检查暴露的 canonical manifest 漂移。 |
| 真正 Pi SDK 经 Host | 单独执行 1 项测试内 3 模式通过，使用注入 fetch、临时文件与无认证配置。 |
| 前端 | 权限编辑器无效 JSON、保存失败重试身份、版本、作用域切换与重置通过；客户端命令与 scope 身份校验、事件投影及 SDK/会话回归通过，数量见证据。 |
| 构建/静态检查 | 前端 build、Tauri Clippy/fmt、debug app bundle 构建通过。 |
| 原生窗口 | 下述真实操作通过；与 SDK 宿主测试分别记录，不混为真实模型账户验收。 |

## 原生操作

通过 `scripts/debug.sh` 启动构建的 Loom.app，使用 `.tmp/permissions-native-data` 独立数据和 fixture，无真实模型请求。macOS 原生 UI 本轮已可操作，旧锁屏阻塞解除。

1. 打开“权限设置”，输入非法 Bash 正则 `[` 并保存：宿主错误展示，未产生权限保存事件。
2. 保存一条有效测试配置，带 `native-qa` 注释：显示已保存、版本 1；未扩大默认权限。
3. 原生截图发现编辑弹窗受通用 max-width 限制，修正到 760 CSS px 上限并重建。
4. 正常停止、使用相同隔离目录重启：显示 `native-qa` 和版本 1，截图确认布局完整。
5. 恢复当前范围默认规则：显示 `{}` 和版本 2，catalog 确认第二个事件 config=null。
6. 首轮测试输入中的 JSON 转义产生了退格字符；改用无歧义的精确只读正则 `^ls$` 再验，版本 3 保存/重启读取后恢复为版本 4。原事件保留，不覆盖历史。
7. 执行 `scripts/debug.sh stop`，进程句柄返回 exit 0。

[原生事件与观察记录](m2-permissions-native-results.json)。截图通过 CUA 当场检查，未另存截图文件；不把文字记录称作录屏。完整输出在 `.tmp/permissions-validation/`。

## 未完成范围

- 外部 permissions.json 编辑、watcher、逐工具热更新及待批准授权重评还未接入；下一回合应用不是热更新完成证明。
- 当前设置是功能接线用 JSON 编辑页；上游规则表格、EditPopover/Agent 自配置、完整导航和视觉一比一仍属后续 UI 交付。
- Source 创建/认证/激活流程尚未完成，页面只能编辑已有 Source 权限文档；不以分类与配置保存冒充可调用的 Source。
- 管理员/记住授权、Shell 会话目录例外、真实 Guarded provider、配置快照回收继续；启动前取消可能留下未绑定快照，暂不自动删除。
- 本轮原生只验收权限页面。此前连接 UI、SDK 原生发送/停止等仍需专门验收，不能因解锁就写通过。
- 实际 HTTP 与 Claude 新 worker 验收仍待先前明确许可；无网络 Pi 测试不抵扣。发行资源/签名、其他 M2–M8 能力继续。
