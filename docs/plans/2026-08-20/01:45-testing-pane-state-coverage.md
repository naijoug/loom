# Testing Pane 状态覆盖补强计划

## 目标

把上一轮补齐的 validation evidence 边界，向上推进到 Testing Pane 的组件级状态覆盖：验证 UI 在“验证运行中 / 验证失败 / 验证通过但等待人工接收 / 预览或其它任务运行不应解锁”这些状态下，展示的阻塞原因、按钮可用性和修复入口与模型层一致。

## 非目标

- 不重写 Testing Pane 交互结构。
- 不新增端到端浏览器或桌面 smoke。
- 不扩展 release docs checker。
- 不修改 Tauri 后端状态机，除非测试暴露真实不一致。

## 成功标准

- `tests/unit/componentInteraction.test.cjs` 或相邻组件测试中新增最少 2 个断言，覆盖 Testing acceptance gate 的关键状态组合。
- 组件测试能证明 preview intent、agent action 或其它 task 的 command run 不会让当前任务的“接受完成”按钮误解锁。
- 组件测试能证明本任务显式 validation intent 的通过证据，会在没有配置同名 slot 时解除 gate 阻塞。
- `pnpm test` 与 `pnpm check` 通过，且 `git diff --check` 无输出。

## 当前状态

- `tests/unit/testingModel.test.cjs` 已覆盖 `deriveValidationEvidence` 的底层判断：preview run、其它任务 run 不计入当前任务；显式 validation intent 即使命令文本不在 slot 配置中也计入证据。
- `pnpm check` 已在 2026-08-20 01:45 通过：release docs check、release docs checker fixture、102 项前端测试、TypeScript/Vite build、193 项 Rust lib 测试、2 项 Rust workflow harness 均通过。
- 现有组件级测试已有 `Testing acceptance gate exposes the blocking reason and only fires when enabled`，但更偏基础阻塞提示，没有把上一轮新增的 intent / task scope 边界提升到 UI 层。

## 里程碑

### M1：定位测试缝隙

- 阅读 `tests/unit/componentInteraction.test.cjs` 中 Testing acceptance gate 现有 fixture。
- 确认组件测试如何构造 task、commandRuns、terminalSlots 与用户点击。
- 记录是否需要抽 helper，避免复制过多状态对象。

### M2：补 UI 级边界断言

- 新增“preview / unrelated task runs do not unlock acceptance”的组件断言。
- 新增“explicit validation intent unlocks acceptance without slot command match”的组件断言。
- 如果现有组件测试入口过重，优先补纯 render + click 层，不引入 mock server 或 Tauri runtime。

### M3：验证与收口

- 先运行 `pnpm test`，确保新增断言稳定。
- 再运行 `pnpm check`，确认前端、构建和 Rust 回归仍绿。
- 用 `git diff --check` 检查 Markdown 与代码空白。

## 具体任务

1. 打开 `tests/unit/componentInteraction.test.cjs`，找到 Testing acceptance gate 测试和可复用 fixture。
2. 用最小 task fixture 构造当前任务处于 verifying/testing 阶段，且 commandRuns 包含：
   - 当前任务 preview intent success；
   - 其它 task validation intent success；
   - 当前任务 validation intent success。
3. 分别断言 acceptance gate 的阻塞/解锁文案与按钮状态。
4. 若发现 Testing Pane 使用的 evidence 派生与 `deriveValidationEvidence` 不一致，优先修代码；否则只提交测试。
5. 跑 `pnpm test && pnpm check && git diff --check`。

## 风险

- 组件测试可能依赖较大的 App fixture，新增状态组合容易脆弱；应优先复用现有 helper。
- 如果 Testing Pane 的 props 与状态模型过度耦合，可能需要先做小型测试 helper 重构，但不要在同一轮扩大为 UI 重构。
- `pnpm check` 包含 Rust 测试，时间较长但仍在可接受范围；若仅改组件测试，至少先跑 `pnpm test`，有时间再跑完整 check。

## 待确认问题

- Testing Pane 当前是否直接复用 `deriveValidationEvidence`，还是在组件中重复判断？若重复判断，本计划应优先消除重复逻辑。
- “接受完成”按钮的精确文案是否已经稳定中文化？断言应优先检查行为和关键阻塞原因，避免绑定脆弱的整段文案。

## 验证策略

- 快速验证：`pnpm test`。
- 完整验证：`pnpm check`。
- 变更卫生：`git diff --check`。
