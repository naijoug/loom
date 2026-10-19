# Craft Chat M3 — 多 Agent 对等 + 诊断内联

- **Date**: 2026-09-17
- **Focus**: Claude tool/stream → 同一 `parts[]`；权限档 CLI 提示与 M0/M2 表一致；Chat 内 Agent 状态弹出

## 本机 Mac 手工路径（验收）

1. Chat → 新建会话 → Agent 切到 Claude（若可用）。
2. 发一条会触发工具的问题（例如「列出当前目录」）；气泡应出现「工具 · …」卡片（与 Grok/Codex 同款 `parts`）。
3. 权限保持「探索」：Claude argv **不应**带 `--permission-mode`；切「自动」后再发：应带 `--permission-mode acceptEdits`。
4. 点 Composer 旁「状态 · …」：应显示路径 / 版本 / 说明，无需打开 Settings；缺失二进制时应显示「缺失」摘要。
5. 切回 Grok：探索/询问 → `--permission-mode plan`；自动 → `acceptEdits`。

## 本执行环境

Linux box / CI 通常无用户 Mac 上的 `claude`/`grok` 二进制；M3 以 stream fixture 单测 + UI 诊断路径完成，真实 dogfood 在开发者 Mac 勾选。

## 权限映射（与 M0/M2 相同）

| UI | stage | Grok | Codex | Claude |
|---|---|---|---|---|
> **Phase 2 更新（2026-09-17）**：`ask` 改为 Debugging / 可写 CLI + Composer 每回合确认；见 `docs/plans/2026-09-17/10:55-craft-chat-phase2.md`。下表保留 Phase 1 历史映射。

| explore / ask | Planning | `plan` | `read-only` | 不传 `--permission-mode` |
| auto | Debugging | `acceptEdits` | `workspace-write` | `acceptEdits` |
