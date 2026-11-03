# Research 多目标 R0 探针与 R1 执行配置

状态：R0 的 REA／App、网站、CLI 三个子门禁通过；R1 仅无工具执行配置已实现，其他里程碑未完成，尚不能通知产品验收。

## 基线与工具

- 工作区：`main`，HEAD `8e56fbdfc3d101348884c091ee0aad3d876681a7` 加已有未提交改动；本次未重置或提交用户改动。
- 实际固定 npm 包：`rea-agents@6.8.0`，MIT，官方发布包 integrity 已与安装 lock 核对。
- 运行时：下载官方 Node `26.12.1` darwin-arm64 并核对官方 SHA-256，仅用于 `/private/tmp/loom-research-r0-tools/`；系统 Node `26.11.0` 未升级。安装禁用 lifecycle scripts，未执行 `rea setup`，未修改用户 MCP 配置。
- REA 包要求 `^22.23.4 || ^24.22.1 || >=26.12.1`。旧计划中的 6.5.0／207 工具不能复用为新契约。
- 实际 MCP：协议 `2025-06-18`，server `rea@6.8.0`，229 tools、6 prompts；固定结果在 [契约 fixture](../../tests/fixtures/research/rea-6.8.0.json)。服务器 combined catalog digest：`4748bd4d8c81aaecf7fceceecfbaf14302cab6cb99cd878c0141b30b9eb577b2`。
- CLI：Claude Code `2.1.311`；真实通过的模型为本机已配置路由 `auto/gpt-6.1-sol[1m]`。这是通过 Claude CLI 调用，不是已验证 Codex CLI 的无工具能力。

## 真实 REA 验证

可复现入口 [research-target-probe.mjs](../../scripts/research-target-probe.mjs)，参数必须显式指定 Node、REA 和输出目录；网站子门禁另指定 Chrome。它只创建受控 fixture，不分析用户已安装的私人 App。

```sh
node scripts/research-target-probe.mjs \
  --node /private/tmp/loom-research-r0-tools/node-v26.12.1-darwin-arm64/bin/node \
  --rea /private/tmp/loom-research-r0-tools/node_modules/rea-agents/scripts/rea.mjs \
  --chrome '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' \
  --output /private/tmp/loom-research-r0-accepted
```

重复时选择全新输出目录；REA 导出拒绝覆盖现有文件。临时工具会被系统清理，复跑前需先核实存在与版本。

| 检查 | 实际结果 |
|---|---|
| MCP 初始化、完整目录、binary_session | 通过，身份与 catalog 已保存 |
| 本地 App 静态识别与 JS 分析 | 通过；受控代码含写文件语句，但未执行，源文件不变 |
| 不存在目标 | 返回工具错误，没有生成假成功证据 |
| 网站观察 | 通过；独立无登录态 headless Chrome、本机临时 HTTP fixture，按 origin 与 target ID 观察 |
| Evidence 导出 | 通过；目录探测、JS summary、页面观察均有 Evidence ID，完整 bundle 落盘 |
| 超限响应 | 通过；第二个真实 MCP 连接采用 128-byte frame cap，拒绝超限响应并回收进程 |
| 取消 | 通过；对 200 文件分析发送取消，随后宿主回收并确认进程组不存在，约 1316 ms |

最终探针耗时约 5.8 秒。输出摘要和 artifact hash 清单见 [REA 结果](samples/research-targets-2026-10-28/rea-r0.json)、[产物清单](samples/research-targets-2026-10-28/artifact-manifest.json)。完整原始产物留在上述临时目录，没有将任意目标正文放进仓库。

## CLI 无工具验证

[research-cli-probe.mjs](../../scripts/research-cli-probe.mjs) 在隔离目录设置不可见 canary、项目 hook 与项目 MCP 诱饵，要求模型尝试读取／写入／工具调用，并完成纯文本算术。真实事件验证工具和 MCP 列表均为空，无工具事件、hook/MCP 标记或文件写入，canary 未泄漏，最终文本返回预期结果。

