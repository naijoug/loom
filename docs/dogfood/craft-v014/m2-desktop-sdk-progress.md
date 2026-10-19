# M2 Tauri 宿主 SDK 接线记录

- 日期：2026-10-09，Asia/Shanghai。
- 基线：`9b4bcb7` 后本切片。源码/日志哈希见 [证据](m2-desktop-sdk-evidence.json)。
- 范围：计划第 21 节。完成开发宿主生产路径接线；完整 M2、原生交互和发行交付仍未完成。

## 已实现

- Tauri `Host` 在明确启用 SDK 时调度真实 worker，沿用新的 core 会话队列，保持每会话唯一运行者。排队观察与所有权释放同锁，不丢失新发送唤醒；不同会话可以各自执行，不因一个会话等待审批而阻塞另一个。
- `Connections::runtime_secret` 只给 Rust 返回冻结连接对应的不可变 slot，轮换后的新密钥不会覆盖已接受回合；秘密缓冲使用 Zeroizing，校验 UTF-8/长度/控制字符后借用到 worker 的私有 stdin。无认证不读取 vault。
- Rust 负责 cwd、默认策略 digest、SDK 工具集、模型/thinking 与执行上限。模型端点来自接受快照；前端不能设置 worker/executable 或凭据 slot。
- `claim_prepared_turn` 要求准备的 turn 仍为队首，避免准备期间取消后将旧配置套到下一条。新增 `TurnPreparationFailed` 与 host-only `fail_queued_turn`，准备错误有持久化终态/原因、没有伪造 startedAt/执行绑定；TS 投影同步支持该事件。
- SDK 失败保留已产生事件并结束回合，继续后续独立消息，不自动重跑失败副作用。关闭应用发共享 stop，回收进程、撤销审批并保留未执行队列；重启后用户可继续队列。
- 开发入口报告 `runtime: sdk`；前端启用已连接会话发送、排队恢复和状态提示，缺连接的发送按钮与 Enter 均受约束。既有 fixture 仍明确区分。

## 验证

| 检查 | 结果与范围 |
| --- | --- |
| `pnpm check:core` | 104 项 workspace 测试入口、fmt、Clippy、协议漂移、协议 TS 编译通过。新增精确队首/取消竞争/准备失败重启测试。 |
| Tauri Workbench 回归 | 17 项通过，原生 Keychain 与真实 SDK 显式用例默认 ignored；随后单独执行下面的 SDK 用例。 |
| 实际 Pi SDK → Tauri Host → Core | 显式集成测试 1 项内的 3 模式通过：Explore 拒绝、Ask 实际批准后写、Execute 自动写；检查临时 probe.txt 内容和策略绑定。 |
| 宿主失败与生命周期 | 顺序、重复发送、失败不重试、无连接准备失败、并行会话隔离、审批允许/拒绝/取消、退出撤销与排队保留、重启后继续通过。 |
| Vault 读取 | 内存 vault 测试验证读取被冻结的旧/新 slot、缺失项、无效 UTF-8 和换行拒绝。未用该测试宣称系统 Keychain UI 已验收。 |
| Tauri 检查 | fmt 和 `cargo clippy --locked --offline --all-targets -- -D warnings` 通过。仅新增运行时所需 lock 条目，移除了 Cargo 初次解析的无关依赖升级。 |
| 前端与脚本 | 定向 18 项通过，含 SDK 连接门禁/Enter/继续队列、Rust 真实准备失败事件投影、原有会话和连接/脚本回归；`pnpm build` 通过。 |

本地记录位于 `.tmp/desktop-sdk-validation/`。真实 SDK 测试是无网络的 fetch 注入，用生产 Tauri Host 直接调用运行链路，没有启动原生窗口，也没有调用公网模型或真实模型账户。

## 运行与剩余工作

开发启动：`LOOM_PREVIEW_TARGET=workbench LOOM_WORKBENCH_SDK=1 ./scripts/debug.sh desktop`。Bun 绝对路径由启动脚本提供，worker 从固定仓库位置读取；工作台选择已保存连接后发送。每回合预算当前为 15 分钟、100 次 SDK turn、768 KiB 历史，超限明确失败，尚非完整动态设置。

- 发行 Bun/worker/依赖资源及签名未完成，release 当前仍报告 runtime unavailable；不能使用本机源码位置冒充安装包可用。
- 权限使用默认策略；工作区/Source 配置存储与热更新、完整策略快照内容持久化、管理员/记住授权、Shell session 输出例外、真实 Guarded provider 仍按计划继续。
- Claude 新 worker 与实际 HTTP 仍待验证；先前普通沙箱禁止 bind，自动审批两次超时，明确许可待回复。本轮无网络测试不抵扣。
- 原生连接/发送/停止界面仍待 Mac 解锁后验收；组件测试和宿主集成不代替原生操作。
- 原生 resume/compact/steer、完整账户、附件、Sources/Skills、浏览器和其余 M3–M8 范围继续实施。
