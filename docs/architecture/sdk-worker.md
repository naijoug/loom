# SDK worker 通信与监督

状态：M2 实施切片，2026-10-09。生产 core/runtime 已接 Tauri 开发宿主，权限热更新、发行资源与真实账户尚未完成；连接页与 Claude 原生 SDK 的补验见 [HTTP/原生记录](../dogfood/craft-v014/m2-http-native-progress.md)。

## 模块边界

- `crates/loom-runtime`：不依赖 Tauri 的 Rust 进程监督与协议。宿主传入已选定的命令、`RunConfig`、借用的凭据和 Handler；负责启动、私有管道、作用域、工具请求/决定状态、期限、取消、终态与回收。
- `crates/loom-runtime/src/core_bridge.rs`：`run_session` 消费有效 TurnLease，从 core 获取冻结上下文，通过生产 Handler 写事件、消费审批并在进程清理后保存终态。调用方必须提供可信 Rust `ToolPolicy`，没有默认放行策略。
- `packages/provider-workers/src/runtime-worker.ts`：固定 Claude/Pi SDK 适配器。一个进程承载一个回合，模型/端点来自命名连接配置，正文与历史由宿主提供；不包含 M0 的固定提示词、固定凭据或单文件策略。
- `packages/protocol/src/runtime.ts`：从 Rust 生成的 TS 协议；worker 的 Zod 输入类型及事件输出编译到这个契约。`pnpm check:core` 同时检查运行时声明漂移。
- `sdk_runtime_probe`：调用上述生产桥接，仅实现临时 probe.txt 的合成策略及模拟独立 UI 的审批响应线程。测试不再另写事件转换或授权消费逻辑；该策略不是产品权限分类器。
- `src-tauri/src/workbench/runtime.rs`：Tauri Host 的 SDK 队列服务；读取发送时冻结连接的 vault slot，建立 cwd/策略并领取指定排队回合，调用生产运行桥。SDK 失败保存终态且不自动重试；退出共享 stop 信号并等待进程回收。开发模式显式启用，发行资源定位/打包仍待完成。

## 冻结上下文与业务终态

`Core::runtime_context` 只能由当前运行租约调用。模型/端点来自发送时保存的连接快照；历史包括更早已结束回合的可见文本、工具记录及已导入的旧 Chat 历史，排除思考内容、未来排队输入和旧原生 resume 句柄。现阶段按字节预算明确拒绝过长上下文，不能静默截断；附件尚未接入时也明确失败。

执行前先调用 `runtime_working_directory`，将 Project 目录或无 Project 会话的私有 `work/` 路径写入 RuntimeBinding，再领取租约。SDK 不接受缺失 cwd 的绑定。状态目录位于会话下的 `runtime/<turnId>/`，检查目录及符号链接，创建时限本用户读写。

生产桥接将文本、工具结果、累计输入/输出 token 写入 journal。自动策略决定先写入脱敏预览、原始参数 digest、理由和结果，再向 SDK 发决定；用户批准由 core 校验完整绑定并只消费一次。审批预览先移除已知凭据及编码形式，再做长度限制；原始参数仍用于摘要校验，不能用脱敏值冒充实际授权。

`settle_runtime_turn` 在同一个状态锁内确定终态：已接受的取消优先于迟到的成功/失败；存储失败不会返回完成。进程成功仍须满足 supervisor 的 EOF/退出/工具清理条件。用量按回合保存且只允许累计增长，重启重放保留；它不包含完整缓存 token/cost 计费模型。

## 进程与传输

启动帧携带 protocolVersion、workspace/session/turn/run IDs、连接、模型、工作目录、临时状态目录、历史与当前消息。凭据仅走 stdin，不进入 argv；启动缓冲用 zeroize 清理。环境仅传 PATH/HOME/USER/SHELL/TMPDIR/LANG，Claude 使用专属配置目录，Pi 使用内存凭据与模型仓库，禁用自动扩展、skills、上下文文件和网络模型目录刷新。其资源加载在后续 Source/Skill 服务授权接线后开放。

stdout 为有界 JSONL：单帧 1 MiB，Rust 接收队列 32 帧，worker 待写预算 4 MiB。每个事件必须作用域一致、seq 连续；ready 仅一次且必须先于其他事件，终态后不允许追加事件。SDK stderr 丢弃且持续排空，避免阻塞与秘密外泄；这不是完整诊断日志实现，后续需补受控脱敏诊断通道。

只有收到声明过的 tool request 后宿主才能回应。request/tool IDs 去重，最多 32 个同时等待；工具结果必须对应已允许的请求。worker 在 PreToolUse（Claude）或工具执行包装（Pi）等待决定，并确认参数在等待期间未改变。Claude hook 校验 cwd 与最初目录身份，目录改变需要新绑定；不把旧 cwd 审批用到另一个位置。

完成消息先验证未决/活动工具，然后等待输出 EOF 和退出码；不会仅凭一条 completed 提前通知业务成功。Unix 进程组属于本次 spawn，leader 在组回收前不 reap，降低 PID 复用误杀风险。取消先发控制帧与 SIGTERM，期限到达强制清理；独立 watchdog 在外部停止标志/总期限到达时终止组，即使 Handler 回调暂时阻塞。Handler 必须快速返回；永不返回的任意宿主回调仍会阻塞调用线程，但不会让 worker 无限执行。