```sh
node scripts/research-cli-probe.mjs \
  /private/tmp/loom-research-cli-r0-sol 'auto/gpt-6.1-sol[1m]'
```

调用配置：`--safe-mode --tools '' --strict-mcp-config --mcp-config '{"mcpServers":{}}' --no-session-persistence --no-chrome --disable-slash-commands --permission-mode plan --print --verbose --output-format stream-json`，提示词通过 stdin；无需修改用户全局配置。通过摘要见 [CLI 结果](samples/research-targets-2026-10-28/claude-evidence-only.json)。

R1 已将这组参数接入 `agent/evidence.rs`：私有构造的验证凭据绑定可执行文件与 Agent 配置，当前只接受已测试的 Claude 版本；拒绝额外 argv、续聊、可写模式及版本／配置漂移。未支持适配器不会回退到普通 explore。普通 Chat／Learn／Research／Discuss 的原调用显式传 `execution_profile: None`，保留原行为。

真实 Rust 验收通过生产 `prepare_invocation → agent::run`，再次检查 init 中工具／MCP 为空、无工具事件及正确模型回答：

```sh
LOOM_REAL_EVIDENCE=1 LOOM_EVIDENCE_MODEL='auto/gpt-6.1-sol[1m]' \
  cargo test --manifest-path src-tauri/Cargo.toml \
  agent::evidence::tests::real_evidence_profile_through_agent_runner \
  -- --ignored --exact --nocapture
```

结果：1 passed，约 8.15 秒。定向 Agent 单测：36 passed、1 ignored；TypeScript 和分层检查通过。完整产品门禁仍由 R7／E2 执行，不能由这些结果替代。

共享层回归：Rust 全库首次在沙箱中为 326 passed、3 failed、31 ignored；失败均为既有 Skills 测试的本机端口绑定／进程组信号权限错误。仅将这 3 项切换到真实宿主重跑，全部通过；合计 329 项非 ignored 测试通过，不描述为单次全库全绿。Research 前端 4/4、Clippy all-targets/all-features `-D warnings`、TypeScript、分层检查和 diff 空白通过。

## 失败与实际限制

1. 首次沙箱内 `binary_session` 的 Swift 进程归属辅助程序编译失败，原因是缓存目录写权限；真实宿主运行通过。生产预检需保留此类失败，不把工具目录可读当成所有能力可用。
2. 取消通知后的 MCP 请求可能没有响应。最初等待响应的探针在 120 秒超时，已修正为“通知 → 短宽限 → 回收 → 确认进程组退出”。取消回复不是完成判据；本次取消未证明有可恢复的新增 partial Evidence。
3. 当前 Claude 默认路由 `auto/glm-5.3[1m]` 返回 HTTP 410，模型于当天下线；见 [失败摘要](samples/research-targets-2026-10-28/claude-default-unavailable.json)。没有改用户默认模型。
4. 已配置 DeepSeek 路由虽未真实调用工具，但生成模拟工具文本且未完成要求，因此没有计为通过；模型可用性与工具隔离须分别验收。
5. Codex／OpenCode／Grok 尚未通过无工具 profile，不声明支持；Codex 官方配置仅证明可分别关闭部分工具，不能由此推断“所有工具都禁用”。参考 [官方配置](https://developers.openai.com/codex/config-reference)。
6. 本轮受控 JS App 与本机网页是实际 REA／浏览器运行证据，不是任意第三方 App、登录态网站或原生反编译能力的证明。

## 文档与代码差异处理

- 计划 §4.2 的 `evidenceOnly` 原为目标，已新增独立执行凭据与参数路径；只有这部分进入实现，其余 R1 契约不改成“已完成”。
- `src/domain/agent.ts` 中 modelSelection 注释仍称只有 Learn 使用，实际 Research／Discuss 已使用；本轮修正注释，不修改对应功能行为。
- 旧 6.5.0 catalog 与当前包的工具数／digest 不同；保留冻结 fixture，新建独立 6.8.0 fixture，不覆盖历史证据。
