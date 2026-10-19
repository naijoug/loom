# M2 生产内核运行桥阶段记录

- 日期：2026-10-09，Asia/Shanghai。
- 代码基线：`2fb672a` 后的本切片；具体源码 SHA-256 见 [证据](m2-core-bridge-evidence.json)。
- 范围：计划第 19 节。M2 与完整复刻仍实施中，本记录不是桌面交付验收。

## 已实现

- `Core::runtime_context` 从有效租约读取冻结连接、本轮输入、先前终态回合和导入 Chat 历史。排除未来排队输入及思考内容；不复用旧 native resume。超过预算或附件未接入时明确失败。
- 无 Project 会话使用私有 `work/`，必须在领取租约前明确绑定实际 cwd；目录被符号链接替换或任意目录冒充时拒绝。
- `loom_runtime::run_session` 使用生产 Handler 接 SDK 事件、用户审批消费、自动策略审计和取消。必须传入可信 policy，当前接口不等于完整 Craft 分类策略已经实现。
- 自动决定保存原始参数 digest 与脱敏预览；已知凭据在截断前从字段名和值移除，仍用原始参数校验授权。工具结果、错误和文本也移除已知凭据。
- `settle_runtime_turn` 在单锁内裁决取消与终态竞争，已接受取消不会被迟到成功覆盖。错误有界；存储失败不返回成功。
- 累计输入/输出 token 按回合持久化，回退/非法大整数被拒绝；旧记录缺少用量字段仍可重放。TS 声明从 Rust 生成，前端 reducer 与 Rust 事件重放一致。
- `sdk_runtime_probe` 已移除重复业务 Handler；模拟 UI 线程响应真实 pending 审批，执行与消费由生产桥接负责。

## 验证

| 检查 | 结果 / 实际范围 |
| --- | --- |
| `pnpm check:core` | 通过：fmt、Clippy、91 个 workspace 测试入口、双协议漂移及协议 TS 检查。包含进程内与子进程场景。 |
| 新增 11 个测试 | 上下文/冻结/历史、目录与链接边界、用量/重启/旧格式、秘密/原始授权、允许/拒绝/取消、早期失败/坏退出、取消终态竞争。 |
| `pnpm test -- tests/unit/workbenchProjection.test.cjs` | 7/7：真实 Rust 事件包含自动允许/拒绝和用量，前端最终投影、乱序、缺口恢复一致。 |
| `bun run typecheck`（provider-workers） | 通过。 |
| `pnpm build` | 通过；未启动桌面预览。 |
| `python3 scripts/runtime-fetch-smoke.py` | 6/6：真实 Pi SDK + 注入 fetch + 生产桥接；允许、拒绝、取消、错误绑定拒绝、无认证、命令形状凭据。 |

Pi 检查实际临时文件副作用、审批持久化与终态；模型请求必须包含已结束的用户/助手历史，不能包含未来排队消息，且全局端点变更不能改变已接受回合。密钥按字面发送、没有作为命令执行；无认证头移除和非取消回合用量保存均已验证。该测试使用构造的历史作为输入，不宣称 SDK 原生续聊已实现。

可复查数据：[Pi 结果](m2-core-bridge-fetch-results.json)。本地详细输出在 `.tmp/core-bridge-validation/`，测试文件副作用位于自动清理的临时目录。

## 未完成与限制

- 当前生产桥接是宿主可调用库，尚未接正式桌面队列调度和 vault 密钥读取；普通 Workbench 未因此启用 SDK。
- 完整 Explore/Ask/Guarded/Execute 分类、分层配置/记住授权、其余账户认证、Sources/Skills/MCP、背景任务继续实施。
- 历史以结构化转录提示传给 SDK；native resume、compact、steer、图像附件、完整缓存 token/cost 和大输出资源持久化未完成。很长的导入历史读取还需优化。
- Claude 新 worker/真实 HTTP 测试仍未运行：普通沙箱禁止 localhost bind，先前自动审批两次超时，明确许可待回复。这不是测试失败或安全否决，不能写成通过。
- Mac 解锁待回复，因此连接原生界面仍未验收；本切片没有新增原生界面证据。
- Windows 进程树监督尚未实现；macOS 的无网络测试不能证明其他平台完成。
