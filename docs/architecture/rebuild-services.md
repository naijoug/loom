# 重建的外部服务与功能开关清单

- 基线：Craft v0.14.0 / `73bd9c2a3573158bea880984eb8d5fdb41e0cac2`。
- 状态：M0 源码核对；没有调用下列外部写入接口，也没有接入真实账户。
- 目标：保留上游用户体验，同时使用 Loom 自己的配置、品牌和服务。桌面本地能力不依赖分享服务可用。

路径以下游 clone 根 `.ref/repos/craft-agents-oss/` 为相对起点。

| 依赖 | 已确认的客户端入口/行为 | OSS 内可用部分 | Loom 落地工作与验收 |
| --- | --- | --- | --- |
| 会话公开分享 | `packages/server-core/src/sessions/SessionManager.ts` 的 shareToViewer/updateShare/revokeShare；向 `VIEWER_URL/s/api` POST、向 `/s/api/{id}` PUT/DELETE | `apps/viewer`、客户端上传和本地分享状态；当前导出仓库无对应托管应用目录 | Rust HTTP 服务 + 对象存储接口 + Viewer；独立管理 token；创建/更新/撤销/过大请求/失败回滚。不能直接沿用上游 URL。 |
| Page 发布 | `packages/shared/src/pages/publisher.ts`；默认 `https://thecraftagents.com/p/api`，支持 `CRAFT_PAGES_SHARE_API_URL`；multipart bundle、密码、revision、adminToken | bundle、客户端 publisher 和 tokenStore；根 scripts 指向 `workers/pages`，目录不在当前 OSS 导出 | Rust 发布服务实现相容行为，凭据 vault 保存管理 token；匿名访问、密码保护、内容更新、撤销和未知提交对账。 |
| OAuth relay | `packages/shared/src/auth/oauth-relay.ts`；`https://thecraftagents.com/auth/callback` 包装 state/returnTo | 客户端 state envelope、provider OAuth handlers | 独立 HTTPS callback/本机回调路由；校验 state、PKCE、returnTo allowlist 和单次使用；逐 provider 真实登录/取消/过期验收。不能假定有上游 OAuth app 配额或凭据。 |
| 自动更新 | `apps/electron/src/main/auto-update.ts`；上游 Electron 更新服务 `https://thecraftagents.com/electron/latest` | Electron 更新客户端/配置 | Tauri updater 对接独立签名 manifest 和产物；禁止把 Electron 更新包作为 Tauri 更新。分别验签、版本、回退、失败恢复。 |
| 诊断/遥测 | `apps/electron/src/main/index.ts` 的 Sentry 初始化由构建时 `SENTRY_ELECTRON_INGEST_URL` 控制 | 采集配置和 UI 诊断逻辑 | 默认不携带上游 DSN；Loom 独立策略及显式诊断导出，敏感内容脱敏；用户可见错误和日志不依赖遥测。 |
| 开发者反馈 | session tool handler 委派 `ctx.submitFeedback`；`agent/claude-context.ts` 与 session-mcp-server 提供实现 | 功能受开发开关控制，源码 handler 的“sent”文案本身不是外发证明 | 核对实际落盘/交付路径后迁移；使用 Loom 目的地，只在用户请求/允许的场景执行。不能将 tool 名字视为无条件外发授权。 |
| Decision model | `shared/src/decisions/providers.ts`：TypeSafe/OpenRouter/Vercel gateway/Laya/custom | 请求/结果、记录、开关、失败回退 | 自带账户或本地 endpoint；只复制协议/产品能力，不承诺第三方额度。每个功能开关分别验收；模型判断不能提升权限。 |
| 模型 provider | Claude/Pi SDK + `shared/config/llm-connections.ts` 的 3 provider types / 8 auth types | SDK 接入、模型配置和 OAuth/云凭据流 | Rust vault + 最小 SDK worker；各认证/模型能力真实核验。M0 的本地模拟模型成功不代表真实认证通过。 |
| Messaging | `packages/messaging-gateway` + WhatsApp worker | Telegram/WhatsApp/Lark adapters、binding/准入/回传 | 专用测试账户、连接状态、消息去重、权限与生命周期；不使用私人会话发送验收消息。 |

## 条件可见性

从 `packages/shared/src/feature-flags.ts` 确认：

- `fastMode` 默认 false；不能因有代码就把默认 UI 展示为开启。
- developerFeedback 仅开发环境默认开启，可由环境变量覆盖。
- craftAgentsCli 指导/guardrails 默认关闭，不等于 `apps/cli` 客户端不存在。
- embeddedServer 设置页面默认关闭，解释了“源码有 11 个设置组件而空账户桌面菜单没有 Server”的差异。
- Pages sharing 默认开启；关闭后 publish/update 不可用，但 unpublish 仍可执行，避免已分享内容无法撤销。
- Guarded 可见性由 Decision layer 的 guardedMode 控制，还需保留当前已处于 Guarded 的会话选择项。

## 实施顺序与边界

M1 定义 `ServiceEndpoints` 与 capability 查询，UI 不拼接服务地址。M4 完成 Sources/OAuth 相关接口，M6 完成 Decision 和 scheduler，M7 部署/验证分享、远程与渠道，M8 完成更新源和正式产物。

账号、域名和部署凭据在对应集成已可运行、请求范围具体时再处理；现在可以先实现本地服务、协议测试和失败恢复。没有可用服务的项目标记 blocked，不能从完整复刻范围中静默删除。
