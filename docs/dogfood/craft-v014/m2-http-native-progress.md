# M2 新 worker HTTP 与原生 SDK 验收

- 工作日期：2026-10-10，Asia/Shanghai。机器采集时间保留在 JSON 中。
- 基线：`f5a01c1` 加本轮脚本与文档修改。
- 范围：补齐正式 Claude/Pi worker 的本机 HTTP 证据，验收连接表单与 Claude SDK 原生发送、拒绝、停止、再发、允许和历史恢复。完整 M2 仍未达到出口。
- 当前全局状态以 [计划第 24 节](../../plans/2026-09-30/13:17-craft-parity-rebuild.md#24-当前残差) 为准。

## 修复

- `scripts/debug.sh` 的 `workbench-bundle` 分支原先在 Bun 初始化前执行应用，导致设置 `LOOM_WORKBENCH_SDK=1` 后仍缺少宿主 Bun 路径。将初始化和绝对路径校验移到分支之前；无效路径不启动应用。
- HTTP smoke 提取共用的本机模型 fixture，并增加 `--serve-fixture`，用于人工操作原生应用。固定监听 `127.0.0.1`，只记录请求路径、模型和是否流式，不记录凭据或提示内容。测试目标由启动者指定，UI 仍通过 Rust 策略审批。
- 上轮计划增加验收列后，静态 inventory 解析器会把 status/evidence 一并读入 acceptance。修正解析器，只提取原有四列；固定基线 inventory 无需改写，`--check` 已通过。

## 验证

| 检查 | 结果与范围 |
| --- | --- |
| 正式 worker 本机 HTTP | Claude/Pi 各 allow、deny、cancel、stale、disconnect，共 10/10。当前 Rust probe 重新构建后运行；前四项经生产 core/runtime/supervisor，disconnect 直接关闭真实 worker stdin。逐项检查工具副作用、审批和终态。[HTTP 结果](m2-http-native-results.json) |
| 原生连接页 | 保存合成 API key，保存后凭据字段为空；重命名且留空保留凭据；重启重新选中连接后名称、端点、模型和已保存状态仍在，凭据不回显。SDK 随后成功读取该连接对应的 Keychain slot。 |
| 原生首轮拒绝 | 使用真实 Claude SDK 和本机 HTTP fixture；审批出现前文件不存在，拒绝后仍不存在，审批为 denied，回合为 completed。工具被拒绝与回合结束分别记录。 |
| 原生第二轮停止 | 待工具审批时点击停止；文件仍不存在，审批 cancelled、回合 cancelled、无活动回合。 |
| 原生第三轮再发与允许 | 停止后再次发送；出现新的审批，允许一次后文件精确为 `loom-sdk-probe\n`，审批 consumed、工具 succeeded、回合 completed。 |
| 原生重启历史 | 正常停止后重启同一隔离数据目录，三轮输入、完成/停止状态、成功工具和 3 条审批历史可读。此项是 journal 恢复，不证明 SDK native resume。 |
| 前端/脚本定向回归 | `pnpm test -- tests/unit/debugScript.test.cjs tests/unit/connectionSettings.test.cjs tests/unit/workbenchSdk.test.cjs`，6/6。新增 bundled SDK 路径初始化及无效路径拒绝回归。 |
| 脚本检查 | Bash 语法、Python 编译、静态 inventory 校验通过。 |

原生验收沿用已有 Workbench debug app，本轮没有修改 Rust/React 应用代码或重建发行包；机器摘要保存实际使用的 app/probe hash、来源 hash、持久化终态与文件 hash，见 [证据清单](m2-http-native-evidence.json)。截图及 AX 在 [原生资源目录](../../../designs/workbench-m2/http-native-2026-10-10/)，其中 [连接保存](../../../designs/workbench-m2/http-native-2026-10-10/01-connection-saved.png)、[停止](../../../designs/workbench-m2/http-native-2026-10-10/04-sdk-stopped.png)、[允许](../../../designs/workbench-m2/http-native-2026-10-10/05-sdk-allowed.png) 和 [重启](../../../designs/workbench-m2/http-native-2026-10-10/06-sdk-restarted.png) 均留存。截图已检查；没有录屏。

## 复现

自动 worker 验收：

```sh
cargo build -p loom-runtime --example sdk_runtime_probe --locked --offline
python3 scripts/runtime-bridge-smoke.py --runtime --output .tmp/m2-http-native/runtime-http-results.json
```

原生测试在两个终端分别启动模型 fixture 和应用；使用新的隔离目录与合成凭据，不选真实模型连接：

```sh
mkdir -p .tmp/m2-http-native/tool-target
python3 scripts/runtime-bridge-smoke.py --serve-fixture --provider claude \
  --fixture-directory .tmp/m2-http-native/tool-target --port 18765 \
  --output .tmp/m2-http-native/native-http-requests.json
```

```sh
scripts/debug.sh stop
LOOM_PREVIEW_TARGET=workbench-bundle LOOM_WORKBENCH_SDK=1 \
  LOOM_WORKBENCH_DATA_DIR="$PWD/.tmp/m2-http-native/data" scripts/debug.sh desktop
scripts/debug.sh stop
```

连接使用 `http://127.0.0.1:18765`、模型 `claude-sonnet-4-6` 和任意合成 API key；Ask 模式按上表操作。首次副作用测试要求 `tool-target/probe.txt` 不存在，重跑需另用新目录。结束后停止应用并向 fixture 发送 Ctrl+C，使请求摘要写盘。本轮全部实例已正常退出，隔离数据和合成 Keychain slot 保留用于复查；未修改用户原有连接或凭据。

## 剩余范围

Pi 追加原生补验：新建 `pi_compat` / OpenAI Chat Completions / 无认证连接，首轮通过原生 UI 拒绝写入，文件不存在；第二轮审批等待期间 Mac 锁屏，CUA 无法操作停止按钮，已请求手动解锁。使用 `scripts/debug.sh stop` 正常退出并撤销待审批，fixture 也已退出，机器摘要保留两轮状态。该退出仅证明宿主清理，Pi 原生停止/再发/允许仍未通过，不与 Claude 的三轮结果合并。

环境审批已成功执行，先前 Claude 新 worker/本机 HTTP 阻塞解除。HTTP 是真实传输，模型响应是确定性 fixture；公网模型账户、TLS/OAuth/服务认证和模型质量未验证。Claude 原生三轮已补验；Pi 原生拒绝已有证据，停止/再发路径因锁屏仍 blocked。

resume/compact/steer、其他认证类型、管理员/Guarded、权限热更新、全工具能力、Windows/Linux、发行资源和 M0/M1 其它残差继续。所有 F 行仍未完整验收，不因 10/10 或三轮本机请求标为 passed。
