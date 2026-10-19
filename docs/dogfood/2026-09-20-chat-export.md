# Chat 导出与回滚格式验收（2026-09-20）

基线：`0fe5f79` 上当前未提交工作树，对应 [主计划 §19](../plans/2026-09-17/10:15-local-agent-chat-rebuild.md#19-m13-导出与回滚2026-09-20)。本轮仅执行本地文件/组件/构建测试，没有外部模型调用。

## 实现

Rust `chat_export` 在已登记项目 `.loom/chat/exports/<exportId>/` 输出 `session.json`、`transcript.md`、`manifest.json` 和可用的分流日志。后台文件线程执行，目录 ID 由后端生成，不接受任意目的地、不覆盖旧产物。先写 staging，全部成功后 rename 发布；正常失败清理 staging。

JSON 保留所选已提交序号的完整会话模型，Markdown 使用动态代码围栏保留原文，日志按分别捕获的完整换行前缀复制。运行中、日志尾部不完整、日志丢失/损坏都在结果和 manifest 明示，不将这些情况伪装为最终完整执行。日志副本使用相对路径，整个目录可移动。

会话菜单新增导出入口。结果提示路径、快照序号及证据缺失；用户显式点击后才定位文件夹。跨会话迟到响应不污染当前页，失败可重试，切回旧会话不遗留 busy。旧版 v1 文件保持原样；导出的 v2 JSON 不是旧版导入文件。

## 验证

`CARGO_NET_OFFLINE=true pnpm check` **通过**：前端 **169**、Rust **281**（5 ignored）、工作流集成 **2**，包含 fmt/Clippy/build 与发布文档检查。5 个 ignored 中包括本轮新增的离线样本生成器，其余保持既有显式运行边界。日志 `/tmp/loom-export-check.log`。

`CARGO_NET_OFFLINE=true pnpm tauri build --no-bundle` **通过**，默认名称仍为 Loom。日志 `/tmp/loom-export-desktop-build.log`。未替换或重启用户正在操作的 Loom.app，当前二进制不是已通过签名/公证的发布候选。

生产仓储/导出测试覆盖：

- 已归档会话的 JSON 投影与源模型逐字段一致，消息/分段/回执/回合/续聊/游标全部保留；导出不追加会话事件。
- Markdown 保留中文、原始空白和不同长度的反引号，实际 Markdown 解析后的 HTML 不执行原文 script。
- 日志仅复制完整行前缀，manifest 的字节边界与产物匹配；运行中导出后新增的消息和日志不会回写旧产物。
- 缺失/损坏/symlink 日志明确标错；导出根 symlink 和非法 ID 不可越界。损坏的回合 ID 使 staging 写入失败时清理临时目录，不发布成功目录。
- 重复导出到相同 ID 拒绝覆盖，不同 ID 可独立生成；v1 迁移后导出仍逐字节保留原始 v1 文件，不补造旧回合日志。

最后一项 staging 清理断言在全量检查之后补入同一测试，再运行 `cargo test ... chat::export`：**4 passed、1 ignored**，日志 `/tmp/loom-export-final-tests.log`。

生产 UI/hook/wrapper 测试覆盖实际 `chat_export` IPC 信封、重复点击合并、运行中/缺失证据提示、显式文件夹定位、错误与重试，以及 A/B 切换和迟到结果隔离。

## 可审阅产物

离线生成器实际运行 **1 passed**（日志 `/tmp/loom-export-sample.log`）；由生产 exporter 输出的五个文件已逐字节复制并校验到 [回滚样本](samples/README.md)。该样本是明确的合成数据，仅证明格式/复制可读性，不是模型执行证据。两份合成 `.log` 已有精确的 gitignore 例外，避免提交样本时漏掉日志。

[回滚指南](../guides/chat-export-rollback.md) 明确停止回合、检查 manifest、备份整个 `.loom/chat/`、旧版只读原 v1、返回新版时历史不自动合并的流程与限制。

通过 `scripts/debug.sh web` 和 CUA 查看 [生产组件视觉样例](../../designs/chat-export-preview.html)：实际展开会话菜单、点击导出入口并观察结果、路径、运行中提示与缺失日志列表，布局无截断。样例响应是明确标注的本地合成数据，没有写文件或调用 Agent，不能当成桌面 IPC 验收。截图在任务工具输出中；没有虚构独立截图文件。

## 未完成门禁

真实权限/工具/停止及完整应用退出、长历史、Chat Shell、发布签名/公证/安装/实际桌面回退仍待完成。此前真实 Grok 重跑因私人 skill 可能外发被自动审批拒绝，尚待用户明确授权；本轮未尝试绕过。原生桌面导出入口也需在最终候选上复验。
