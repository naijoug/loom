# Loom Craft Chat — Phase 2 Plan

- **Date**: 2026-09-17
- **Author**: Droplet
- **Status**: in_progress
- **Progress**: P2-M0 ✅（Ask 语义 PRD 冻结：ask = 可写 CLI + 每回合发送前确认）；P2-M1 ✅（Composer 确认门 + CLI/stage 映射与 explore 分离）；P2-M2 deferred；P2-M3 / P2-M4 未开
- **Scope**: 在 Phase 1（`08e3193`，`docs/plans/2026-09-17/09:03-craft-inspired-local-agent-chat.md`）之上，把 **Ask** 从「与 explore 相同的只读 CLI」升格为可区分产品档位；顺带排期 Sources/MCP 尖刺、后台回合指示、可选 Inbox 五态。**仍不**做完整 Electron Craft 移植；保持 Tauri + `ProcessSupervisor` Chat 路径；**ChatSession ≠ Task**。

参考：

- Phase 1 收口：`docs/dogfood/craft-chat-phase1-checklist.md`
- 契约：`docs/architecture/chat-contracts.md`、`docs/guides/craft-local-chat-prd.md`

## 目标

- Ask 在 **UX 与 CLI** 上均不再等同于 Explore。
- 权限三档可解释：
  - **explore**：只读 / plan（不变）
  - **ask**：可写 CLI（同 auto 的 stage / acceptEdits），但 **每回合发送前** 必须显式确认「允许本回合写文件/跑可写工具」
  - **auto**：可写、无确认
- 保持 Chat 经 `agent_adapter::prepare_invocation` + `ProcessSupervisor(ProcessKind::Chat)`；不绑 Task stage。

## 非目标

- 不全量移植 Craft Electron / server-core / Sources 全家桶。
- 不做 per-tool 运行时审批弹窗（stream 拦截写工具）——本 Phase 选定 **发送前确认**，更易端到端落地。
- 不重写 Task 状态机；不强制 Craft 五态 Inbox（P2-M4 仅产品决策后可选）。
- Beta 公证：仅备注，仍可能堵在 `loom-beta-notary`（见 beta-release-readiness）。

## 成功标准（Phase 2 本切片）

- 磁盘上有本计划；`docs/PLANS.md` 已索引；Author Droplet；Status 随进度更新。
- Ask ≠ Explore：UI 有发送前确认门；Rust/前端映射 ask → Debugging / 可写 CLI flags。
- `pnpm test` 相关单测 + `cargo test chat::tests` 通过；无关脏文件不暂存。
- 推送到 `origin/main`。

## 已冻结决策（P2-M0）

1. **Ask 设计（选定）**：**可写 CLI + 每回合发送前确认**（不是「首个写工具再暂停」）。
   - 理由：现有 stream `parts` 无可靠的 pre-tool 拦截钩子；发送前确认可在 Composer 端到端落地，且与「询问编辑」文案一致。
   - 确认文案：「允许本回合写文件/跑可写工具」→ 确认后才 `chat_send`；取消则不发送。
2. **CLI / stage 映射**：
   - `explore` → `Planning`（plan / read-only）
   - `ask` → `Debugging`（acceptEdits / workspace-write）**且** UI 确认门
   - `auto` → `Debugging`，无确认门
3. **旧值**：`read_write` → `ask` 仍成立；加载后变为「可写但需确认」，仍比 auto 更安全。
4. **MCP / Sources（P2-M2）**：若尖刺过大则 **defer** 并在本计划写明；不阻塞 Ask 门禁。
5. **Inbox 五态（P2-M4）**：仅当产品明确需要再开；默认保持 `active` | `archived` + `needs_attention` 过滤。
6. **Beta 公证**：本计划只记笔记，不排实施任务。

## 里程碑

### P2-M0 — Ask 语义 PRD 冻结 + 映射表

**Progress**: ✅ 完成（2026-09-17）— 本计划 + 契约/PRD/IA 同步；`chatPermissionAllowsWrite` / `permission_to_stage` / CLI hint 与确认门语义对齐。

| # | Task | Verification |
|---|---|---|
| 0.1 | 冻结 Ask = 可写 + 每回合确认；写入本计划与 PRD/契约 | 文档一致 |
| 0.2 | 更新前端 `chatPermissionAllowsWrite` / `permissionCliHint` | ask 显示可写 CLI hint |
| 0.3 | 更新 Rust `permission_to_stage`：ask → Debugging | 单测 |

### P2-M1 — Ask 确认 / tool-gate stub 端到端

**Progress**: ✅ 完成（2026-09-17）— Composer 内联确认条；Enter/发送先过门；确认后才 invoke `chat_send`。

| # | Task | Verification |
|---|---|---|
| 1.1 | Composer：ask 模式下发送弹出确认条 | 不确认不发送 |
| 1.2 | explore / auto 路径不变 | 回归单测 / 手工 |
| 1.3 | 单元测试覆盖确认门辅助函数与映射 | `pnpm test` + cargo |

### P2-M2 — Minimal Sources/MCP read-only connect spike

**Progress**: deferred（过大）— Phase 1 上下文空态足够；真正 stdio/http MCP 连接另开计划，避免与 Ask 门禁抢带宽。

### P2-M3 — Background turn indicator / timeout

**Progress**: 未开始 — 产品化后台回合指示与超时文案；现有 spinner/abort/120s safety timeout 可作基线。

### P2-M4 — Inbox status expansion

**Progress**: 未开始 / 可选 — 仅当需要 Craft 五态（todo / in_progress / needs_review / done / cancelled）时再开；需单独产品决策。

## Beta 公证（笔记）

公开 Beta / notarization 仍可能阻塞在 `loom-beta-notary` 与 `docs/plans/2026-08-07/10:00-beta-release-readiness.md`；**不在**本 Phase 实施。

## 风险

| Risk | Impact | Mitigation |
|---|---|---|
| 发送前确认被当成「全会话授权」 | 中 | 文案强调「本回合」；每发必确认 |
| ask 可写后用户误以为已有 per-tool gate | 中 | 契约写明 stub；完整 tool-gate 后续 |
| MCP 尖刺拖垮节奏 | 高 | P2-M2 已 defer |
| 与 Phase 1 dogfood 文档漂移 | 低 | 更新契约/PRD 映射表 |

## 验证门禁

- 相关 `tests/unit/chatPermission*.test.cjs` 通过
- `cargo test chat::tests`（permission 映射）通过
- `pnpm exec tsc --noEmit`（或仓库 `pnpm check` 子集）触及文件无类型错误
- ChatSession 仍不推进 Task stage

## 计划变更日志

| 日期 | 变更 |
|---|---|
| 2026-09-17 10:55 CST | 初稿 Phase 2；冻结 Ask=可写+每回合确认；P2-M0/M1 实施；P2-M2 defer |
