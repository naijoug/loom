# 结构化续聊、权限参数与工具结果验收（2026-09-20）

基线：`0fe5f79` 上的当前未提交工作树，接续发送幂等切片。对应 [主计划 §17](../plans/2026-09-17/10:15-local-agent-chat-rebuild.md#17-m21-续聊绑定与参数边界2026-09-20)。本记录不代表完整发布验收。

## 已实施

- Chat 的 `resumeHandle` 保存版本、adapter、原生会话 ID 和 SHA-256 配置指纹；完成时使用该回合 prepared 配置生成，不读取随后改变的全局设置。新增直接依赖 sha2 0.10.9，复用锁文件已有版本。
- 指纹绑定 Loom 内的 Agent、程序/参数、目录、权限、能力与兼容 stage；不保存可能含秘密的配置原文。CLI 自有配置、登录身份和二进制替换不在指纹覆盖范围内。
- 原始 resumeCommand 只作诊断；旧会话缺少指纹或配置不匹配时明确拒绝续聊，提示“开新 CLI 会话”后保留历史重放。UI 区分“可续聊”和“续聊需重建”。
- adapter 旧字符串入口只接受确切程序前缀、已知操作、单个有界非选项 ID，不复用额外参数；支持含空格程序路径。Codex 首轮/resume 同样设置 cwd/sandbox/never approval，并保持 stdin；Claude/Grok Chat 显式传 plan/acceptEdits。前导连字符正文不进入 CLI 选项解析。
- Chat 权限独立传给 adapter，不以 Task planning/debugging capability 作为聊天前提。全量回归发现 Claude Task 的计划输出不能使用 plan 模式，已保持该路径原行为，Chat 独立设置权限。
- 真实探针暴露同一 JSON 消息的多个工具结果只保存首个的问题：已改为全部解析、逐个发布/持久化，并识别 `is_error`。本地真实子进程回归覆盖三个同帧结果、第二个结果内容、错误状态以及最终投影一致性。
- 同步移除上下文面板中已经失效的 Shift+Tab 循环权限提示。

## CLI 探针来源

本机版本：Codex **0.155.0**、Claude **2.1.273**、Grok **1.0.30 (04b7ffed98c6)**。读取各 CLI help；`codex exec --json --cd /tmp --sandbox read-only --config 'approval_policy="never"' resume --help` 退出 0，证明本机参数位置可解析，不等同于实际沙箱验收。

OpenAI Docs 参考：[CLI 命令参考](https://learn.chatgpt.com/docs/developer-commands?surface=cli) 与 [非交互续聊](https://developers.openai.com/zh-Hans/docs/non-interactive-mode)。本机 help 位于 `/tmp/loom-codex-exec-help.txt`、`/tmp/loom-codex-resume-help.txt`、`/tmp/loom-codex-policy-resume-help.txt`、`/tmp/loom-claude-help.txt`。

## 最终本地检查

`CARGO_NET_OFFLINE=true pnpm check` **通过**：前端 **164**、Rust **271**（另 4 ignored）、集成 **2**，含 build/fmt/Clippy/发布文档门禁。`CARGO_NET_OFFLINE=true pnpm tauri build --no-bundle` **通过**，产物名称配置保持 Loom。

新增覆盖：原生续聊与首轮权限参数一致、stdin 与正文选项隔离、空格路径、额外参数/跨程序/无效 ID 拒绝、配置指纹变化、旧记录显式重置、UI 恢复标签和重置动作、跨语言句柄样本，以及批量工具结果不丢失。已有 Task 回归通过。

日志：`/tmp/loom-resume-final-check.log`、`/tmp/loom-resume-final-build.log`、`/tmp/loom-batched-tool-test.log`。没有重启或覆盖用户正在操作的 Loom.app；本轮仅生成默认 release 二进制，没有将旧桌面包描述为本轮最终候选。

## 真实 Grok 结果与限制

增强的 `grok_three_turns_resume_after_repository_reopen` **通过**：三个无工具文本回合都恢复同一原生上下文，每轮正确回复唯一随机标记；结构化句柄/指纹从持久化仓储恢复，活动和重开后重复请求仍去重。此测试在批量工具结果修复前运行，不能代替修复后文件工具验收。

文件探针尝试两个模式，各两轮（首轮与 resume）：读取合成哨兵，再尝试用编辑工具写入指定标记。已观察到 explore 的读取和 search_replace 调用，编辑返回取消、文件未变。第一次探针只断言模型正文，遗漏工具读取结果；修正断言后再次暴露解析器丢失同帧第二个工具结果的问题。两次整体测试都 **failed**，未计为权限矩阵通过，也尚未运行到 ask 写文件部分。探针保留合成项目供复查：

- `/var/folders/wh/8gkvgk297s392p974p39b0s40000gp/T/loom-permission-canary-1789885940017-1`
- `/var/folders/wh/8gkvgk297s392p974p39b0s40000gp/T/loom-permission-canary-1789886040074-1`

真实测试日志分别为 `/tmp/loom-resume-real.log`、`/tmp/loom-resume-permissions.log`。上述三个文本回合的通过记录在前一日志中，但整条命令因文件测试失败而退出 101，不能将整条命令标为通过。

修复后文件探针重跑被运行环境的自动审批拒绝：此前 Grok 还读取了主目录中的私人 skill（已观察到 in-english），可能把该内容发送给外部服务，超出仅临时文件测试的授权范围。已停止重跑并请求用户明确授权；没有绕过拒绝。**最终修复后的真实文件权限验收仍未完成。**

## 后续

获授权后重跑文件探针并据结果修复；仍需真实停止后再发、运行中退出、完整 ChatTurn/运行日志、导出/长历史、Chat 默认 Shell 分离及同候选签名/公证/安装门禁。未验收的 Codex/Claude 权限和外部工具边界不列为通过。
