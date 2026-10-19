# Craft v0.14.0 重建 M0 验证记录

- 日期：2026-09-30，Asia/Shanghai。
- 状态：**M0 实施中，未达到完整里程碑出口**。本文件是 2026-09-30 的切片记录；全局状态以 [计划第 24 节](../../plans/2026-09-30/13:17-craft-parity-rebuild.md) 为准。
- Loom 基线：`5e90f550065ed00dc09304dc2692b580ebd61a7d` + 当前工作区；实施分支 `codex/craft-parity-rebuild`。
- 参考 SHA：`73bd9c2a3573158bea880984eb8d5fdb41e0cac2`。

## 已获得的证据

1. 参考仓库 `bun install --frozen-lockfile --ignore-scripts` 完成；官方 Electron 下载器安装 39.2.7。main/preload/renderer/resources/assets 构建成功。renderer 存在大 chunk 警告，不视为性能通过。
2. 经 `LOOM_PREVIEW_TARGET=craft-reference scripts/debug.sh desktop` 启动真实参考桌面，Vite 固定 1420，配置与 Chromium profile 在 `.ref/state/craft-v0.14.0/`。空凭据库防止上游首次运行自动迁入默认 Craft 凭据。
3. CUA 实查首次引导→Setup later→收件箱→新会话→App/AI 设置→Sources；保存 [截图元数据](../../../designs/craft-v0.14.0/screenshots.json)。参考会自动发现 global skills，本轮未调用它们或将内容发给模型。
4. [源码清单](../../../designs/craft-v0.14.0/inventory.json)记录 23 功能域、35 session 工具、11 设置组件、3 provider 类型、8 auth 类型及 4 permission 模式。生成器校验固定 SHA 和源码 hash；仅代表静态盘点，不是运行覆盖率。
5. [SDK 桥接结果](sdk-bridge-results.json)：真实 Claude Agent SDK 0.3.280 与 Pi coding-agent 0.87.1 各运行 allow/deny/cancel/stale/disconnect，共 10/10 通过。Rust 在收到审批请求时确认文件未写，匹配请求后才放行；测试直接读取 `probe.txt` 核对副作用。旧 turn 回应被拒，随后有效回应才写入。
6. Rust/CEF 宿主已构建为 macOS app；当前回环自动验证包括加载/CDP AX/表单/隐藏恢复/原生关闭隐藏/持久化/正常退出/重启，8 项通过，见 [Browser 结果](browser-probe-results.json)。只代表本机 prototype，完整窗口 chrome、多 Workspace、远程桥、下载、生产签名仍未交付。
7. CUA 验证了原生英文输入、中文粘贴+回车、链接导航；修正初版缺少原生 AX 和 Edit 菜单转发的问题。当前中文结果与 AX 见 [截图](../../../designs/browser-host-m0/native-chinese-input.jpg)、[AX](../../../designs/browser-host-m0/native-chinese-input.ax.txt)。另在当前产物通过 Cmd+Q 真实退出，关联调试进程返回 0。窗口关闭隐藏与应用退出已区分；这不是实际输入法 composition 全覆盖。
8. provider worker TypeScript typecheck 通过；Rust 绑定校验单测、fmt、Clippy `-D warnings` 通过；debug 进程识别现有 2 项回归及脚本语法检查通过。

## 关键发现

- SDK 的 pre-tool hook/执行器包装可以真实往返 Rust；无需把“发送前确认”假装成逐工具审批。
- 这里调用的 SDK 和 Write/write 执行器是真的，模型 HTTP 回复是回环 fixture。**未验证真实模型、账户/OAuth、其它工具、MCP、OS 沙箱、恢复、多平台**；不能把 10/10 当成 F08/F09 全部完成。
- 首次界面没有账户也能进入工作台；空新会话离开后未保留在列表，需要在正式对齐案例中核验该生命周期。
- 源码有 11 个设置组件，但本地空账户状态只展示其中部分导航。条件可见性需逐项验证，不能按目录数量画固定菜单。
- 参考 Browser 通过 Add panel → New Browser Window 创建，主窗口出现 browser tab/窗口入口。完整窗口行为和截图尚待补齐。
- 预览脚本曾在端口被执行沙箱阻止时错误报告启动成功，已修正为验证监听成功后才报告成功；真实启动获环境授权后通过。

## 未完成的 M0 工作

- 完整正常/异常/恢复子案例、固定尺寸和深浅主题的截图基线；现有截图是初始观测，其中新会话尺寸不同，已标记重采。
- SDK 的 Read/Bash/MCP、超时、退出/子进程回收及 resume/steering 实验；完整能力声明和正式协议尚未交付。
- BrowserHost：已选取官方 `tauri-apps/cef-rs` 作为 Chromium/Rust 原生窗口候选；已完成隔离 profile、CDP/AX、输入、窗口隐藏与重启验证。Tauri 当前 2.11.1 默认 WebKit，不能直接重用 Electron BrowserView。
- [第一方服务与功能开关清单](../../architecture/rebuild-services.md)已完成初版源码核对；服务实现/部署、浏览器与 SDK 的最终架构决策和全部平台差异仍待完成。
- 以上「未完成」清单停在 2026-09-30。当时 M2–M8 尚未实施；此后已有 M1/M2 切片，本清单不回写。新应用和完整复刻仍未交付，见计划第 24 节。

## 重复执行

```sh
node scripts/craft-inventory.mjs --check
bun run --cwd packages/provider-workers typecheck
cargo test --manifest-path experiments/runtime-bridge/Cargo.toml --offline
cargo clippy --manifest-path experiments/runtime-bridge/Cargo.toml --offline -- -D warnings
cargo build --manifest-path experiments/runtime-bridge/Cargo.toml --offline
python3 scripts/runtime-bridge-smoke.py --output .tmp/runtime-bridge-results.json
node --test tests/unit/debugScript.test.cjs
```

模拟模型服务需要本机回环监听；执行沙箱不允许时走环境审批，不能把权限错误记录为产品失败或跳过真实执行。报告中的临时路径已替换为 `$FIXTURE`。
