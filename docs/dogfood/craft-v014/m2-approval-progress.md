# M2 持久化工具审批与 SDK 集成切片

日期：2026-10-07。基线：`ecfaebc` 加本轮修改。范围是单次工具审批的持久化边界、桌面操作和真实 SDK 单写工具集成；**生产模型连接、完整策略与 M2 里程碑尚未完成**。

## 实现

- Runtime 持有 Rust `TurnLease` 才能创建请求。原始参数参与 digest，日志仅保存脱敏预览；绑定包含工作区、会话、回合、工具、请求、cwd、连接、权限模式与策略版本。
- 用户允许/拒绝先提交 journal。SDK 宿主只有通过同一租约和原始参数消费一次授权，才能取得 `ToolPermit`；消费与工具 Running 状态同一事件提交。
- 重复请求不延长期限，跨会话/参数变化/过期/取消/重启不能取得新许可。未决审批阻止回合误报成功。已登记审批的工具不能用普通 ToolStarted 绕过消费。
- 新前端投影回放审批事件，参数面板支持单次允许/拒绝和历史终态；迟到回应与连点不会修改别的会话。真实 Rust fixture 同时验证前端的乱序/补流/恢复，而非手写一份预期状态。
- `sdk_core_approval_probe` 将真实 Claude/Pi SDK 的 Write/write 接到生产 core，保存工具结果、文本流和回合终态。它仍是限定合成目录、loopback 模型端点的验收程序，未作为生产 adapter 发布。

## 验证结果

| 检查 | 结果与范围 |
| --- | --- |
| `pnpm check:core` | 60 个测试入口通过（27 原内核 + 19 迁移 + 2 迁移进程入口 + 12 审批入口，包含子进程 helper）；fmt、Clippy、生成协议漂移和协议 TS 编译通过。 |
| 审批行为 | 精确参数绑定、一次消费、普通 ToolStarted 绕过拒绝、并发相反决定、拒绝、迟到、过期、取消、写盘失败和进程退出后撤销均通过。 |
| 脱敏 | journal/cache 不含已知测试密钥与文件正文；修复 `sk-` 前缀归一化误伤普通 task 路径，新增 `tasks/task-list.md` 与真实形状密钥同时出现的回归。不是任意文本秘密识别保证。 |
| 前端定向回归 | 12 项通过：审批连点/请求归属/切换隔离，以及真实 core 生成事件的投影、乱序、补流、草稿与 IME。 |
| Tauri 宿主 | Workbench 6 项测试通过，包括通过真实 core 响应的允许、拒绝、退出撤销；严格 Clippy 通过。 |
| 构建 | `pnpm build` 与 macOS debug app bundle 通过；最终脱敏修复后重新构建并复验。 |
| 真实 SDK + 本机模型 fixture | Claude/Pi × allow/deny/cancel/stale/disconnect，共 10/10 通过；允许前没有 probe.txt，拒绝/取消/断线没有写入，允许仅产生精确合成内容，core 的批准消费和终态均被验证。[SDK 机器证据](m2-core-sdk-approval-results.json)。 |
| 原生 UI 与重启 | 待审批参数可见且 API key 隐藏；单次允许、拒绝、停止和等待期间 Cmd+Q 均通过。退出后新 core 回读证明消费 2 次、拒绝 1 次、撤销 2 次，拒绝/撤销无 ToolCall；最终 app 重启显示旧撤销记录并完成一次新请求。[桌面机器证据](m2-approval-desktop-evidence.json)。 |

原生图片保存在 `designs/workbench-m2/`：[待审批](../../../designs/workbench-m2/01-pending-approval.jpg)、[已使用](../../../designs/workbench-m2/02-consumed-approval.jpg)、[拒绝](../../../designs/workbench-m2/03-denied-approval.jpg)、[撤销](../../../designs/workbench-m2/04-cancelled-approval.jpg)、[最终构建重启](../../../designs/workbench-m2/05-restarted-cancelled.jpg)、[最终构建新授权](../../../designs/workbench-m2/06-final-consumed.jpg)。同名 AX 文本记录了实际按钮与状态。

桌面证据分别保存首轮和最终构建的 app/源码 SHA；SDK 证据对应最终审批实现。原生桌面使用明确标注的无副作用测试工具，没有调用模型。SDK 测试使用真正的 SDK 与临时写工具，但模型 API 为本机 fixture，没有私人凭据或真实模型账户请求。两者不能合并宣称生产模型已接到桌面。

## 复现

```sh
cargo build -p loom-core --example sdk_core_approval_probe --offline
python3 scripts/runtime-bridge-smoke.py --core --output .tmp/core-sdk-approvals.json
scripts/debug.sh stop
VITE_LOOM_WORKBENCH=1 pnpm tauri build --debug --bundles app
LOOM_PREVIEW_TARGET=workbench-bundle LOOM_WORKBENCH_FIXTURE=1 \
LOOM_WORKBENCH_APPROVAL_FIXTURE=1 LOOM_WORKBENCH_DATA_DIR="$PWD/.tmp/approval-qa" \
scripts/debug.sh desktop
scripts/debug.sh stop
```

本轮结束已关闭桌面实例并执行 stop；合成数据保留在 `.tmp/workbench-approval-2026-10-03/data`。没有更改用户真实项目数据。

## M2 剩余要求

- 正式 worker 协议、桌面进程监督、Claude/Pi 完整工具/流/恢复/compact/steer、usage/错误映射，以及历史导入上下文接线。
- 连接配置、凭据 vault、API/OAuth/自定义端点与真实账户的独立验收。
- Explore/Ask/Execute 完整分类与规则优先级、Guarded 风险服务/回退、记住授权、Source 策略版本变化后的失效处理。当前单次 broker 不是自动策略实现。
- 全工具类别的副作用边界、跨平台进程回收、OS sandbox 能力与发布打包。授权后已执行的副作用不能因取消而回滚。

本切片不抵扣 M0 未完成项、M1 跨平台迁移和 M3–M8 出口，也不将整个重建目标标为完成。
