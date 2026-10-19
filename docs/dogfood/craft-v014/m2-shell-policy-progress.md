# M2 Rust Shell 权限分类阶段记录

- 日期：2026-10-09，Asia/Shanghai。
- 基线：`1ab2fe4` 后的本切片；代码与用例哈希见 [证据](m2-shell-policy-evidence.json)。
- 范围：计划第 20 节的 Bash 分类基础。完整 policy 与正式桌面调度仍实施中。

## 已实现

`crates/loom-runtime/src/shell.rs` 使用 Tree-sitter Bash 语法树分类，不运行命令。逐个验证命令链、管道、子 Shell、命令组、否定、引号和安全重定向；不允许环境赋值、后台执行、变量/命令/进程替换或未知控制流通过只读分类。正确处理续行与注释边界，不能让注释中的反斜杠吞掉下一行命令。

`shell/arguments.rs` 检查 Shell 语法树看不见的程序参数能力：find、awk、sed、sort、gh api，并补充 rg 预处理。自定义允许正则仍不能绕过语法和这些参数检查。正则支持默认规则所需的前瞻和回溯引用，针对 JS 的 ASCII 单词/数字分类作转换；不是完整 ECMAScript 正则引擎。

输入最多 64 KiB、语法嵌套最多 128 层、解析期限 250 ms、规则数/长度/编译大小和回溯次数均有界。错误不回显原始命令。`plain_command` 只返回可用于后续记住授权的单命令 argv；它本身不授予任何权限。

复制固定版本默认规则为编译内资源，不依赖 `.ref` 运行。上游 CLI 尚未在 Loom 接入，默认构造器可以显式关闭其规则。新增依赖固定为 tree-sitter 0.25.10、tree-sitter-bash 0.25.1、fancy-regex 0.16.2；原因是 Rust 内的语法分类及默认规则兼容。保留 [许可证和来源](../../../third-party/craft-agents/README.md)。

## 实际验证

- `bun scripts/generate-craft-shell-cases.ts`：调用本地固定 SHA 的真实上游 `validateBashCommand`/`parseSimpleCommand`，生成 **141 个分类与 argv 用例**，不是手工猜测 expected。生成器只解析字符串，不执行用例中的命令。
- Rust 差分测试对照上述结果，除下列显式登记的安全修正外一致。用例文件：`crates/loom-runtime/tests/fixtures/craft-shell-cases.json`，包含上游 SHA、源码和默认规则哈希。
- 4 个新增测试入口覆盖差分、plain argv、不可被宽松自定义规则绕过的边界、非法配置与超限输入。
- `pnpm check:core` 通过：fmt、Clippy、**95 个 workspace 测试入口**、两套协议漂移检查和协议 TS 编译。日志位于 `.tmp/shell-policy-validation/core-check.log`。
- 对续行后的 `#` 字面值使用 `/bin/bash --noprofile --norc` 和内置 printf 做无文件副作用核验，确认 argv 为 `a#word`。

## 与上游的显式差异

本计划不复刻上游 bug。差分数据保存上游原结果以及 Loom 的目标结果，未覆盖上游结果。

| 用例 | 上游结果 | Loom / 原因 |
| --- | --- | --- |
| 输入重定向目标为命令替换（两种语法） | 允许 | 拒绝；读取前会执行嵌入命令。 |
| 输入 FD 重定向目标为环境变量 | 允许 | 拒绝；与其他位置的变量展开一样，不可绕过参数检查。 |
| `find -fprint` | 允许 | 拒绝；会写文件，同类输出 flag 一并检查。 |
| awk print 重定向 | 允许 | 拒绝；Shell 引号不会阻止 awk 自己写文件。 |
| `rg --pre` | 允许 | 拒绝；会执行预处理程序。 |
| 续行连接已有单词与 `#word` | argv 丢失 `#word` | 保留 `a#word`，与 Bash 实际字面参数一致。 |

## 剩余工作

- 这只是 Shell 分类模块，尚未作为完整 `ToolPolicy` 接入桌面；没有给用户会话启用新执行权限。
- 工作区/Source 规则存储、读取与热更新、路径例外、记住授权、管理员批准、完整 Guarded 决策仍待接入。
- PowerShell 与 Windows 路径语义还未实现；本记录不声明 Windows 完成。
- 141 个用例证明所列行为，不证明任意 Shell/程序参数均无副作用，也不等于 OS sandbox。后续接线须继续对固定版本完整权限 inventory 验收。
- 正式桌面调度、账户、M2 其他能力继续实施；真实 HTTP 与原生连接界面的先前环境阻塞未解除。