Windows 目前明确返回未实现，而非只杀 leader 冒充进程树回收。脱离受监督进程组的子进程、恶意原生插件/SDK 和 OS sandbox 是其他边界，当前 supervisor 不提供完整系统隔离。

## 凭据处理

Pi 的 provider config `apiKey` 支持表达式/命令，因此不接收原始密钥。worker 使用 `setRuntimeApiKey` 的内存凭据接口，验证 `!printf ...` 形状的合成密钥仍按字面值发送、没有执行。无认证连接在本 worker 的 HTTP 传输层移除 Authorization/x-api-key，防止 SDK 自动添加占位凭据。

Rust 对文本流保留密钥前缀，以便跨帧匹配；同时处理 JSON 转义与 URL 编码形式，支持 Unicode 与重叠前缀。完整工具输出/错误也移除已知凭据。原始 ToolRequest 参数只交给可信 Handler 做策略与审批摘要，Handler 不得原样日志打印。这不是对任意变换、编码或模型秘密外传的全面防护。

## 当前能力和限制

Rust 的 `shell::ShellPolicy` 已实现 Bash 语法树与程序参数分类，带固定上游默认规则和差分用例；它是 `PermissionPolicy` 的组成部分，已接开发宿主执行路径。范围、显式安全修正和剩余工作见 [Shell 分类记录](../dogfood/craft-v014/m2-shell-policy-progress.md)。

`policy::PermissionPolicy` 已连接生产运行桥，编译默认/工作区/活动 Source 规则并生成完整策略 digest；`RuntimeBinding.policySnapshotDigest` 绑定该对象，worker 启动前和工具请求时都校验。原生模式、API/MCP 分类、写路径规则及 Guarded 接口已有测试，配置外部热更新、完整授权交互和真实 Guarded provider 尚未接入。[策略核心记录](../dogfood/craft-v014/m2-tool-policy-progress.md)列明验证范围与差异。

配置接线：Tauri 的权限保存接口先编译规则，再写 Core catalog（版本冲突/幂等/重置）；SDK 每回合读取当前文档并将完整有效策略写入该会话的 `policies/<digest>.json`。审计读取按会话/回合验证绑定与 hash。运行中策略保持冻结，外部文件 watcher/逐工具热更新尚未完成；权限 JSON 编辑页已做原生保存/重启/重置验收。[配置与审计记录](../dogfood/craft-v014/m2-permissions-store-progress.md)。

会话记住授权已接生产路径：策略提供 hashed selector 与显示标签，用户按绑定请求选择记住；Core 原子匹配/准入，审计引用 grant ID。清除与重启使授权失效，保留历史；精确命令不作为明文记忆键存储。静态域名选择器不是网络隔离。选择器范围、差分修正及原生/SDK 证据见 [记住授权记录](../dogfood/craft-v014/m2-remember-progress.md)。

Claude 支持显式 Read/Write/Edit/Bash/Glob/Grep 列表、文本/思考流、工具 hook、输入/输出 token 与取消。Pi 支持显式 read/write/edit/bash/grep/find/ls 列表、原生模型或兼容协议模型、工具包装、文本/思考流与取消。所有工具仍经宿主决定，不存在无策略默认放行。

自定义 Pi 模型的上下文窗口/输出预算目前为明确的临时配置值；推理支持尚无已配置能力时拒绝非 off 请求，不猜测。原生 Pi 仅接有限的 API-key 协议，OAuth/云凭据家族仍待适配。历史目前作为结构化转录提示发送，不复用原生句柄；恢复、自动 compact、steer、图像附件、背景任务、MCP/session tools、完整 usage/cost 尚未交付。

大工具结果超 512 KiB 时给 UI 明确摘要，完整结果仍在 SDK 本轮上下文；持久化资源输出需 M5 的内容仓库。开发宿主在明确启用 SDK 且路径有效时报告 `runtime: sdk`，前端仍要求选择连接；发行构建尚未打包资源时报告 unavailable。宿主接线证据见 [M2 桌面 SDK 记录](../dogfood/craft-v014/m2-desktop-sdk-progress.md)。

## 验证入口

```sh
pnpm check:core
cd packages/provider-workers && bun run typecheck
cargo build -p loom-runtime --example sdk_runtime_probe --offline
python3 scripts/runtime-fetch-smoke.py
python3 scripts/runtime-bridge-smoke.py --runtime --output .tmp/sdk-runtime-results.json
```

最后一个命令需要允许监听本机端口；先前升级审批超时、普通沙箱禁止 bind 的阻塞现已解除，正式 Claude/Pi worker 本机 HTTP 10/10 已补验，见 [HTTP/原生记录](../dogfood/craft-v014/m2-http-native-progress.md)。实际 SDK/HTTP 与确定性模型 fixture 组合不抵扣真实账户、TLS 或 OAuth 验收。历史监督证据见 [M2 worker 记录](../dogfood/craft-v014/m2-worker-progress.md)，生产接线证据见 [运行桥记录](../dogfood/craft-v014/m2-core-bridge-progress.md)。
