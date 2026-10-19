# M1 旧数据只读预检验收

日期：2026-09-30。基线：`3972b0c` 加本轮修改；最终源码和应用哈希见 [机器记录](m1-legacy-preview-evidence.json)。这是 M1 迁移的预检切片，**实际导入尚未实施**。

## 已实现

- 新内核独立读取项目内 Chat v1 JSON、v2 journal 与 Task v1 裸对象/封套，不调用旧仓储或迁移函数。
- 日志回放、版本/作用域/序号验证、收据/回合不可变检查；v2 损坏不降级读取同 ID 的 v1。未提交残尾和过时快照只报告，不修复。
- 逐项读取结果、SHA-256/预检指纹、引用资源缺失清单；已登记项目的 Workspace 归属检查、Unix 描述符逐层只读访问、符号链接/越界/FIFO/超预算拒绝。
- 生产 `preview_legacy_import` 协议与生成 TS 类型；新工作台项目下的报告弹窗，包含重新检查、损坏项和缺失资源展开。没有导入成功提示或执行按钮。

## 验证

| 检查 | 结果与范围 |
| --- | --- |
| `pnpm check:core` | 最终通过；27 个原内核测试入口 + 13 个迁移预检测试，fmt/Clippy/生成类型漂移/协议 TS 编译通过。 |
| 定向前端测试 | `legacyPreviewDialog`、`workbenchApp`、`workbenchClient` 共 5 项通过；覆盖迟到响应隔离、源码文本转义、重试、原草稿/IME 与 Channel 行为。 |
| `pnpm build` | 通过；修复弹窗高度后又通过原生 bundle 的 `beforeBuildCommand`。 |
| Tauri workbench 测试 | 5 项通过；这次在最后的项目根符号链接强化前运行，强化后由 core 13 项和最终桌面 bundle 复验。 |
| macOS debug app bundle | `VITE_LOOM_WORKBENCH=1 CARGO_NET_OFFLINE=true pnpm tauri build --debug --bundles app` 通过。未验证发布签名/公证。 |
| 原生 UI + 真正 Rust IPC | 隔离合成项目返回可读取 2 项、无法读取 1 项；缺失附件、未提交残尾、旧权限重置说明均可展开；关闭焦点、重复检查、Escape 和 Cmd+Q 通过。 |
| 文件与新仓库回读 | 三份源文件验收前后 SHA-256 相同；重复预检与真实 UI 结果一致；新工作区 Session 数仍为 0。 |

首次截图发现报告区被 flex 布局压缩为 60px；已修复为限定弹窗高度内滚动，并重新打包、启动和截屏核对。[最终截图](../../../designs/workbench-m1/05-legacy-preview.jpg) 与 [AX 树](../../../designs/workbench-m1/05-legacy-preview.ax.txt) 已保存，截图的白色边缘是原生捕获结果，不作为像素对齐证据。

预览使用 `scripts/debug.sh`；开始和结束执行 stop。受沙箱限制的首次直接启动退出，随后以获批的沙箱外启动运行隔离数据目录。最终应用已退出。未调用模型或外部服务，未读取用户真实旧项目数据。

## 未完成

- 持久化预检计划、执行前来源复查、staging/原子发布、幂等 ID、失败与中断恢复。
- 导入历史转录、Task 历史资源和附件，以及可阅读与可回退的产物。
- 独立导出包源格式、Linux 实机验收、Windows 安全文件读取。当前非 Unix 平台返回明确不支持。
- 大型历史的增量读取。当前预算为单文件 32 MiB / 单次累计读取 128 MiB / 10,000 目录项；超限明确报告。

以上限制不会通过把预检称为迁移完成而省略；M1 和整体 M0–M8 保持实施中。
